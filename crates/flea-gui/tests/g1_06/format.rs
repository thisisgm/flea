use super::harness::{english, row};
use flea_gui::navigation::sort::Size;
use flea_gui::row::format::{
    date_text, day_start, is_recent, match_run, permissions, seconds_until_midnight, size_text,
    Words,
};
use std::process::Command;

// POSIX zones with both transitions written out, so the child needs no tzdata: second Sunday of March and first of November at 02:00.
const NEW_YORK: &str = "EST5EDT,M3.2.0,M11.1.0";
const EDMONTON: &str = "MST7MDT,M3.2.0,M11.1.0";
const CHILD: &str = "FLEA_G1_06_ZONE_CHILD";

fn words() -> Words {
    Words::from_catalog(&english())
}

#[test]
fn sizes_print_through_core_locale_with_a_link_word_and_a_folder_walk() {
    let words = words();
    let file = |bytes: u64| size_text(&row("a", "text-plain", bytes, false), None, &words);
    assert_eq!(file(0), "0 B");
    assert_eq!(file(999), "999 B");
    assert_eq!(file(1_000), "1.0 kB");
    assert_eq!(file(18_000), "18.0 kB");
    assert_eq!(
        file(999_950),
        "1.0 MB",
        "a size that rounds to the next unit prints in it"
    );
    assert_eq!(file(12_345_678), "12.3 MB");
    assert_eq!(file(5_000_000_000_000_000), "5.0 PB");
    let mut link = row("shell", "folder", 22, false);
    link.mode = 0o120777;
    assert_eq!(
        size_text(&link, None, &words),
        "link",
        "a link's st_size is its target's length, not a size"
    );
    let folder = row("src", "folder", 4096, true);
    assert_eq!(
        size_text(&folder, None, &words),
        "\u{b7}",
        "an unwalked folder prints a dot"
    );
    let walked = |bytes, partial| size_text(&folder, Some(Size { bytes, partial }), &words);
    assert_eq!(walked(18_000, false), "18.0 kB");
    assert_eq!(
        walked(18_000, true),
        ">18.0 kB",
        "a walk that could not read everything says so"
    );
}

#[test]
fn modes_and_match_runs_follow_format_and_match_js() {
    assert_eq!(permissions(0o100755), "rwxr-xr-x");
    assert_eq!(permissions(0o040700), "rwx------");
    assert_eq!(permissions(0o100000), "---------");
    assert_eq!(match_run("Notes.TXT", "notes"), Some((0, 5)));
    assert_eq!(match_run("readme", ""), None);
    let name = "\u{c5}rsrapport.pdf";
    let (start, end) = match_run(name, "\u{e5}rs").unwrap();
    assert_eq!(
        &name[start..end],
        "\u{c5}rs",
        "the run is a byte range of the name itself, whatever its case"
    );
}

#[test]
fn a_stamp_the_clock_cannot_place_prints_two_dashes() {
    assert_eq!(date_text(i64::MAX, &words()), "--");
}

// Runs the ignored zone test in a child with only TZ and the suite name set, as tests/js/dst.js runs under its harness's zone.
fn in_zone(zone: &str, suite: &str) {
    let exe = std::env::current_exe().expect("the test binary has a path");
    let output = Command::new(exe)
        .env_clear()
        .env("TZ", zone)
        .env(CHILD, suite)
        .args([
            "--exact",
            "format::local_dates_in_the_zone_the_parent_sets",
            "--ignored",
            "--test-threads=1",
        ])
        .output()
        .expect("the child test runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "the {suite} child failed:\n{text}");
    assert!(
        text.contains("1 passed"),
        "the {suite} child ran no test:\n{text}"
    );
}

#[test]
fn dates_land_on_local_days_across_both_daylight_saving_transitions() {
    in_zone(NEW_YORK, "new-york");
}

#[test]
fn a_late_evening_file_stays_on_its_local_day() {
    in_zone(EDMONTON, "edmonton");
}

#[test]
#[ignore = "run by the zone tests above, each inside its own fixed zone"]
fn local_dates_in_the_zone_the_parent_sets() {
    let suite = std::env::var(CHILD).unwrap_or_default();
    let words = words();
    let date = |stamp: i64| date_text(stamp, &words);
    match suite.as_str() {
        "new-york" => {
            // tests/js/dst.js: 01:59 EST is followed by 03:00 EDT on 8 March 2026.
            assert_eq!(
                date(1_772_951_400),
                "2026-03-08 01:30",
                "the hour before spring forward keeps its own wall clock"
            );
            assert_eq!(
                date(1_772_955_000),
                "2026-03-08 03:30",
                "the first hour after the spring gap keeps its own wall clock"
            );
            assert_eq!(
                date(1_772_944_200),
                "2026-03-07 23:30",
                "the evening before spring forward keeps the earlier day"
            );
            assert_eq!(date(1_772_953_140), "2026-03-08 01:59");
            assert_eq!(date(1_772_953_200), "2026-03-08 03:00");
            // 01:00 through 01:59 repeats on 1 November 2026.
            assert_eq!(
                date(1_793_507_400),
                "2026-11-01 00:30",
                "the hour before fall back keeps its own wall clock"
            );
            assert_eq!(
                date(1_793_511_000),
                "2026-11-01 01:30",
                "the first pass through the repeated hour reads 01:30"
            );
            assert_eq!(
                date(1_793_514_600),
                "2026-11-01 01:30",
                "the second pass through the repeated hour reads 01:30 as well"
            );
            assert_eq!(
                date(1_793_503_800),
                "2026-10-31 23:30",
                "the evening before fall back keeps the earlier day"
            );
            assert_eq!(
                date(1_792_380_600),
                "2026-10-18 23:30",
                "23:30 local is still the 18th though UTC has turned the 19th"
            );
            // Midnights are built from components, so the short day is 23 hours, the long one 25, and the wake waits as long.
            assert_eq!(
                day_start(1_773_072_000) - day_start(1_772_985_600),
                23 * 3_600,
                "spring forward makes a 23 h day"
            );
            assert_eq!(
                day_start(1_793_638_800) - day_start(1_793_552_400),
                25 * 3_600,
                "fall back makes a 25 h day"
            );
            assert_eq!(
                seconds_until_midnight(1_772_946_000),
                23 * 3_600,
                "midnight before spring forward arms 23 hours"
            );
            assert_eq!(
                seconds_until_midnight(1_793_505_600),
                25 * 3_600,
                "midnight before fall back arms 25 hours"
            );
            assert_eq!(seconds_until_midnight(1_772_947_800), 81_000);
            // Just after local midnight the evening before is no longer today, which a UTC day would get wrong.
            assert!(!is_recent(1_792_380_600, day_start(1_792_384_200)));
            assert!(is_recent(1_792_380_600, day_start(1_792_381_500)));
            assert!(is_recent(1_792_382_400, day_start(1_792_384_200)));
        }
        "edmonton" => {
            // Observed at 23:21 MDT on 3 September 2026: a late-evening file must not advance into the next day.
            assert_eq!(
                date(1_788_499_140),
                "2026-09-03 23:19",
                "the reporter's first late-evening file stays on its own day"
            );
            assert_eq!(
                date(1_788_498_720),
                "2026-09-03 23:12",
                "the reporter's second late-evening file stays on its own day"
            );
            assert_eq!(
                date(1_788_237_660),
                "2026-08-31 22:41",
                "the reporter's August date does not advance into September"
            );
            assert_eq!(
                date(1_787_631_840),
                "2026-08-24 22:24",
                "the reporter's older date keeps its local day"
            );
        }
        _ => {}
    }
}
