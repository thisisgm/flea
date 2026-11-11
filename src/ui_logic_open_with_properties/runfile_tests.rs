// tests/js/runfile.js and run-card-sink.qml's decisions, with the board's three buttons and Run behind Shift.
use super::english;
use super::runfile::*;
use Button::*;

const SCRIPT_ASK: &str = r#"{"ask":"run","kind":"script","size":9420,"handlerId":"","handlerName":"","dev":"8","ino":"9","mtimeS":11,"mtimeNs":12,"target":"/d/tool.sh"}"#;

#[test]
fn each_kind_draws_the_boards_buttons() {
    assert_eq!(buttons_for(Kind::Script, false), vec![Cancel, Text, Terminal], "Run folds into Run in Terminal");
    assert_eq!(buttons_for(Kind::Elf, false), vec![Cancel, Terminal], "a binary is not text");
    assert_eq!(buttons_for(Kind::AppImage, false), vec![Cancel, Terminal]);
    assert_eq!(buttons_for(Kind::AppImage, true), vec![Cancel, Handler, Terminal], "the named handler is offered first");
    assert_eq!(buttons_for(Kind::Script, true), vec![Cancel, Text, Terminal]);
}

#[test]
fn labels_take_title_case_and_shift_relabels_the_default() {
    let i18n = english();
    let labels: Vec<String> = buttons_for(Kind::Script, false).into_iter().map(|b| label_for(&i18n, b, "", false)).collect();
    assert_eq!(labels.join("|"), "Cancel|Open as Text|Run in Terminal");
    assert_eq!(label_for(&i18n, Terminal, "", true), "Run", "holding Shift relabels it Run");
    assert_eq!(label_for(&i18n, Handler, "AppImage Manager", false), "AppImage Manager", "the handler is the app's own name");
    assert_eq!(label_for(&i18n, Handler, "AppImage Manager", true), "AppImage Manager", "Shift relabels nothing else");
}

#[test]
fn the_ring_opens_on_the_default_and_a_stray_tweak_falls_back() {
    assert_eq!(start_focus(&buttons_for(Kind::Script, false), Some(Terminal)), Terminal);
    assert_eq!(start_focus(&buttons_for(Kind::Script, false), None), Terminal);
    assert_eq!(start_focus(&buttons_for(Kind::AppImage, true), None), Handler);
    assert_eq!((default_button(Kind::AppImage, true), default_button(Kind::Script, false)), (Handler, Terminal));
    assert_eq!(start_focus(&buttons_for(Kind::Script, false), Some(Text)), Text, "a tweak naming a drawn button moves it");
    assert_eq!(start_focus(&buttons_for(Kind::AppImage, false), Some(Text)), Terminal);
}

#[test]
fn keys_walk_the_ring_stopping_or_coming_round() {
    let row = [Cancel, Text, Terminal];
    assert_eq!((moved(&row, Cancel, Step::Left), moved(&row, Terminal, Step::Right)), (Cancel, Terminal), "h and l stop at the ends");
    assert_eq!(moved(&row, Terminal, Step::Tab), Cancel, "Tab comes round");
    assert_eq!(moved(&row, Cancel, Step::Backtab), Terminal, "Backtab comes round the other way");
    assert_eq!(moved(&row, Handler, Step::Right), Terminal, "an unknown focus starts from the default");
}

#[test]
fn the_name_row_carries_its_glyph_kind_and_size() {
    let i18n = english();
    assert_eq!(glyph_for(Kind::Script, ""), "code");
    assert_eq!(glyph_for(Kind::AppImage, ""), "app-window");
    assert_eq!(format!("{}|{}", glyph_for(Kind::Elf, "terminal"), glyph_for(Kind::AppImage, "file")), "terminal|file");
    assert_eq!(glyph_for(Kind::Script, "terminal"), "code", "a script keeps the board's code glyph");
    assert_eq!(title(&i18n, "hello.sh"), "hello.sh is a program.", "the title is a full sentence");
    assert_eq!(subtitle(&i18n, Kind::Script, 9420), "Shell script \u{b7} 9.4 kB");
    assert_eq!(subtitle(&i18n, Kind::AppImage, 96_000_000), "AppImage application bundle \u{b7} 96.0 MB");
    assert_eq!(subtitle(&i18n, Kind::Elf, 10), "Program \u{b7} 10 B");
    assert_eq!(subtitle(&i18n, Kind::Script, 21), "Shell script \u{b7} 21 B", "the board's hello.sh row");
}

#[test]
fn the_ask_line_parses_and_refuses_what_is_not_a_run_ask() {
    let ask = parse_ask(SCRIPT_ASK).expect("a run ask");
    assert_eq!((ask.kind, ask.size, ask.has_handler()), (Kind::Script, 9420, false));
    let named = parse_ask(r#"{"ask":"run","kind":"appimage","size":8,"handlerId":"a.desktop","handlerName":"AppImage Manager","dev":"8","ino":"9","mtimeS":11,"mtimeNs":12,"target":"/d/app"}"#).unwrap();
    assert_eq!((named.has_handler(), named.handler_id.as_str(), named.handler_name.as_str()), (true, "a.desktop", "AppImage Manager"));
    let unnamed = parse_ask(r#"{"ask":"run","kind":"appimage","size":8,"handlerId":"a.desktop","handlerName":" ","dev":"8","ino":"9","mtimeS":11,"mtimeNs":12,"target":"/d/app"}"#).unwrap();
    assert!(!unnamed.has_handler(), "a handler with no name offers no handler");
    assert_eq!(parse_ask(r#"{"ask":"openWith"}"#), None);
    assert_eq!(parse_ask("hello"), None);
    assert_eq!(parse_ask(r#"{"ask":"run","kind":"mach-o","size":8}"#), None);
    assert_eq!(parse_ask(r#"{"ask":"run","kind":"script","size":8,"dev":8,"ino":"9","target":"/x"}"#), None, "a numeric dev is malformed");
    assert_eq!(parse_ask(r#"{"ask":"run","kind":"script","size":8,"dev":"8","ino":"9"}"#), None, "an ask without its target is malformed");
    let strings = parse_ask(r#"{"ask":"run","kind":"script","size":8,"handlerId":"","handlerName":"","dev":"18446744073709551615","ino":"9","mtimeS":"11","mtimeNs":"12","target":"/real.sh"}"#).unwrap();
    assert_eq!((strings.dev.as_str(), strings.mtime_s, strings.mtime_ns, strings.target.as_str()), ("18446744073709551615", 11, 12, "/real.sh"), "dev and ino stay strings end to end");
}

#[test]
fn spoofed_names_show_escaped_and_clean_names_pass_through() {
    assert_eq!(escape_name("a\u{061C}b"), "a<U+061C>b");
    assert_eq!(escape_name("a\u{2028}b"), "a<U+2028>b");
    assert_eq!(escape_name("a\u{2029}b"), "a<U+2029>b");
    assert_eq!(escape_name("a\u{202E}b.sh"), "a<U+202E>b.sh");
    assert_eq!(escape_name("a\tb"), "a<U+0009>b");
    assert_eq!(escape_name("plain.sh"), "plain.sh");
    assert_eq!(title(&english(), "x\u{202E}y"), "x<U+202E>y is a program.");
}

#[test]
fn every_choice_leaves_with_the_ask_identity() {
    let ask = Ask { kind: Kind::AppImage, size: 10, handler_id: "a.desktop".into(), handler_name: "A".into(), dev: "8".into(), ino: "9".into(), mtime_s: 11, mtime_ns: 12, target: "/d/tool.sh".into() };
    let lines: Vec<String> = [Action::Run, Action::Terminal, Action::Text, Action::Handler].into_iter().map(|a| argv("flea", a, &ask).join("|")).collect();
    assert_eq!(lines.join(","), "flea|--run|/d/tool.sh|8|9|10|11|12,flea|--run-terminal|/d/tool.sh|8|9|10|11|12,flea|--open-text|/d/tool.sh,flea|--open-handler|/d/tool.sh|a.desktop|8|9|10|11|12");
}

#[test]
fn each_exit_names_its_own_failure_and_a_quiet_exit_says_nothing() {
    let i18n = english();
    let said = |action, status| match exit_outcome(&i18n, action, status) { Exit::Message(text) => text, other => format!("{other:?}") };
    assert_eq!(exit_outcome(&i18n, Action::Run, Some(0)), Exit::Quiet);
    assert_eq!(exit_outcome(&i18n, Action::Terminal, Some(IS_DIRECTORY)), Exit::Navigate);
    assert_eq!(said(Action::Handler, Some(RUN_CHANGED)), "That file changed after Flea checked it, so it was not opened.");
    assert_eq!(said(Action::Run, Some(RUN_CHANGED)), "That file changed after Flea checked it, so it was not run.");
    assert_eq!(said(Action::Run, Some(RUN_NOEXEC)), "That program is on a volume that does not allow running files.");
    assert_eq!(said(Action::Handler, Some(RUN_HANDLER_GONE)), "That application is no longer installed.");
    assert_eq!(said(Action::Terminal, Some(RUN_SPAWN)), "Nothing on this system could run that file in a terminal.");
    assert_eq!(said(Action::Text, Some(RUN_SPAWN)), "Nothing on this system could open that file as text.");
    assert_eq!(said(Action::Run, Some(1)), "That program could not be run.");
    assert_eq!(said(Action::Handler, None), "That application did not open the file.", "a launch that never started is a failure");
    assert_eq!(said(Action::Text, Some(2)), "No text editor opened that file.");
    assert_eq!(i18n.tr("dialogs.file.run.unchecked"), "Flea could not check that file, so it was not run.");
}

#[test]
fn the_exit_statuses_are_the_ones_src_runfile_answers_with() {
    let source = include_str!("../runfile.rs");
    for line in ["pub const ASK_RUN: i32 = 4;", "pub const ASK_OPEN_WITH: i32 = 5;", "pub const RUN_CHANGED: i32 = 6;", "pub const RUN_NOEXEC: i32 = 7;", "pub const RUN_SPAWN: i32 = 8;", "pub const RUN_HANDLER_GONE: i32 = 9;"] {
        assert!(source.contains(line), "{line}");
    }
    assert!(include_str!("../open.rs").contains("pub const IS_DIRECTORY: i32 = 3;"));
    assert_eq!((ASK_RUN, ASK_OPEN_WITH, RUN_CHANGED, RUN_NOEXEC, RUN_SPAWN, RUN_HANDLER_GONE), (4, 5, 6, 7, 8, 9));
}
