use super::*;
use crate::backend::{
    aliases::Aliases, events, state, thumbcache::Cache, thumbs, thumbspec::Thumbnailers,
};
use std::{
    io::Write,
    sync::{mpsc, Arc},
    thread,
};

fn context(path: &Path) -> (state::State, identity::Shared) {
    let (events, _) = events::channel();
    let mut st = state::State::new(crate::backend::dirsizeworker::Worker::new(events));
    st.asked.push((path.to_owned(), 0));
    st.thumb_mtimes.insert(path.to_owned(), 1);
    let shared = identity::start(&mut st, path.to_owned(), 1, None);
    identity::begin_video(&shared);
    (st, shared)
}

#[test]
fn cancellation_before_socket_arming_never_starts_a_worker() {
    let dir = TestDir::new("worker-cancel-pending");
    let input = dir.file("clip.mp4", "video");
    let output = dir.file("out.png", "");
    let (st, shared) = context(&input);
    assert!(identity::cancel_video(&st, &input));
    let link = WorkerLink {
        state: Mutex::new(State::Unstarted),
    };
    assert!(link
        .generate(&input, &output, THUMB_SIZE, false, HUNG, Some(&shared))
        .is_err());
    assert!(matches!(*link.state.lock().unwrap(), State::Unstarted));
    assert!(identity::video_cancelled(&shared));
    assert_eq!(std::fs::read(output).unwrap(), b"");
}

#[test]
fn cancellation_discards_partial_output_without_cache_marker_or_exec_retry() {
    if !crate::backend::sandbox::available() {
        panic!("cancellation cache proof requires bwrap and prlimit on PATH");
    }
    // Completion, refusal and worker failure can each race the backend's cancellation.
    for verdict in [Some(SUCCEEDED), Some(FAILED), Some(NOT_STARTED), None] {
        let dir = TestDir::new("worker-cancel-cache");
        let input = dir.file("clip.mp4", "video");
        let program = dir.script("ffmpegthumbnailer", "#!/bin/sh\nexit 1\n");
        let aliases = Arc::new(Aliases::load());
        let spec = format!(
            "[Thumbnailer Entry]\nExec={} -i %i -o %o -s %s\nMimeType=video/mp4;\n",
            program.display()
        );
        let specs = Arc::new(Thumbnailers::from_entries(
            &[("video.thumbnailer".into(), spec)],
            &aliases,
        ));
        let (mine, theirs) = fdpass::pair().unwrap();
        let child = Command::new("true").spawn().unwrap();
        let link = WorkerLink {
            state: Mutex::new(State::Running {
                jailed: dummy_jailed(child),
                requests: mine,
            }),
        };
        let cache = Cache::at(dir.join("cache"));
        let tables = thumbs::Tables {
            aliases,
            specs,
            cache,
            worker: Some(link),
        };
        let (st, shared) = context(&input);
        let mut job = thumbs::Job {
            source_size: None,
            replies: Some(shared.clone()),
            path: input.clone(),
            mtime: 1,
            trash_identity: None,
            mime: "video/mp4".into(),
            trace: None,
        };
        let (taken, wait_taken) = mpsc::channel();
        let (release, wait_release) = mpsc::channel();
        thread::scope(|scope| {
            let responder = scope.spawn(move || {
                let request = fdpass::recv(theirs.as_raw_fd()).unwrap().unwrap();
                let mut fds = request.fds.into_iter();
                let _input = fds.next().unwrap();
                let mut output = std::fs::File::from(fds.next().unwrap());
                let reply = fds.next().unwrap();
                output.write_all(b"incomplete PNG").unwrap();
                taken.send(()).unwrap();
                assert!(matches!(
                    read_byte(&reply, HUNG),
                    Heard::Byte(crate::backend::thumbworker::CANCEL_VIDEO)
                ));
                if let Some(verdict) = verdict {
                    fdpass::send_byte(&reply, verdict).unwrap();
                }
                wait_release.recv_timeout(HUNG).unwrap();
            });
            let generator = scope.spawn(|| thumbs::run_one_in(&tables, &mut job, &dir.join("raw")));
            wait_taken
                .recv_timeout(HUNG)
                .expect("worker did not take the job");
            assert!(identity::cancel_video(&st, &input));
            // Let a closed reply represent worker death without retaining any peer in the responder.
            release.send(()).unwrap();
            responder.join().unwrap();
            assert!(matches!(
                generator.join().unwrap(),
                thumbs::Outcome::Cancelled
            ));
        });
        if let State::Running { jailed, .. } =
            &mut *tables.worker.as_ref().unwrap().state.lock().unwrap()
        {
            assert!(jailed.child.wait().unwrap().success());
        }
        let uri = crate::backend::thumbcache::uri_for(&input);
        assert!(
            !tables.cache.large_path(&uri).exists(),
            "cancelled partial was published"
        );
        assert!(
            !tables.cache.fail_path(&uri).exists(),
            "user cancellation judged the file or retried exec"
        );
        assert_eq!(
            std::fs::read_dir(dir.join("cache/large")).unwrap().count(),
            0,
            "temporary output survived cancellation"
        );
    }
}

#[test]
fn the_pool_registers_a_taken_video_so_a_cancel_before_its_socket_is_kept() {
    let dir = TestDir::new("worker-cancel-pool");
    let input = dir.file("clip.mp4", "video");
    let program = dir.script("ffmpegthumbnailer", "#!/bin/sh\nexit 1\n");
    let aliases = Arc::new(Aliases::load());
    let spec = format!(
        "[Thumbnailer Entry]\nExec={} -i %i -o %o -s %s\nMimeType=video/mp4;\n",
        program.display()
    );
    let specs = Arc::new(Thumbnailers::from_entries(
        &[("video.thumbnailer".into(), spec)],
        &aliases,
    ));
    let (mine, theirs) = fdpass::pair().unwrap();
    let child = Command::new("true").spawn().unwrap();
    let tables = Arc::new(thumbs::Tables {
        aliases,
        specs,
        cache: Cache::at(dir.join("cache")),
        worker: Some(WorkerLink {
            state: Mutex::new(State::Running {
                jailed: dummy_jailed(child),
                requests: mine,
            }),
        }),
    });
    let (events, _) = events::channel();
    let mut st = state::State::new(crate::backend::dirsizeworker::Worker::new(events));
    st.thumb_mtimes.insert(input.clone(), 1);
    let shared = identity::start(&mut st, input.clone(), 1, None);
    let (results, done) = mpsc::channel();
    let pool = thumbs::Pool::sharing(1, results, Arc::clone(&tables));
    // Holding the link stops the pool thread inside generate, after the take and before any socket exists.
    let held = tables.worker.as_ref().unwrap().state.lock().unwrap();
    pool.submit(thumbs::Job {
        source_size: None,
        replies: Some(shared),
        path: input.clone(),
        mtime: 1,
        trash_identity: None,
        mime: "video/mp4".into(),
        trace: None,
    });
    let deadline = std::time::Instant::now() + HUNG;
    // The pool registers the video in the same queue lock it pops with, so an empty queue means it is registered.
    while pool.pending() > 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "the pool never took the job"
        );
        thread::yield_now();
    }
    assert!(
        identity::cancel_video(&st, &input),
        "a taken video was not registered for cancellation"
    );
    drop(held);
    let done = done
        .recv_timeout(HUNG)
        .expect("the cancelled job never reported");
    assert!(matches!(done.result, thumbs::Outcome::Cancelled));
    assert!(
        matches!(read_byte(&theirs, Duration::ZERO), Heard::Silence),
        "a cancelled job still reached the worker"
    );
    let uri = crate::backend::thumbcache::uri_for(&input);
    assert!(!tables.cache.fail_path(&uri).exists());
}

#[test]
fn a_video_another_row_still_waits_for_is_not_cancelled() {
    let dir = TestDir::new("worker-cancel-subscribed");
    let input = dir.file("clip.mp4", "video");
    let (events, _) = events::channel();
    let mut st = state::State::new(crate::backend::dirsizeworker::Worker::new(events));
    st.thumb_mtimes.insert(input.clone(), 1);
    let tag = identity::Tag {
        token: "7".into(),
        path: input.to_str().unwrap().into(),
        listing: st.generation,
    };
    let reply = identity::Reply::capture(&tag, 3, &input, st.generation);
    assert!(reply.is_some());
    let shared = identity::start(&mut st, input.clone(), 1, reply);
    identity::begin_video(&shared);
    assert!(!identity::cancel_video(&st, &input));
    assert!(!identity::video_cancelled(&shared));
}
