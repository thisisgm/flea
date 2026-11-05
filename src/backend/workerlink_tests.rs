use super::*;
use crate::backend::testdir::TestDir;
use crate::backend::thumbs::THUMB_SIZE;
use std::process::Command;

// A stand-in jail that was never sandboxed, so retiring it kills only its own child.
fn dummy_jailed(child: std::process::Child) -> crate::backend::jail::Jailed {
    crate::backend::jail::Jailed {
        child,
        sandbox_pid: std::sync::Arc::new(std::sync::atomic::AtomicI32::new(crate::backend::jail::GONE)),
    }
}

// Stands in for the worker: answers the one request it is sent with `verdict`, on the reply socket that request carried.
/// Reply to one worker request with a controlled decoder verdict.
fn answered_by(verdict: u8) -> (WorkerLink, std::thread::JoinHandle<()>) {
    let (mine, theirs) = fdpass::pair().unwrap();
    let child = Command::new("true").spawn().unwrap();
    let link = WorkerLink { state: Mutex::new(State::Running { jailed: dummy_jailed(child), requests: mine }) };
    let answering = std::thread::spawn(move || {
        let request = fdpass::recv(theirs.as_raw_fd()).unwrap().expect("a request");
        let reply = request.fds.into_iter().nth(2).expect("a reply socket");
        fdpass::send_byte(&reply, verdict).unwrap();
    });
    (link, answering)
}

/// Check which verdicts publish a thumbnail, retry, or retire the worker.
#[test]
fn only_a_thumbnail_is_final_and_a_machine_failure_retires_the_worker() {
    let dir = TestDir::new("worker-verdicts");
    let input = dir.file("clip.mp4", "not a video");
    let output = dir.file("out.png", "");
    for (verdict, published, keeps_serving) in [(SUCCEEDED, true, true), (FAILED, false, true), (NOT_STARTED, false, false)] {
        let (link, answering) = answered_by(verdict);
        let got = link.generate(&input, &output, THUMB_SIZE, true, Duration::from_secs(1));
        answering.join().unwrap();
        assert_eq!(matches!(got, Some(Ran::Succeeded)), published, "verdict {}", verdict as char);
        assert_eq!(got.is_none(), !published, "verdict {} must send the job down the exec path", verdict as char);
        assert_eq!(matches!(*link.state.lock().unwrap(), State::Running { .. }), keeps_serving, "verdict {}", verdict as char);
    }
}

// A non-blocking open returns at once; this bounds a regression to a failure rather than a hung suite.
const HUNG: Duration = Duration::from_secs(5);

/// Register a job as it would be after leaving the pool queue.
fn pending_video(path: &Path) -> (crate::backend::state::State, identity::Shared) {
    let (events, _received) = crate::backend::events::channel();
    let mut st = crate::backend::state::State::new(crate::backend::dirsizeworker::Worker::new(events));
    st.thumb_mtimes.insert(path.to_owned(), 1);
    let shared = identity::start(&mut st, path.to_owned(), 1, None);
    assert!(!identity::cancel_video(&st, path), "exec-only work must stay ineligible");
    identity::begin_video(&shared);
    (st, shared)
}

/// A cancellation before socket registration never sends a decode request.
#[test]
fn a_pending_video_cancel_prevents_dispatch() {
    let dir = TestDir::new("worker-cancel-before-arm");
    let input = dir.file("clip.mp4", "not a video");
    let output = dir.file("out.png", "");
    let (st, shared) = pending_video(&input);
    let (mine, theirs) = fdpass::pair().unwrap();
    let child = Command::new("true").spawn().unwrap();
    let link = WorkerLink {
        state: Mutex::new(State::Running { jailed: dummy_jailed(child), requests: mine }),
    };
    assert!(identity::cancel_video(&st, &input));
    assert!(link.generate_for(&input, &output, THUMB_SIZE, false, HUNG, Some(&shared)).is_err());
    assert!(matches!(read_byte(&theirs, Duration::ZERO), Heard::Silence));
}

/// A success reply racing an accepted cancellation cannot publish the old thumbnail.
#[test]
fn an_active_video_cancel_wins_over_a_successful_reply() {
    let dir = TestDir::new("worker-cancel-before-success");
    let input = dir.file("clip.mp4", "not a video");
    let output = dir.file("out.png", "");
    let (st, shared) = pending_video(&input);
    let (mine, theirs) = fdpass::pair().unwrap();
    let child = Command::new("true").spawn().unwrap();
    let link = WorkerLink {
        state: Mutex::new(State::Running { jailed: dummy_jailed(child), requests: mine }),
    };
    std::thread::scope(|scope| {
        let (sent, ready) = std::sync::mpsc::channel();
        let fake = scope.spawn(move || {
            let mut incoming = PollFd { fd: theirs.as_raw_fd(), events: POLLIN, revents: 0 };
            assert_eq!(unsafe { poll(&mut incoming, 1, HUNG.as_millis() as i32) }, 1);
            let request = fdpass::recv(theirs.as_raw_fd()).unwrap().unwrap();
            let reply = request.fds.into_iter().nth(2).unwrap();
            sent.send(()).unwrap();
            assert!(matches!(read_byte(&reply, HUNG), Heard::Byte(crate::backend::thumbworker::CANCEL_VIDEO)));
            fdpass::send_byte(&reply, SUCCEEDED).unwrap();
        });
        let generate = scope.spawn(|| link.generate_for(&input, &output, THUMB_SIZE, false, HUNG, Some(&shared)));
        ready.recv_timeout(HUNG).unwrap();
        assert!(identity::cancel_video(&st, &input));
        fake.join().unwrap();
        assert!(generate.join().unwrap().is_err());
    });
}

/// Check that a symlink at the output path is refused before dispatch.
#[test]
fn arm_oflags_output_symlinks_never_reach_the_worker() {
    let dir = TestDir::new("arm-oflags-worker-output");
    let input = dir.file("clip.mp4", "not a video");
    let target = dir.file("target.png", "leave this file alone");
    let (mine, theirs) = fdpass::pair().unwrap();
    let child = Command::new("true").spawn().unwrap();
    let link = WorkerLink {
        state: Mutex::new(State::Running {
            jailed: dummy_jailed(child),
            requests: mine,
        }),
    };
    for name in ["target.png", "absent.png"] {
        let output = dir.join(&format!("{name}.link"));
        std::os::unix::fs::symlink(name, &output).unwrap();
        assert!(link
            .generate(&input, &output, THUMB_SIZE, true, Duration::ZERO)
            .is_none());
        assert!(
            matches!(read_byte(&theirs, Duration::ZERO), Heard::Silence),
            "output symlink reached the worker"
        );
        assert!(output.symlink_metadata().unwrap().is_symlink());
    }
    assert_eq!(
        std::fs::read_to_string(target).unwrap(),
        "leave this file alone"
    );
    assert!(!dir.join("absent.png").exists());
    assert!(matches!(*link.state.lock().unwrap(), State::Running { .. }));

    let output = dir.file("regular.png", "");
    let (link, answering) = answered_by(SUCCEEDED);
    assert!(matches!(
        link.generate(&input, &output, THUMB_SIZE, true, Duration::from_secs(1)),
        Some(Ran::Succeeded)
    ));
    answering.join().unwrap();
}

/// Check that a FIFO or vanished input cannot dispatch a video request.
#[test]
fn an_input_that_is_not_a_file_to_judge_never_reaches_the_worker() {
    let dir = TestDir::new("worker-not-a-file");
    let fifo = dir.join("swapped.mp4");
    assert!(Command::new("mkfifo").arg(&fifo).status().unwrap().success(), "mkfifo failed");
    let output = dir.file("out.png", "");
    let (mine, theirs) = fdpass::pair().unwrap();
    let child = Command::new("true").spawn().unwrap();
    let link = std::sync::Arc::new(WorkerLink { state: Mutex::new(State::Running { jailed: dummy_jailed(child), requests: mine }) });
    for input in [fifo, dir.join("vanished.mp4")] {
        let (done, answer) = std::sync::mpsc::channel();
        let (link, output, shown) = (std::sync::Arc::clone(&link), output.clone(), input.display().to_string());
        std::thread::spawn(move || done.send(link.generate(&input, &output, THUMB_SIZE, true, Duration::from_secs(1))).unwrap());
        let got = answer.recv_timeout(HUNG).unwrap_or_else(|_| panic!("generate never returned for {}", shown));
        assert!(matches!(got, Some(Ran::NotStarted)), "{} was not answered as judging nothing", shown);
    }
    assert!(matches!(read_byte(&theirs, Duration::ZERO), Heard::Silence), "a request reached the worker");
    assert!(matches!(*link.state.lock().unwrap(), State::Running { .. }), "judging nothing retired the worker");
}

#[test]
fn a_worker_that_is_not_serving_says_why() {
    assert_eq!(why_not(&Heard::Byte(NO_LIBRARY)), "libffmpegthumbnailer.so.4 did not load");
    assert_eq!(why_not(&Heard::Byte(NO_LANDLOCK)), "this kernel has no Landlock that can deny a truncation");
    assert_eq!(why_not(&Heard::Closed), "it exited before it answered");
    assert_eq!(why_not(&Heard::Silence), "it did not answer within 5 s");
    assert_eq!(why_not(&Heard::Broken(std::io::Error::other("a test error"))), "its socket failed: a test error");
}

/// Check that reply waits distinguish an answer, silence and a broken socket.
#[test]
fn a_wait_names_what_it_heard() {
    let (mine, theirs) = fdpass::pair().unwrap();
    fdpass::send(theirs.as_raw_fd(), &[READY, READY], &[]).unwrap();
    assert!(matches!(read_byte(&mine, Duration::from_secs(1)), Heard::Broken(_)), "a two-byte packet is a broken socket");
    fdpass::send_byte(&theirs, READY).unwrap();
    assert!(matches!(read_byte(&mine, Duration::from_secs(1)), Heard::Byte(READY)));
    assert!(matches!(read_byte(&mine, Duration::ZERO), Heard::Silence), "nothing was sent");
    drop(theirs);
    assert!(matches!(read_byte(&mine, Duration::from_secs(1)), Heard::Closed));
    let not_a_socket = OwnedFd::from(std::fs::File::open("/dev/null").unwrap());
    assert!(matches!(read_byte(&not_a_socket, Duration::from_secs(1)), Heard::Broken(_)), "recvmsg on a file that is not a socket fails");
}

#[test]
fn a_retired_worker_says_whether_the_machine_or_the_worker_failed() {
    assert_eq!(gone_because(&Heard::Byte(NOT_STARTED)), "failed a job on this machine");
    assert_eq!(gone_because(&Heard::Closed), "stopped answering");
    assert_eq!(gone_because(&Heard::Byte(b'?')), "stopped answering");
}

fn spec(exec: &str) -> Spec {
    Spec { exec: exec.split(' ').map(str::to_string).collect() }
}

#[test]
fn the_shipped_exec_line_is_the_worker_shape() {
    assert_eq!(worker_shape(&spec("ffmpegthumbnailer -i %i -o %o -s %s -f")), Some(true));
    assert_eq!(worker_shape(&spec("/usr/bin/ffmpegthumbnailer -s %s -i %i -o %o")), Some(false));
}

/// Keep custom thumbnailer commands on the regular exec path.
#[test]
fn any_other_program_or_flag_stays_on_the_exec_path() {
    for exec in [
        "glycin-thumbnailer -i %i -o %o -s %s",
        "ffmpegthumbnailerx -i %i -o %o -s %s",
        "ffmpegthumbnailer -i %i -o %o -s %s -t 20",
        "ffmpegthumbnailer -i %u -o %o -s %s",
        "ffmpegthumbnailer -i %i -o %o",
        "ffmpegthumbnailer -i %i -o %o -s %s -f -f",
        "ffmpegthumbnailer -i %i -i %i -o %o -s %s",
        "ffmpegthumbnailer -i",
    ] {
        assert_eq!(worker_shape(&spec(exec)), None, "{} took the worker", exec);
    }
}
