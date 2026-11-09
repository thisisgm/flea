//! Original ThreadRunner descendant controls, retaining their 300 ms deadlines.
use super::super::process::{Event, Handle, Job, Runner, ThreadRunner, OUTPUT_CAP};
use super::{bounded_exit, event_before};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_millis(300);

/// Several of the runner's 5 ms polls, so it has seen the leader exit before the test cancels.
const RUNNER_SEES_EXIT: Duration = Duration::from_millis(25);

fn start(code: &str) -> Handle {
    start_job(Job::plain(&["python3", "-c", code]))
}

fn start_job(job: Job) -> Handle {
    let handle = ThreadRunner.start(job);
    assert!(matches!(
        event_before(&handle, Duration::from_secs(3)),
        Some(Event::Started)
    ));
    handle
}

#[test]
fn sol_pl1_drain_escaped_descendant_cannot_hold_leader_exit() {
    let code = r#"
import os,time
os.write(1,b'leader\xff')
os.write(2,b'diagnostic')
r,w=os.pipe()
if os.fork() == 0:
 os.setsid()
 os.write(w,b'r')
 time.sleep(2)
 os._exit(0)
os.read(r,1)
os._exit(23)
"#;
    let at = Instant::now();
    let handle = start(code);
    let outcome = bounded_exit(&handle, DEADLINE.saturating_sub(at.elapsed()));
    println!("escaped descendant drain {:?}", at.elapsed());
    assert!(at.elapsed() < DEADLINE);
    assert_eq!(outcome.code, 23);
    assert_eq!(outcome.stdout, "leader\u{fffd}");
    assert_eq!(outcome.stderr, "diagnostic");
}

#[test]
fn sol_pl1_drain_cancel_during_escaped_descendant_drain() {
    let code = r#"
import os,socket,time
r,w=os.pipe()
if os.fork() == 0:
 os.setsid()
 os.write(w,b'r')
 time.sleep(2)
 os._exit(0)
os.read(r,1)
member=os.fork()
if member == 0:
 time.sleep(10)
 os._exit(0)
with socket.create_connection(('127.0.0.1',int(os.environ['FLEA_DRAIN_PORT']))) as peer:
 peer.sendall(('%d %d\n'%(os.getpid(),member)).encode())
 peer.recv(1)
os._exit(0)
"#;
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut job = Job::plain(&["python3", "-c", code]);
    job.env.push((
        "FLEA_DRAIN_PORT".into(),
        listener.local_addr().unwrap().port().to_string(),
    ));
    let handle = start_job(job);
    let waiting = Instant::now();
    let mut peer = loop {
        match listener.accept() {
            Ok((peer, _)) => break peer,
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            Err(error) => panic!("descendant fixture handshake failed: {error}"),
        }
        assert!(waiting.elapsed() < Duration::from_secs(3));
        std::thread::sleep(Duration::from_millis(1));
    };
    peer.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let mut pids = String::new();
    BufReader::new(&peer).read_line(&mut pids).unwrap();
    // The fixture sends its leader pid and its group member's pid as one line, for example "123 124\n".
    let (pid, member) = pids.trim().split_once(' ').unwrap();
    let (pid, member): (u32, u32) = (pid.parse().unwrap(), member.parse().unwrap());
    peer.write_all(b"r").unwrap();
    // The runner keeps the leader unreaped while its escaped descendant holds both pipes.
    while process_state(pid) != Some('Z') {
        assert!(waiting.elapsed() < Duration::from_secs(3));
        assert!(
            handle.events.try_recv().is_err(),
            "drain ended before cancel"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    std::thread::sleep(RUNNER_SEES_EXIT);
    assert!(
        handle.events.try_recv().is_err(),
        "drain ended before cancel"
    );
    let at = Instant::now();
    handle.kill();
    let outcome = bounded_exit(&handle, DEADLINE);
    println!("cancel during descendant drain {:?}", at.elapsed());
    assert!(at.elapsed() < DEADLINE);
    assert_eq!(outcome.code, 0, "preserve the leader's actual exit code");
    assert!(outcome.stdout.is_empty());
    assert!(outcome.stderr.is_empty());
    group_member_stopped(member);
}

#[test]
fn sol_pl1_drain_continuous_writer_cannot_extend_snapshot_budget() {
    let code = r#"
import os,time
r,w=os.pipe()
if os.fork():
 os.read(r,1)
 os._exit(23)
os.setsid()
os.write(1,b'w'*8192)
os.write(w,b'r')
end=time.monotonic()+2
try:
 while time.monotonic()<end:
  os.write(1,b'w'*(1<<20))
except BrokenPipeError:
 pass
os._exit(0)
"#;
    let at = Instant::now();
    let handle = start(code);
    let outcome = bounded_exit(&handle, DEADLINE.saturating_sub(at.elapsed()));
    println!("continuous writer drain {:?}", at.elapsed());
    assert!(at.elapsed() < DEADLINE);
    assert_eq!(outcome.code, 23);
    assert!(!outcome.stdout.is_empty());
    assert!(outcome.stdout.len() <= OUTPUT_CAP);
    assert!(outcome.stdout.bytes().all(|byte| byte == b'w'));
    assert!(outcome.stderr.is_empty());
}

#[test]
fn sol_pl1_drain_preserves_both_complete_output_streams() {
    let code = "import os,signal\nsignal.alarm(3)\nos.write(1,b'o'*65536)\nos.write(2,b'e'*65536)\nos._exit(23)\n";
    let handle = start(code);
    let outcome = bounded_exit(&handle, Duration::from_secs(3));
    assert_eq!(outcome.code, 23);
    assert_eq!(outcome.stdout, "o".repeat(65536));
    assert_eq!(outcome.stderr, "e".repeat(65536));
}

// /proc/<pid>/stat starts with "123 (python3) Z ..."; the final ") " ends the process name.
fn process_state(pid: u32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(") ")?.1.chars().next()
}

fn descendant_pid(stdout: &str) -> u32 {
    stdout
        .trim()
        .parse()
        .expect("the descendant reports its pid")
}

fn group_member_stopped(pid: u32) {
    let at = Instant::now();
    loop {
        if matches!(process_state(pid), None | Some('Z')) {
            return;
        }
        assert!(at.elapsed() < DEADLINE, "descendant {pid} is still alive");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn sol_pl1_drain_leader_exit_leaves_its_process_group_running() {
    let code = "import os,time\nr,w=os.pipe()\nif os.fork() == 0:\n os.write(1,str(os.getpid()).encode())\n os.write(w,b'r')\n time.sleep(2)\n os._exit(0)\nos.read(r,1)\nos._exit(23)\n";
    let handle = start(code);
    let outcome = bounded_exit(&handle, DEADLINE);
    assert_eq!(outcome.code, 23);
    assert!(outcome.stderr.is_empty());
    let pid = descendant_pid(&outcome.stdout);
    let alive = matches!(process_state(pid), Some(state) if state != 'Z');
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .status();
    assert!(alive, "a natural leader exit killed group member {pid}");
}

#[test]
fn sol_pl1_drain_cancel_kills_the_whole_process_group() {
    let code = "import os,time\nif os.fork() == 0:\n os.write(1,str(os.getpid()).encode())\ntime.sleep(2)\n";
    let handle = start(code);
    std::thread::sleep(Duration::from_millis(100));
    handle.kill();
    let outcome = bounded_exit(&handle, DEADLINE);
    assert_ne!(outcome.code, 0);
    assert!(outcome.stderr.is_empty());
    group_member_stopped(descendant_pid(&outcome.stdout));
}
