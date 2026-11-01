use super::*;
use crate::tui::render::frame_tests::entry;

#[test]
fn t3_codicons_seti_and_documented_exceptions_match_reference_codepoints() {
    for (name, expected) in [("main.rs", "\u{e68b}"), ("todo.md", "\u{eb1d}"), ("table.ods", "\u{ebb7}"),
        ("slides.odp", "\u{f0428}"), ("data.json", "\u{e60b}"), ("report.pdf", "\u{eaeb}"), ("notes.txt", TEXT)] {
        assert_eq!(row(&entry(name, false, 0), true), expected, "{name}");
    }
    let mut folder = entry("A", true, 0);
    assert_eq!(row(&folder, true), FOLDER);
    folder.link = "target".into();
    assert_eq!(row(&folder, true), "\u{eaed}");
    folder.directory = false;
    assert_eq!(row(&folder, true), "\u{eaee}");
    assert_eq!(row(&folder, false), " ");
    folder.icon = "folder".into();
    assert_eq!(row(&folder, true), "\u{eaed}");
    assert_eq!(row(&folder, false), "›");
}
#[test]
fn t3_probe_success_refusal_missing_tool_timeout_and_output_are_bounded() {
    if crate::tui::render::frame_tests::isolated_test("tui::icons::tests::t3_probe_success_refusal_missing_tool_timeout_and_output_are_bounded", &[]) { return; }
    let cancel = AtomicBool::new(false);
    assert_eq!(output(Command::new("sh").args(["-c", "printf covered"]), &cancel).as_deref(), Some("covered"));
    assert_eq!(collect_output(Command::new("sh").args(["-c", "exit 1"]), &cancel), Err(Stop::Refused));
    assert_eq!(collect_output(&mut Command::new("/nonexistent-flea-font-probe"), &cancel), Err(Stop::Failed));
    assert_eq!(collect_output(Command::new("sh").args(["-c", "while :; do printf aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa; done"]), &cancel), Err(Stop::OutputLimit));
    let start = Instant::now();
    assert_eq!(collect_output(Command::new("sh").args(["-c", "sleep 2"]), &cancel), Err(Stop::Deadline));
    assert!(start.elapsed() < Duration::from_secs(1));
    // A command that would succeed shows the cancel branch alone, since no deadline or cap can end it first.
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(collect_output(Command::new("sh").args(["-c", "printf covered"]), &cancel), Err(Stop::Cancelled));
}
#[test]
fn t3_builtin_marks_survive_missing_font_but_forced_test_fallback_is_explicit() {
    let mut m = crate::tui::render::frame_tests::model();
    m.font_marks = false;
    assert!(!m.marks());
    m.capabilities.graphics = crate::tui::capabilities::Protocol::Kitty;
    assert!(m.marks());
    m.force_text_marks = true;
    assert!(!m.marks());
}
#[path = "icons_signal_tests.rs"]
mod signal_tests;
// tui-3d-column: G-code, which the preview draws as a 3D toolpath, takes the 3D mark beside the model files.
#[test]
fn gcode_takes_the_3d_mark_as_the_board_draws_it() {
    for name in ["benchy.gcode", "benchy.bgcode", "bracket.stl", "shelf-kit.3mf"] {
        assert_eq!(row(&entry(name, false, 0), true), "\u{eb29}", "{name}");
    }
}
