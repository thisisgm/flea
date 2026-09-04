use super::*;
use std::sync::{Arc, Barrier};

#[test]
fn sandbox_broker_child() {
    if std::env::var_os("FLEA_BROKER_OUTPUT_FD").is_some() {
        std::process::exit(broker_main());
    }
}

#[test]
fn sandbox_gate_child() {
    if let Some(raw) = std::env::var_os("FLEA_GATE_TEST_ARGV") {
        if raw == "__flea_gate_stall__" {
            std::thread::sleep(Duration::from_secs(60));
            std::process::exit(86);
        }
        let mut args = vec!["flea".to_string(), "--sandbox-gate".to_string(), "--".to_string()];
        args.extend(raw.to_string_lossy().split('\u{1f}').map(str::to_string));
        std::process::exit(crate::backend::sandbox::gate_main(&args));
    }
}

#[test]
fn sandbox_exec_gate_child() {
    let Some(error) = std::env::var_os("FLEA_EXEC_GATE_ERROR") else {
        return;
    };
    let count: usize = std::env::var("FLEA_EXEC_GATE_ARGC").unwrap().parse().unwrap();
    let mut args = vec!["flea".to_string(), "--sandbox-exec-gate".to_string(), error.to_string_lossy().into_owned(), "--".to_string()];
    for index in 0..count {
        args.push(std::env::var(format!("FLEA_EXEC_GATE_ARG_{index}")).unwrap());
    }
    std::process::exit(crate::backend::sandbox_exec::exec_gate_main(&args));
}

#[test]
fn a_broker_runs_success_failure_and_exec_failure() {
    let mut client = Client::new();
    assert_eq!(client.run(&["/usr/bin/true".to_string()], Duration::from_secs(5)), Ran::Succeeded);
    assert_eq!(client.run(&["/usr/bin/false".to_string()], Duration::from_secs(5)), Ran::Failed);
    assert_eq!(client.run(&["/definitely/not/here".to_string()], Duration::from_secs(5)), Ran::NotStarted);
    assert_eq!(client.run(&["/usr/bin/true".to_string()], Duration::from_secs(5)), Ran::Succeeded);
}

#[test]
fn a_healthy_broker_restarts_after_broken_ipc() {
    let mut client = Client::new();
    let argv = ["/usr/bin/true".to_string()];
    assert_eq!(client.run(&argv, Duration::from_secs(5)), Ran::Succeeded);
    let Client::Ready(broker) = &mut client else { panic!("broker did not start") };
    broker.stop_for_test();
    assert_eq!(client.run(&argv, Duration::from_secs(5)), Ran::NotStarted);
    assert!(matches!(client, Client::Fresh));
    assert_eq!(client.run(&argv, Duration::from_secs(5)), Ran::Succeeded);
}

#[test]
fn four_concurrent_brokers_do_not_keep_a_crashed_brokers_pipe_open() {
    let barrier = Arc::new(Barrier::new(4));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                start().expect("broker start")
            })
        })
        .collect();
    let mut brokers: Vec<_> = handles.into_iter().map(|handle| handle.join().unwrap()).collect();
    let started = Instant::now();
    let ran = brokers[0].run(&["__flea_broker_crash__".to_string()], Duration::from_millis(100));
    assert_eq!(ran, Ran::NotStarted);
    assert!(started.elapsed() < Duration::from_secs(2), "a foreign broker held the crashed broker pipe open");
    assert_eq!(brokers.len(), 4);
}

#[test]
fn a_short_decoder_deadline_stops_and_reaps_the_tree() {
    let mut broker = start().unwrap();
    let started = Instant::now();
    let ran = broker.run(&["/usr/bin/sleep".to_string(), "60".to_string()], Duration::from_millis(50));
    assert_eq!(ran, Ran::Failed);
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn a_stalled_broker_reply_hits_the_supervisor_deadline() {
    let mut broker = start().unwrap();
    let started = Instant::now();
    let ran = broker.run(&["__flea_broker_stall__".to_string()], Duration::from_millis(50));
    assert_eq!(ran, Ran::NotStarted);
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_stalled_gate_acknowledgement_is_bounded() {
    let mut broker = start().unwrap();
    let started = Instant::now();
    let ran = broker.run(&["__flea_gate_stall__".to_string()], Duration::from_secs(5));
    assert_eq!(ran, Ran::NotStarted);
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn gate_stdin_stdout_and_stderr_are_null() {
    let mut broker = start().unwrap();
    let script = "test \"$(readlink /proc/$$/fd/0)\" = /dev/null && \
                  test \"$(readlink /proc/$$/fd/1)\" = /dev/null && \
                  test \"$(readlink /proc/$$/fd/2)\" = /dev/null";
    let argv = ["/usr/bin/sh".to_string(), "-c".to_string(), script.to_string()];
    assert_eq!(broker.run(&argv, Duration::from_secs(5)), Ran::Succeeded);
}

#[test]
fn a_decoder_deadline_kills_descendants_and_removes_the_job() {
    let dir = crate::backend::testdir::TestDir::new("broker-tree");
    let pid_file = dir.join("pid");
    let script = format!("(sleep 60) & echo $! > '{}'; wait", pid_file.display());
    let argv = ["/usr/bin/sh".to_string(), "-c".to_string(), script];
    let mut broker = start().unwrap();
    assert_eq!(broker.run(&argv, Duration::from_millis(200)), Ran::Failed);
    let pid = std::fs::read_to_string(&pid_file).unwrap();
    let proc_path = std::path::Path::new("/proc").join(pid.trim());
    for _ in 0..100 {
        if !proc_path.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!proc_path.exists(), "a decoder descendant survived cgroup.kill");
}
