use super::*;
use std::os::unix::process::ExitStatusExt;
use std::time::Instant;

const SIGTERM: i32 = 15;

// A run of our own to signal: the test never names a pid it did not start.
fn sleeper() -> std::process::Child {
    Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .spawn()
        .expect("sleep starts")
}

// The signal that ended the run within five seconds, or None while it still runs.
fn ended_by(child: &mut std::process::Child) -> Option<Option<i32>> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        if let Some(status) = child.try_wait().expect("the child can be waited on") {
            return Some(status.signal());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

// Runs only as the child of the next test: a quit's teardown, then a closing window, over Omdrop's runs.
#[test]
fn omdrop_teardown_child() {
    let Ok(pids) = std::env::var("FLEA_OMDROP_HOLD_PIDS") else {
        return;
    };
    let [on, off, peers]: [u32; 3] = pids
        .split(',')
        .map(|pid| pid.parse().expect("a pid"))
        .collect::<Vec<_>>()
        .try_into()
        .expect("three pids");
    let mut slots = Pids::default();
    Hold::new(&mut slots, Stack::Omdrop, Proc::Radio, 1).started(on);
    Hold::new(&mut slots, Stack::Omdrop, Proc::Browse, 1).started(peers);
    super::super::airdrop_pids::teardown(Duration::from_secs(2));
    Hold::new(&mut slots, Stack::Omdrop, Proc::Radio, 2).started(off);
    drop(slots);
}

#[test]
fn a_quit_or_closing_window_never_signals_omdrop_on_or_off_but_stops_its_lookup() {
    let (mut on, mut off, mut peers) = (sleeper(), sleeper(), sleeper());
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "g2::send_share::airdrop_run::tests::omdrop_teardown_child",
            "--test-threads=1",
        ])
        .env(
            "FLEA_OMDROP_HOLD_PIDS",
            format!("{},{},{}", on.id(), off.id(), peers.id()),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    let lookup = ended_by(&mut peers);
    let (on_alive, off_alive) = (on.try_wait().unwrap(), off.try_wait().unwrap());
    for child in [&mut on, &mut off, &mut peers] {
        let _ = child.kill();
        let _ = child.wait();
    }
    assert!(status.success(), "{status:?}");
    assert_eq!(
        lookup,
        Some(Some(SIGTERM)),
        "the quit stops Omdrop's lookup"
    );
    assert!(on_alive.is_none(), "teardown left omdrop on alone");
    assert!(
        off_alive.is_none(),
        "the closing window left omdrop off alone"
    );
}
