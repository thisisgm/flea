use super::*;
use std::process::Command;

#[test]
fn local_calendar_in_fresh_timezone_processes() {
    const CHILD: &str = "FLEA_T4_CALENDAR_CASE";
    if let Ok(case) = std::env::var(CHILD) {
        let (seconds, expected) = case.split_once('|').unwrap();
        assert_eq!(modified(seconds.parse().unwrap()), expected);
        return;
    }
    let cases = [
        ("UTC0", 1790158470, "2026-09-23 10:14"),
        ("EST5EDT,M3.2.0,M11.1.0", 1790158470, "2026-09-23 06:14"),
        ("JST-9", 1790158470, "2026-09-23 19:14"),
        ("UTC0", 1609459199, "2020-12-31 23:59"),
        ("JST-9", 1609459199, "2021-01-01 08:59"),
        ("UTC0", 1709164800, "2024-02-29 00:00"),
        ("EST5EDT,M3.2.0,M11.1.0", 1710053999, "2024-03-10 01:59"),
        ("EST5EDT,M3.2.0,M11.1.0", 1710054000, "2024-03-10 03:00"),
        ("EST5EDT,M3.2.0,M11.1.0", 1730613599, "2024-11-03 01:59"),
        ("EST5EDT,M3.2.0,M11.1.0", 1730613600, "2024-11-03 01:00"),
        ("UTC0", 253402300799, "9999-12-31 23:59"),
        ("UTC0", 253402300800, ""),
        ("JST-9", 253402300799, ""),
        ("UTC0", i64::MAX, ""),
    ];
    for (zone, seconds, expected) in cases {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "tui::calendar::tests::local_calendar_in_fresh_timezone_processes",
                "--nocapture",
            ])
            .env("TZ", zone)
            .env(CHILD, format!("{seconds}|{expected}"))
            .output()
            .unwrap();
        assert!(
            one_calendar_case_passed(&output),
            "{zone} {seconds}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

// Sample output: test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3145 filtered out; finished in 0.00s
fn one_calendar_case_passed(output: &std::process::Output) -> bool {
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.starts_with("test result: ok. 1 passed; 0 failed;"))
}

#[test]
fn a_zero_test_child_is_not_a_successful_calendar_case() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "tui::calendar::tests::missing_timezone_case",
            "--nocapture",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "the negative control needs libtest's successful zero-test exit"
    );
    assert!(
        !one_calendar_case_passed(&output),
        "a filter that runs no test must be refused"
    );
}

#[test]
fn recent_same_day_and_future_times_keep_the_calendar_form() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    for seconds in [now, now - 60, now - 3600, now + 3600] {
        let stamp = modified(seconds);
        assert_eq!(stamp.len(), 16);
        assert_eq!(
            (&stamp[4..5], &stamp[7..8], &stamp[10..11], &stamp[13..14]),
            ("-", "-", " ", ":")
        );
    }
    for seconds in [0, -1, i64::MIN, i64::MAX] {
        assert!(modified(seconds).is_empty());
    }
}

#[test]
fn deleted_wall_dates_validate_fields_without_timezone_conversion() {
    for (raw, expected) in [
        ("2025-06-14T09:30:59", "2025-06-14 09:30"),
        ("2000-02-29T00:00:00", "2000-02-29 00:00"),
        ("0001-01-01T00:00:00", "0001-01-01 00:00"),
        ("9999-12-31T23:59:59", "9999-12-31 23:59"),
        ("1900-02-29T00:00:00", ""),
        ("2025-04-31T00:00:00", ""),
        ("0000-01-01T00:00:00", ""),
        ("10000-01-01T00:00:00", ""),
        ("2025-13-01T00:00:00", ""),
        ("2025-01-00T00:00:00", ""),
        ("2025-01-01T24:00:00", ""),
        ("2025-01-01T00:60:00", ""),
        ("2025-01-01T00:00:60", ""),
        ("2025-01-01T00:00:00Z", ""),
        ("2025-01-01T00:00", ""),
        ("2025-01-01T00:00:xx", ""),
        ("２０２５-01-01T00:00:00", ""),
        ("", ""),
    ] {
        assert_eq!(value(&Json::Str(raw.into())), expected, "{raw}");
    }
    for invalid in [Json::Null, Json::Bool(false), Json::Num("1e99".into())] {
        assert!(value(&invalid).is_empty());
    }
    assert_eq!(value(&Json::Num("1790158470".into())), modified(1790158470));
}
