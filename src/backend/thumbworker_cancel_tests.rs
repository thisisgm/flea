//! Exercise the worker's real socket loop with a synchronized decoder stand-in.
use super::*;
use crate::backend::testdir::TestDir;
use std::{
    fs::File,
    io::Read,
    process::{Command, Stdio},
    time::Duration,
};

const PROBE: &str = "FLEA_VIDEO_CANCEL_PROBE";
const THIS_TEST: &str = "backend::thumbworker::cancel_tests::active_decoder_cancel_protocol_reaps_only_the_requested_job";
const WAIT: Duration = Duration::from_secs(5);

struct Probe(std::process::Child);

impl Drop for Probe {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_some() {
            return;
        }
        // On a failed assertion, end only decoders parented by this test's worker.
        let pid = self.0.id();
        if let Ok(children) = std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children")) {
            for child in children
                .split_whitespace()
                .filter_map(|child| child.parse().ok())
            {
                let _ = crate::procsignal::signal_process(child, SIGKILL);
            }
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

extern "C" {
    fn getpid() -> c_int;
    fn pause() -> c_int;
    fn signal(number: c_int, handler: usize) -> usize;
}

const SIGUSR1: c_int = 10;

extern "C" fn released(_: c_int) {
    unsafe { _exit(CHILD_OK) }
}

// The confined child announces it has taken the input before blocking in the decoder.
unsafe extern "C" fn create() -> *mut VideoThumbnailer {
    let mut input = File::from_raw_fd(INPUT_FD);
    let mut first = [0];
    input.read_exact(&mut first).unwrap();
    signal(SIGUSR1, released as extern "C" fn(c_int) as usize);
    let mut output = File::from_raw_fd(OUTPUT_FD);
    writeln!(output, "taken {}", getpid()).unwrap();
    if first[0] == b's' {
        loop {
            pause();
        }
    }
    _exit(CHILD_OK)
}

unsafe extern "C" fn set_size(_: *mut VideoThumbnailer, _: c_int, _: c_int) -> c_int {
    0
}
unsafe extern "C" fn create_image() -> *mut ImageData {
    std::ptr::null_mut()
}
unsafe extern "C" fn to_buffer(
    _: *mut VideoThumbnailer,
    _: *const c_char,
    _: *mut ImageData,
) -> c_int {
    1
}

fn send_job(requests: &OwnedFd, input: &Path, output: &Path) -> OwnedFd {
    let input = File::open(input).unwrap();
    let output = File::create(output).unwrap();
    let (reply, theirs) = fdpass::pair().unwrap();
    fdpass::send(
        requests.as_raw_fd(),
        &[0, 1, 0, 0, 0],
        &[input.as_raw_fd(), output.as_raw_fd(), theirs.as_raw_fd()],
    )
    .unwrap();
    reply
}

use std::path::Path;

fn receive(reply: &OwnedFd) -> Option<u8> {
    let mut p = PollFd {
        fd: reply.as_raw_fd(),
        events: POLLIN,
        revents: 0,
    };
    if unsafe { poll(&mut p, 1, WAIT.as_millis() as c_int) } <= 0 {
        return None;
    }
    fdpass::recv(reply.as_raw_fd())
        .ok()?
        .and_then(|message| message.payload.first().copied())
}

// Sample output file: "taken 4242\n", written by the stand-in decoder once it holds its input.
fn taken(output: &Path) -> c_int {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(pid) = std::fs::read_to_string(output)
            .ok()
            .and_then(|s| s.strip_prefix("taken ")?.trim().parse().ok())
        {
            return pid;
        }
        assert!(Instant::now() < deadline, "decoder never took its input");
        std::thread::yield_now();
    }
}

#[test]
fn active_decoder_cancel_protocol_reaps_only_the_requested_job() {
    if std::env::var_os(PROBE).is_some() {
        let requests = unsafe { OwnedFd::from_raw_fd(0) };
        let abi = landlock_abi().expect("Landlock in probe");
        let decoder = Decoder {
            create,
            set_size,
            create_image_data: create_image,
            to_buffer,
        };
        std::process::exit(serve(&requests, &decoder, abi));
    }
    if landlock_abi().is_none() {
        eprintln!("SKIP cancellation protocol: no Landlock truncation support");
        return;
    }
    let dir = TestDir::new("video-cancel-protocol");
    let slow = dir.file("slow.mp4", "slow");
    let fast = dir.file("fast.mp4", "fast");
    let (requests, theirs) = fdpass::pair().unwrap();
    let mut worker = Probe(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", THIS_TEST, "--nocapture"])
            .env(PROBE, "1")
            .stdin(Stdio::from(theirs))
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let output = dir.join("partial.png");
    let slow_reply = send_job(&requests, &slow, &output);
    let pid = taken(&output);
    let other_output = dir.join("other.png");
    let other_reply = send_job(&requests, &slow, &other_output);
    let other_pid = taken(&other_output);
    fdpass::send_byte(&slow_reply, b'C').unwrap();
    let cancelled = receive(&slow_reply);
    let reaped = !Path::new(&format!("/proc/{pid}")).exists();
    // Clean the negative control too, before asserting: signal only this probe's announced child.
    if !reaped {
        crate::procsignal::signal_process(pid, SIGKILL).unwrap();
        let _ = receive(&slow_reply);
    }
    let mut polled = PollFd {
        fd: other_reply.as_raw_fd(),
        events: POLLIN,
        revents: 0,
    };
    let other_running = unsafe { poll(&mut polled, 1, 0) } == 0;
    crate::procsignal::signal_process(other_pid, SIGUSR1).unwrap();
    let other = receive(&other_reply);
    let later_reply = send_job(&requests, &fast, &dir.join("later.png"));
    let later = receive(&later_reply);
    let mut raced = Vec::new();
    for i in 0..8 {
        let reply = send_job(&requests, &fast, &dir.join(&format!("race-{i}.png")));
        let _ = fdpass::send_byte(&reply, b'C');
        raced.push(receive(&reply));
    }
    drop(requests);
    assert!(worker.0.wait().unwrap().success());
    assert_eq!(
        other,
        Some(SUCCEEDED),
        "unrelated active job was cancelled or could not complete"
    );
    assert!(other_running, "cancelling one job ended its active sibling");
    assert_eq!(
        cancelled,
        Some(b'X'),
        "active decoder ignored its cancel packet"
    );
    assert!(reaped, "cancel verdict preceded child reap");
    assert_eq!(
        later,
        Some(SUCCEEDED),
        "cancelling one decode stopped the worker"
    );
    assert!(
        raced
            .iter()
            .all(|verdict| matches!(verdict, Some(SUCCEEDED | b'X'))),
        "exit/cancel race lost a verdict: {raced:?}"
    );
}
