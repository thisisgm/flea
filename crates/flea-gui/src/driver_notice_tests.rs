use super::{show, DriverNotice, FLAGS, WAIT};
use flea::i18n::{I18n, Paths};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

const SCHEDULING_SLACK: Duration = Duration::from_secs(1);
const STUCK_PAST_BOUND: Duration = Duration::from_secs(3);
const CHINESE_SUMMARY: &str = "没有图形驱动能打开 Flea";
const CHINESE_BODY: &str = "请安装或修复 vulkan-swrast 和 mesa，或在终端中运行 flea --tui。";
const ENGLISH_SUMMARY: &str = "No graphics driver could open Flea";
const ENGLISH_BODY: &str =
    "Install or repair vulkan-swrast and mesa, or run flea --tui in a terminal.";

// An Omarchy roster, a zh-CN language file and a catalog root holding English and zh-CN, in a marked mktemp root.
struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let made = Command::new("mktemp")
            .args(["-d", "/tmp/flea-driver-notice.XXXXXXXX"])
            .output()
            .unwrap();
        assert!(made.status.success());
        let root = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
        std::fs::write(root.join(".flea-test-sandbox"), "driver notice").unwrap();
        std::fs::create_dir_all(root.join("default/i18n")).unwrap();
        std::fs::write(
            root.join("default/i18n/locales.json"),
            include_str!("../../../src/i18n/fixtures/locales.json"),
        )
        .unwrap();
        std::fs::write(root.join("language"), "zh-CN").unwrap();
        for (code, catalog) in [
            (
                "en",
                r#"{"window.no_driver": "No graphics driver could open Flea", "window.no_driver_hint": "Install or repair %1 and %2, or run %3 in a terminal."}"#,
            ),
            (
                "zh-CN",
                r#"{"window.no_driver": "没有图形驱动能打开 Flea", "window.no_driver_hint": "请安装或修复 %1 和 %2，或在终端中运行 %3。"}"#,
            ),
        ] {
            let directory = root.join("catalogs").join(code);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join("flea.json"), catalog).unwrap();
        }
        Self(root)
    }

    fn catalog(&self, catalogs: &str) -> I18n {
        I18n::load(Paths {
            omarchy: self.0.clone(),
            language_file: self.0.join("language"),
            catalogs: vec![self.0.join(catalogs)],
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(self.0.is_absolute() && self.0.starts_with("/tmp"));
        assert!(self.0.join(".flea-test-sandbox").is_file());
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn words(notice: &DriverNotice) -> Vec<String> {
    let command = notice
        .command(false)
        .expect("a launch without a terminal notifies");
    assert_eq!(command.get_program(), "notify-send");
    let args: Vec<_> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(args[..3], FLAGS);
    args[3..].to_vec()
}

#[test]
fn a_zh_cn_desktop_gets_the_notice_in_chinese_with_the_names_literal() {
    let fixture = Fixture::new();
    let catalog = fixture.catalog("catalogs");
    assert_eq!(catalog.language(), "zh-CN");
    assert_eq!(
        words(&DriverNotice::new(&catalog)),
        [CHINESE_SUMMARY, CHINESE_BODY]
    );
}

#[test]
fn a_start_whose_catalogs_did_not_load_gets_the_notice_in_english() {
    let fixture = Fixture::new();
    let catalog = fixture.catalog("missing");
    assert_eq!(catalog.language(), "zh-CN");
    assert_eq!(
        words(&DriverNotice::new(&catalog)),
        [ENGLISH_SUMMARY, ENGLISH_BODY]
    );
}

#[test]
fn a_terminal_launch_reads_stderr_and_gets_no_desktop_notice() {
    let fixture = Fixture::new();
    assert!(DriverNotice::new(&fixture.catalog("catalogs"))
        .command(true)
        .is_none());
}

#[test]
fn a_stuck_notify_send_holds_the_exit_for_the_wait_and_no_longer() {
    // A notify-send with no daemon to answer it stands in as a sleep past the upper bound below.
    let stuck_for = WAIT + SCHEDULING_SLACK + STUCK_PAST_BOUND;
    let mut stuck = Command::new("sleep");
    stuck.arg(stuck_for.as_secs_f64().to_string());
    let started = Instant::now();
    show(stuck);
    let waited = started.elapsed();
    assert!(waited >= WAIT, "returned after {waited:?}, before the wait");
    assert!(
        waited < WAIT + SCHEDULING_SLACK,
        "held the exit for {waited:?}"
    );
}
