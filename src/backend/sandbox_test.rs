use crate::backend::child::Ran;
use crate::backend::sandbox_broker::Client;
use crate::backend::{cgroup::Scope, sandbox, sandbox_exec::Launch, sandbox_gate, systemd_scope};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

const TEST_LIMITS: crate::backend::cgroup::Limits = crate::backend::cgroup::Limits { high: 64 * 1024 * 1024, max: 64 * 1024 * 1024, swap_max: 0 };

pub fn main(args: &[String]) -> i32 {
    match args {
        [_, mode, kind, input, argv @ ..] if mode == "--sandbox-test" && kind == "one-shot" && !argv.is_empty() => one_shot(Path::new(input), argv),
        [_, mode, kind, input, argv @ ..] if mode == "--sandbox-test" && kind == "disposable-low" && !argv.is_empty() => disposable_job(Path::new(input), argv, None),
        [_, mode, kind, input, argv @ ..] if mode == "--sandbox-test" && kind == "disposable-place-fail" && !argv.is_empty() => disposable_job(Path::new(input), argv, Some(u32::MAX)),
        [_, mode, kind, placement, argv @ ..] if mode == "--sandbox-test" && kind == "disposable-inner" && !argv.is_empty() => match placement.parse() {
            Ok(placement) => disposable_inner(argv, placement),
            Err(_) => 2,
        },
        [_, mode, kind, input, output, count, argv @ ..] if mode == "--sandbox-test" && kind == "broker-burst" && !argv.is_empty() => match count.parse() {
            Ok(count) => broker_burst(Path::new(input), Path::new(output), count, argv),
            Err(_) => 2,
        },
        [_, mode, kind, count, argv @ ..] if mode == "--sandbox-test" && kind == "broker-direct" && !argv.is_empty() => match count.parse() {
            Ok(count) => run_burst(Arc::new(argv.to_vec()), count),
            Err(_) => 2,
        },
        [_, mode, kind, input, output, argv @ ..] if mode == "--sandbox-test" && kind == "broker-wrapped" && !argv.is_empty() => broker_wrapped(Path::new(input), Path::new(output), argv),
        _ => 2,
    }
}

fn broker_wrapped(input: &Path, output: &Path, argv: &[String]) -> i32 {
    let launch = match Launch::new(argv, input, output) {
        Ok(launch) => launch,
        Err(_) => return 2,
    };
    let ran = Client::new().run_checked(&launch.argv, Duration::from_secs(20), &launch.error);
    println!("ran={}", ran_name(ran));
    0
}

fn one_shot(input: &Path, argv: &[String]) -> i32 {
    if !sandbox::available() {
        return 2;
    }
    let inner = sandbox::wrap_readonly(argv, input);
    let full = sandbox::one_shot(&inner);
    match Command::new(&full[0]).args(&full[1..]).stdin(Stdio::null()).stdout(Stdio::inherit()).stderr(Stdio::inherit()).status() {
        Ok(status) if status.success() => 0,
        Ok(_) => 1,
        Err(_) => 2,
    }
}

fn disposable_job(input: &Path, argv: &[String], placement_pid: Option<u32>) -> i32 {
    if !sandbox::available() {
        return 2;
    }
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(_) => return 2,
    };
    let mut inner = vec![exe.to_string_lossy().into_owned(), "--sandbox-test".to_string(), "disposable-inner".to_string(), placement_pid.unwrap_or(0).to_string()];
    inner.extend(sandbox::wrap_readonly(argv, input));
    let full = systemd_scope::delegated(&inner);
    match Command::new(&full[0]).args(&full[1..]).status() {
        Ok(status) if status.success() => 0,
        Ok(_) => 1,
        Err(_) => 2,
    }
}

fn disposable_inner(argv: &[String], placement_pid: u32) -> i32 {
    let mut scope = match Scope::enter() {
        Ok(scope) => scope,
        Err(_) => return 2,
    };
    let forced = (placement_pid != 0).then_some(placement_pid);
    let (ran, events) = sandbox_gate::run_for_test(&mut scope, argv, Duration::from_secs(20), TEST_LIMITS, forced);
    let leaves = match scope.job_leaf_count() {
        Ok(leaves) => leaves,
        Err(_) => return 2,
    };
    println!("ran={} max={} oom={} oom_kill={} leaves={leaves}", ran_name(ran), events.max, events.oom, events.oom_kill);
    0
}

fn ran_name(ran: Ran) -> &'static str {
    match ran {
        Ran::Succeeded => "succeeded",
        Ran::Failed => "failed",
        Ran::NotStarted => "not_started",
    }
}

fn broker_burst(input: &Path, output: &Path, count: usize, argv: &[String]) -> i32 {
    if !sandbox::available() || count == 0 {
        return 2;
    }
    let full = Arc::new(sandbox::wrap(argv, input, output));
    run_burst(full, count)
}

fn run_burst(full: Arc<Vec<String>>, count: usize) -> i32 {
    let workers = count.min(4);
    let handles: Vec<_> = (0..workers)
        .map(|worker| {
            let full = Arc::clone(&full);
            std::thread::spawn(move || {
                let mut client = Client::new();
                (worker..count).step_by(workers).map(|_| client.run(&full, Duration::from_secs(20))).collect::<Vec<_>>()
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().flat_map(|handle| handle.join().unwrap_or_default()).collect();
    println!(
        "succeeded={} failed={} not_started={}",
        results.iter().filter(|&&ran| ran == Ran::Succeeded).count(),
        results.iter().filter(|&&ran| ran == Ran::Failed).count(),
        results.iter().filter(|&&ran| ran == Ran::NotStarted).count()
    );
    if results.len() == count && results.iter().all(|ran| *ran == Ran::Succeeded) {
        0
    } else {
        1
    }
}
