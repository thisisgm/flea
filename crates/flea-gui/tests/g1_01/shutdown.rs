use super::*;

fn wait_file(path: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.is_file() {
        assert!(
            Instant::now() < deadline,
            "missing fixture {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn wait_drained(
    replies: &async_channel::Receiver<Event>,
    limit: Duration,
) -> Option<Vec<&'static str>> {
    let deadline = Instant::now() + limit;
    let mut failures = Vec::new();
    loop {
        match replies.try_recv() {
            Ok(Event::Drained) => return Some(failures),
            Ok(Event::Failed(error)) => failures.push(error.key),
            Err(async_channel::TryRecvError::Closed) => return None,
            _ if Instant::now() >= deadline => return None,
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    }
}

// A killed child is reaped off the worker thread, so Drained can come first; a zombie keeps its /proc entry until then.
fn assert_reaped(root: &std::path::Path) {
    let pid = std::fs::read_to_string(root.join("ready")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while PathBuf::from(format!("/proc/{pid}")).exists() {
        assert!(Instant::now() < deadline, "backend {pid} was not reaped");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn quit_deadline_covers_a_backend_that_stops_reading_requests() {
    let root = root();
    let script = "import os,pathlib,sys,time\nr=pathlib.Path(sys.argv[1])\nsys.stdin.readline()\n(r/'ready').write_text(str(os.getpid()))\nfor _ in range(450):\n if (r/'release').exists(): break\n time.sleep(.1)\n";
    let mut command = Command::new("python3");
    command.args(["-c", script]).arg(&root);
    let backend = Backend::start_command(command, "/files".into(), None, 320).unwrap();
    wait_file(&root.join("ready"));
    for start in 0..20_000 {
        backend.window(start, 320).unwrap();
    }
    std::thread::sleep(Duration::from_millis(100));
    let closed_at = Instant::now();
    backend.quit();
    let bounded = wait_drained(&backend.replies, Duration::from_secs(31));
    let elapsed = closed_at.elapsed();
    std::fs::write(root.join("release"), b"release only this fixture\n").unwrap();
    let cleaned =
        bounded.is_some() || wait_drained(&backend.replies, Duration::from_secs(5)).is_some();
    eprintln!(
        "blocked pipe: drained within 31 seconds={}, elapsed={elapsed:?}, cleaned={cleaned}",
        bounded.is_some()
    );
    assert!(cleaned, "fixture backend did not finish after release");
    assert_reaped(&root);
    assert!(
        bounded.is_some(),
        "quit never reached its 30-second deadline while the writer was blocked"
    );
    assert_eq!(bounded.unwrap(), ["window.backend_timeout"]);
    assert!(backend.window(0, 1).is_err());
    assert!(backend.drained().recv_blocking().is_ok());
}

#[test]
fn quit_prioritizes_queued_windows_and_drains_every_reply() {
    let root = root();
    let script = "import json,os,pathlib,sys,time\nr=pathlib.Path(sys.argv[1])\nsys.stdin.readline()\n(r/'ready').write_text(str(os.getpid()))\nwhile not (r/'release').exists(): time.sleep(.005)\nn=0\nfor line in sys.stdin:\n if json.loads(line)['c']=='quit': break\n n+=1\n(r/'windows').write_text(str(n))\nassert sys.stdin.read()==''\nfor i in range(5000):\n sys.stdout.write(json.dumps({'t':'listed','path':'/files/é','n':i},ensure_ascii=False)+('\\n' if i<4999 else ''))\nsys.stdout.flush()\ntime.sleep(.05)\n(r/'finished').write_text('all replies flushed')\n";
    let mut command = Command::new("python3");
    command.args(["-c", script]).arg(&root);
    let backend = Backend::start_command(command, "/files".into(), None, 320).unwrap();
    wait_file(&root.join("ready"));
    for start in 0..20_000 {
        backend.window(start, 320).unwrap();
    }
    std::thread::sleep(Duration::from_millis(100));
    backend.quit();
    std::fs::write(root.join("release"), b"allow this child to read\n").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut received = 0;
    loop {
        assert!(Instant::now() < deadline, "readable child did not drain");
        match backend.replies.try_recv() {
            Ok(Event::Drained) => break,
            Ok(Event::Reply(flea::protocol::Reply::Listed { path, total })) => {
                assert_eq!(path, "/files/é");
                assert_eq!(total, received);
                received += 1;
            }
            Ok(event) => panic!("unexpected drain event: {event:?}"),
            Err(async_channel::TryRecvError::Empty) => std::thread::sleep(Duration::from_millis(5)),
            Err(error) => panic!("missing drain event: {error}"),
        }
    }
    assert_eq!(received, 5000);
    assert!(root.join("finished").is_file());
    let windows: usize = std::fs::read_to_string(root.join("windows"))
        .unwrap()
        .parse()
        .unwrap();
    assert!(windows < 20_000, "quit followed all queued windows");
    assert_reaped(&root);
    eprintln!("readable child: windows before quit={windows}/20000, replies drained={received}");
}

#[test]
fn platform_stdin_close_returns_at_once_and_the_backend_finishes_its_eof_cleanup() {
    let root = root();
    let script = "import os,pathlib,sys,time\nr=pathlib.Path(sys.argv[1])\nsys.stdin.readline()\n(r/'ready').write_text(str(os.getpid()))\nsys.stdin.read()\n(r/'eof').write_text('stdin closed')\ntime.sleep(.3)\n(r/'cleanup').write_text('cleanup finished')\n";
    let mut command = Command::new("python3");
    command.args(["-c", script]).arg(&root);
    let backend = Backend::start_command(command, "/files".into(), None, 320).unwrap();
    wait_file(&root.join("ready"));
    for start in 0..20_000 {
        backend.window(start, 320).unwrap();
    }
    let replies = backend.replies.clone();
    let closed_at = Instant::now();
    // The platform path closes stdin and drops the handle, whose quit must not arm a deadline.
    backend.close_stdin();
    drop(backend);
    let returned = closed_at.elapsed();
    let cleanup_at_return = root.join("cleanup").exists();
    let failures = wait_drained(&replies, Duration::from_secs(5));
    eprintln!(
        "platform stdin close: returned in {returned:?}, cleanup at return={cleanup_at_return}, eof={}, cleanup={}, reaped after {:?}, failures={failures:?}",
        root.join("eof").is_file(),
        root.join("cleanup").is_file(),
        closed_at.elapsed()
    );
    assert!(
        !cleanup_at_return,
        "platform quit waited for the backend's cleanup"
    );
    assert_eq!(failures.unwrap(), Vec::<&str>::new());
    assert!(root.join("eof").is_file());
    assert!(
        root.join("cleanup").is_file(),
        "platform quit killed the backend before its stdin-EOF cleanup completed"
    );
    assert_reaped(&root);
}

#[test]
fn unexpected_eof_reaps_a_child_that_has_not_exited() {
    let root = root();
    let mut command = Command::new("python3");
    command.args(["-c", "import os,pathlib,sys,time\nr=pathlib.Path(sys.argv[1])\nsys.stdin.readline()\n(r/'ready').write_text(str(os.getpid()))\nos.close(1)\ntime.sleep(20)\n"]).arg(&root);
    let backend = Backend::start_command(command, "/files".into(), None, 320).unwrap();
    assert_eq!(
        wait_drained(&backend.replies, Duration::from_secs(5)).unwrap(),
        ["window.backend_eof"]
    );
    assert_reaped(&root);
}

#[test]
fn spawn_failure_still_delivers_failure_before_drained() {
    let root = root();
    let backend = Backend::start_command(
        Command::new(root.join("absent-executable")),
        "/files".into(),
        None,
        320,
    )
    .unwrap();
    assert_eq!(
        wait_drained(&backend.replies, Duration::from_secs(5)).unwrap(),
        ["window.backend_start"]
    );
}

#[test]
fn quit_cancels_a_reader_when_an_inherited_stdout_remains_open() {
    let root = root();
    let script = "import os,pathlib,sys,time\nr=pathlib.Path(sys.argv[1])\nsys.stdin.readline()\nif os.fork()==0:\n (r/'held').write_text('stdout remains open')\n for _ in range(450):\n  if (r/'release').exists(): break\n  time.sleep(.1)\n (r/'released').write_text('holder exiting')\n os._exit(0)\n(r/'ready').write_text(str(os.getpid()))\nsys.stdin.readline()\nos._exit(0)\n";
    let mut command = Command::new("python3");
    command.args(["-c", script]).arg(&root);
    let backend = Backend::start_command(command, "/files".into(), None, 320).unwrap();
    wait_file(&root.join("ready"));
    wait_file(&root.join("held"));
    let closed_at = Instant::now();
    backend.quit();
    let bounded = wait_drained(&backend.replies, Duration::from_secs(31));
    let elapsed = closed_at.elapsed();
    std::fs::write(root.join("release"), b"release only the stdout holder\n").unwrap();
    let cleaned =
        bounded.is_some() || wait_drained(&backend.replies, Duration::from_secs(5)).is_some();
    wait_file(&root.join("released"));
    assert!(cleaned, "fixture did not finish after stdout release");
    assert_reaped(&root);
    assert_eq!(bounded.unwrap(), ["window.backend_timeout"]);
    eprintln!("inherited stdout: worker released after {elapsed:?}, child reaped");
}
