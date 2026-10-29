//! Failed enumeration produces one bounded stderr line rather than silently losing the device section.
use super::*;

#[test]
fn device_probe_failure_reports_program_exit_and_spawn_reason_on_one_line() {
    let outcome = Outcome {
        code: -1,
        stderr: "No such file\n\r\u{1b}PATH omitted".into(),
        ..Outcome::default()
    };
    let line = probe_failure("lsblk", &outcome).unwrap();
    assert_eq!(
        line,
        "flea-gui: device probe lsblk failed (exit -1): No such file PATH omitted"
    );
    assert_eq!(line.lines().count(), 1);
    assert!(!line.contains('\u{1b}'));
    let long = Outcome {
        stderr: "x".repeat(1000),
        ..outcome
    };
    assert!(probe_failure("gio mount -li", &long).unwrap().len() < 350);
}

#[test]
fn successful_empty_probes_stay_quiet_and_malformed_lsblk_is_reported() {
    assert!(probe_failure(
        "lsblk",
        &Outcome {
            stdout: r#"{"blockdevices":[]}"#.into(),
            ..Outcome::default()
        }
    )
    .is_none());
    assert!(probe_failure("gio mount -li", &Outcome::default()).is_none());
    assert_eq!(
        probe_failure("lsblk", &Outcome::default()).as_deref(),
        Some("flea-gui: device probe lsblk failed (invalid blockdevices JSON)")
    );
    assert!(probe_failure(
        "lsblk",
        &Outcome {
            stdout: "{}".into(),
            ..Outcome::default()
        }
    )
    .is_some());
}

#[test]
fn hostname_newline_never_reaches_the_machine_row() {
    assert_eq!(hostname_of("testhost\n"), "testhost");
    let source = Source::new(Rc::new(EmptyRunner), Some("testhost\n".into()), &Json::Null);
    assert_eq!(source.devices.hostname, "testhost");
}

#[test]
fn probe_timeouts_name_the_bound_that_killed_them() {
    assert_eq!(
        probe_timeout("lsblk", crate::g2::pl1::devices::LIST_TIMEOUT_MS),
        format!(
            "flea-gui: device probe lsblk failed: timed out after {} ms",
            crate::g2::pl1::devices::LIST_TIMEOUT_MS
        )
    );
    assert_eq!(
        probe_timeout("gio mount -li", 7),
        "flea-gui: device probe gio mount -li failed: timed out after 7 ms",
        "the bound is read, never a literal"
    );
    let long = Outcome {
        code: 1,
        stderr: "q".repeat(STDERR_CHARS * 2),
        ..Outcome::default()
    };
    let line = probe_failure("lsblk", &long).unwrap();
    assert_eq!(
        line.matches('q').count(),
        STDERR_CHARS,
        "stderr is cut at STDERR_CHARS"
    );
}
