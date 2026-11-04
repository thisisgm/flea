//! Each clock oracle runs in its own process so timezone state cannot leak between tests.
use super::Locale;
use std::{
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn date(arguments: &[&str]) -> String {
    let output = Command::new("date")
        .env("LC_ALL", "C")
        .args(arguments)
        .output()
        .unwrap();
    assert!(output.status.success(), "date failed: {output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[test]
fn sol_gap64_shared_clock_matches_date_in_four_isolated_timezones() {
    const TEST: &str =
        "i18n::gap64_tests::sol_gap64_shared_clock_matches_date_in_four_isolated_timezones";
    const ZONES: [&str; 4] = [
        "UTC0",
        "America/Santo_Domingo",
        "Asia/Kathmandu",
        "Europe/Berlin",
    ];
    if let Ok(zone) = std::env::var("FLEA_GAP64_CLOCK_CHILD") {
        assert!(ZONES.contains(&zone.as_str()));
        let locale = Locale {
            glibc_locale: "C".into(),
            ..Locale::default()
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        for timestamp in [-1, 0, 951868799, 2147483648, 4102444800, now] {
            let input = format!("@{timestamp}");
            let expected = date(&["-d", &input, "+%Y-%m-%d %H:%M"]);
            assert_eq!(
                locale.format_date(timestamp).as_deref(),
                Some(expected.as_str()),
                "{zone} {timestamp}"
            );
        }
        let before = date(&["+%s"]).parse::<i64>().unwrap();
        let deleted = crate::backend::trashdate::deletion_date_local();
        let after = date(&["+%s"]).parse::<i64>().unwrap();
        let parsed = date(&["-d", &deleted, "+%s"]).parse::<i64>().unwrap();
        assert!(
            (before..=after).contains(&parsed),
            "{zone}: {before} <= {parsed} <= {after}"
        );
        return;
    }
    for zone in ZONES {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env("TZ", zone)
            .env("FLEA_GAP64_CLOCK_CHILD", zone)
            .output()
            .unwrap();
        assert!(output.status.success(), "{zone}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "{zone}: child selected no test"
        );
    }
}
