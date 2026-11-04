use super::{
    epoch::{config, faces_differ, launch_from, tag, Launch, LAUNCH_WAIT},
    fixture::Fixture,
};
use flea::i18n::{I18n, Paths};
use gpui::{px, TestAppContext, TestDispatcher, TextRun};
use std::{borrow::Cow, time::Duration};

const FAMILY: &str = "JetBrains Mono";
const ROSTER: &str = r#"{"en":{"name":"English","locale":"en_US.UTF-8"},"ja":{"name":"日本語","locale":"ja_JP.UTF-8"},"de":{"name":"Deutsch","locale":"de_DE.UTF-8"},"zh-CN":{"name":"简体中文","locale":"zh_CN.UTF-8"}}"#;

fn catalog(fixture: &Fixture, language: &str) -> I18n {
    let root = fixture.path();
    fixture.write(
        &root.join("omarchy/default/i18n/locales.json"),
        ROSTER.as_bytes(),
    );
    fixture.write(&root.join("language"), language.as_bytes());
    I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: Vec::new(),
    })
}

fn context() -> TestAppContext {
    let cx = TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        gpui_platform::current_platform(true).text_system(),
    );
    // The fork prepares on its own thread, so the launch wait parks this one.
    cx.executor().allow_parking();
    cx.update(|cx| {
        cx.text_system()
            .add_fonts(vec![Cow::Borrowed(
                include_bytes!("../../tests/fonts/JetBrainsMono-Regular.ttf").as_slice(),
            )])
            .unwrap();
    });
    cx
}

// Every run a window shapes now carries the epoch the application installed.
fn shaped_in_window_is_current(cx: &mut TestAppContext) -> bool {
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let text = "日本語 Flea";
        let run = TextRun {
            len: text.len(),
            font: gpui::font(FAMILY),
            color: window.text_style().color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let layout = window
            .text_system()
            .layout_line(text, px(14.), &[run], None);
        !layout.runs.is_empty() && cx.text_system().accepts_layout(&layout)
    })
}

#[test]
fn i50_font_tag_reads_posix_locales_as_the_fork_does() {
    assert_eq!(tag("ja_JP.UTF-8"), "ja-JP");
    assert_eq!(tag("ja_JP.UTF-8@euro"), "ja-JP");
    assert_eq!(tag("zh_TW"), "zh-TW");
    for neutral in ["", "C", "POSIX", "C.UTF-8", "x y.UTF-8", "ja__JP"] {
        assert_eq!(tag(neutral), "en-US", "{neutral:?}");
    }
}

#[test]
fn i50_font_faces_differ_only_where_han_forms_can() {
    // Either side Han, in either order, and two Han languages against each other, all change faces.
    for (roster, process) in [
        ("ja-JP", "en-US"),
        ("de-DE", "ja-JP"),
        ("ja-JP", "zh-CN"),
        ("zh-TW", "zh-CN"),
        ("ko-KR", "ja-JP"),
    ] {
        assert!(faces_differ(roster, process), "{roster} over {process}");
    }
    for (roster, process) in [("ja-JP", "ja-JP"), ("de-DE", "en-US"), ("ar-AE", "en-US")] {
        assert!(!faces_differ(roster, process), "{roster} over {process}");
    }
}

#[test]
fn i50_font_same_faces_prepare_nothing() {
    let fixture = Fixture::new("i50-09-same");
    let ja = catalog(&fixture, "ja");
    let de = catalog(&fixture, "de");
    let cx = context();
    let before = cx.update(|cx| cx.text_system().text_epoch());
    // The same tag, and two languages without Han forms, keep the process faces and their on-demand loading.
    for (process, catalog) in [("ja_JP.UTF-8", &ja), ("en_US.UTF-8", &de), ("x y", &de)] {
        let launched = cx.update(|cx| launch_from(process, catalog, FAMILY, LAUNCH_WAIT, cx));
        assert_eq!(launched, Launch::Same, "{process}");
    }
    assert_eq!(cx.update(|cx| cx.text_system().text_epoch()), before);
    drop(cx);
    fixture.cleanup();
}

#[test]
fn i50_font_config_carries_the_roster_locale() {
    let fixture = Fixture::new("i50-09-config");
    let zh = catalog(&fixture, "zh-CN");
    let config = config(&zh, FAMILY);
    assert_eq!(config.locale, "zh_CN.UTF-8");
    assert_eq!(config.fonts, vec![gpui::font(FAMILY)]);
    let held: String = config.text.iter().map(|text| text.as_ref()).collect();
    for word in ["日本語", "简体中文", "Deutsch", "\u{1F600}"] {
        assert!(word.chars().all(|c| held.contains(c)), "{word} is not held");
    }
    fixture.cleanup();
}

#[test]
fn i50_font_new_window_epoch() {
    let fixture = Fixture::new("i50-09-window");
    let ja = catalog(&fixture, "ja");
    assert_eq!(ja.locale().glibc_locale, "ja_JP.UTF-8");
    let mut cx = context();
    let before = cx.update(|cx| cx.text_system().text_epoch());
    let launched =
        cx.update(|cx| launch_from("en_US.UTF-8", &ja, FAMILY, Duration::from_secs(30), cx));
    let Launch::Installed(epoch) = launched else {
        panic!("the roster locale was not installed: {launched:?}");
    };
    assert_ne!(epoch, before, "the launch kept the process locale's epoch");
    assert_eq!(cx.update(|cx| cx.text_system().text_epoch()), epoch);
    // Windows opened after the launch, the first and any later one, shape only with the installed faces.
    assert!(shaped_in_window_is_current(&mut cx));
    assert!(shaped_in_window_is_current(&mut cx));
    drop(cx);
    fixture.cleanup();
}

#[test]
fn i50_font_failure_holds_old_frame() {
    let fixture = Fixture::new("i50-09-failure");
    let ja = catalog(&fixture, "ja");
    let mut cx = context();
    let before = cx.update(|cx| cx.text_system().text_epoch());
    // A family the fork cannot resolve fails the preparation itself.
    let absent = cx.update(|cx| {
        launch_from(
            "en_US.UTF-8",
            &ja,
            "Flea Absent Family",
            Duration::from_secs(30),
            cx,
        )
    });
    assert!(
        matches!(&absent, Launch::Kept(error) if error != "text epoch timed out"),
        "{absent:?}"
    );
    assert_eq!(cx.update(|cx| cx.text_system().text_epoch()), before);
    // A launch that cannot wait for fontconfig opens with the process faces.
    let late = cx.update(|cx| launch_from("en_US.UTF-8", &ja, FAMILY, Duration::ZERO, cx));
    assert_eq!(late, Launch::Kept("text epoch timed out".into()));
    assert_eq!(cx.update(|cx| cx.text_system().text_epoch()), before);
    assert!(shaped_in_window_is_current(&mut cx));
    drop(cx);
    fixture.cleanup();
}
