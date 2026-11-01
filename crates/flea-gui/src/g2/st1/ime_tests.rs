//! The mounted Settings fields exercise the platform handler, rather than appending raw key characters.
use super::{
    tests::{english, panel},
    SettingsView, Sheet, SheetView, Sources,
};
use gpui::{Entity, PlatformInputHandler, PlatformWindow, TestAppContext, VisualTestContext};

pub(super) fn settings(cx: &mut TestAppContext) -> (Entity<SettingsView>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = SettingsView::new(panel("{}"), cx);
        window.focus(&view.focus, cx);
        view
    });
    cx.simulate_keystrokes("/");
    draw(cx);
    (view, cx)
}

pub(super) fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

pub(super) fn platform<R>(
    cx: &mut VisualTestContext,
    f: impl FnOnce(&mut PlatformInputHandler) -> R,
) -> R {
    let handle = cx.update(|window, _| window.window_handle());
    let mut window = cx.cx.test_window(handle);
    let mut handler = window
        .take_input_handler()
        .expect("focused field registers platform text input");
    let result = f(&mut handler);
    window.set_input_handler(handler);
    cx.run_until_parked();
    result
}

#[gpui::test]
fn i50_settings_preedit(cx: &mut TestAppContext) {
    let (view, cx) = settings(cx);
    platform(cx, |input| {
        input.replace_and_mark_text_in_range(None, "a\u{1d11e}語", Some(1..3));
        assert_eq!(input.marked_text_range(), Some(0..4));
        assert_eq!(input.selected_text_range(false).unwrap().range, 1..3);
        let mut adjusted = None;
        assert_eq!(
            input.text_for_range(2..3, &mut adjusted).as_deref(),
            Some("\u{1d11e}")
        );
        assert_eq!(adjusted, Some(1..3));
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.query.as_deref(), Some(""))
    });
    draw(cx);
    platform(cx, |input| {
        let band = input.bounds_for_range(1..3).unwrap();
        assert!(band.size.width > gpui::px(0.));
        assert!(band.size.height > gpui::px(0.));
    });
    cx.simulate_keystrokes("enter down up tab");
    platform(cx, |input| {
        assert_eq!(input.marked_text_range(), Some(0..4))
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.query.as_deref(), Some(""));
        assert!(view.panel.requests.is_empty());
    });
}

#[gpui::test]
fn i50_settings_query_commit(cx: &mut TestAppContext) {
    let (view, cx) = settings(cx);
    platform(cx, |input| {
        input.replace_and_mark_text_in_range(None, "thu", None);
        input.replace_text_in_range(None, "thumb");
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.query.as_deref(), Some("thumb"));
        assert!(view.panel.hit_target().is_some());
    });
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.panel.query.is_none());
        assert_eq!(view.panel.section, "preview");
    });
}

#[gpui::test]
fn i50_settings_escape_order(cx: &mut TestAppContext) {
    let (view, cx) = settings(cx);
    platform(cx, |input| {
        input.replace_and_mark_text_in_range(None, "検索", None)
    });
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.query.as_deref(), Some(""))
    });
    platform(cx, |input| assert_eq!(input.marked_text_range(), None));
    let handle = cx.update(|window, _| window.window_handle());
    let mut retained = cx.cx.test_window(handle).take_input_handler().unwrap();
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| assert!(view.panel.query.is_none()));
    retained.replace_text_in_range(None, "late");
    assert!(retained.selected_text_range(false).is_none());
    view.read_with(cx, |view, _| assert!(view.panel.query.is_none()));
}

#[gpui::test]
fn i50_settings_retained_handler_cannot_write_reopened_query(cx: &mut TestAppContext) {
    let (view, cx) = settings(cx);
    let handle = cx.update(|window, _| window.window_handle());
    let mut retained = cx.cx.test_window(handle).take_input_handler().unwrap();
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| assert!(view.panel.query.is_none()));
    cx.simulate_keystrokes("/");
    draw(cx);
    retained.replace_text_in_range(None, "old");
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.query.as_deref(), Some(""));
    });
    assert!(retained.selected_text_range(false).is_none());
    platform(cx, |input| input.replace_text_in_range(None, "current"));
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.query.as_deref(), Some("current"));
    });
}

#[gpui::test]
fn i50_settings_key_capture_not_text(cx: &mut TestAppContext) {
    let (view, cx) = settings(cx);
    let before = view.read_with(cx, |view, _| view.panel.document().clone());
    platform(cx, |input| {
        input.replace_and_mark_text_in_range(None, "ctrl-x", None)
    });
    cx.simulate_keystrokes("ctrl-x enter");
    view.read_with(cx, |view, _| {
        assert_eq!(*view.panel.document(), before);
        assert_eq!(view.panel.query.as_deref(), Some(""));
    });
}

#[gpui::test]
fn i50_settings_locale_active_query(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = SheetView::new(Sheet::new("default", english(), Sources::default()), cx);
        window.focus(&view.focus, cx);
        view
    });
    draw(cx);
    platform(cx, |input| input.replace_text_in_range(None, "mute"));
    draw(cx);
    platform(cx, |input| {
        input.replace_and_mark_text_in_range(None, "語", Some(1..1))
    });
    draw(cx);
    let old_line = view.read_with(cx, |view, _| {
        view.editor.drawn.borrow().line.run.clone().unwrap()
    });
    view.update(cx, |view, cx| {
        view.sheet.set_catalog(french());
        cx.notify();
    });
    draw(cx);
    platform(cx, |input| {
        assert_eq!(input.marked_text_range(), Some(4..5));
        assert_eq!(input.selected_text_range(false).unwrap().range, 5..5);
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.sheet.query, "mute");
        assert_eq!(view.sheet.catalog.language(), "fr");
        assert!(!std::rc::Rc::ptr_eq(
            &old_line,
            view.editor.drawn.borrow().line.run.as_ref().unwrap()
        ));
        assert!(view.sheet.ranked.is_empty());
        assert!(view.sheet.rows.iter().any(|row| row.label == "Silence"));
    });
}

#[gpui::test]
fn i50_settings_sheet_commit_and_candidate_keys(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = SheetView::new(Sheet::new("default", english(), Sources::default()), cx);
        window.focus(&view.focus, cx);
        view
    });
    draw(cx);
    platform(cx, |input| {
        input.replace_and_mark_text_in_range(None, "perm", None)
    });
    draw(cx);
    view.read_with(cx, |view, _| {
        assert!(view.sheet.query.is_empty());
        assert!(view.sheet.ranked.is_empty());
        assert!(view.editor.drawn.borrow().marked.is_some());
    });
    cx.simulate_keystrokes("enter up down tab");
    view.read_with(cx, |view, _| assert!(view.sheet.decided.is_none()));
    platform(cx, |input| input.replace_text_in_range(None, "mute"));
    view.read_with(cx, |view, _| {
        assert_eq!(view.sheet.query, "mute");
        assert_eq!(view.sheet.ranked.len(), 1);
    });
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| assert!(view.sheet.decided.is_some()));
}

#[gpui::test]
fn i50_settings_edit_selection_unicode_and_no_duplicate_raw_text(cx: &mut TestAppContext) {
    let (view, cx) = settings(cx);
    cx.simulate_keystrokes("a b c");
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.query.as_deref(), Some("abc"))
    });
    cx.simulate_keystrokes("ctrl-a");
    platform(cx, |input| input.replace_text_in_range(None, "\u{1d11e}語"));
    cx.simulate_keystrokes("left backspace");
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.query.as_deref(), Some("語"))
    });
    platform(cx, |input| input.replace_text_in_range(Some(0..1), "thumb"));
    cx.simulate_keystrokes("ctrl-a ctrl-c end ctrl-v");
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.query.as_deref(), Some("thumbthumb"))
    });
}

#[gpui::test]
fn i50_settings_unmark_commits_and_clear_cancels_preedit(cx: &mut TestAppContext) {
    let (view, cx) = settings(cx);
    platform(cx, |input| {
        input.replace_and_mark_text_in_range(None, "thumb", None);
        input.unmark_text();
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.query.as_deref(), Some("thumb"))
    });
    platform(cx, |input| {
        input.replace_and_mark_text_in_range(None, "語", None)
    });
    draw(cx);
    let clear = view.read_with(cx, |view, _| {
        view.probe.bounds("search-clear").unwrap().center()
    });
    cx.simulate_click(clear, gpui::Modifiers::none());
    draw(cx);
    platform(cx, |input| {
        assert_eq!(input.marked_text_range(), None);
        assert_eq!(input.selected_text_range(false).unwrap().range, 0..0);
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.query.as_deref(), Some(""));
        assert!(view.editor.entry.text.is_empty());
    });
}
fn french() -> std::sync::Arc<flea::i18n::I18n> {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.superpowers/m9-i50-17")
        .join(format!(
            "locale-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    assert!(root.is_absolute());
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join(".flea-test-sandbox"), "I50-17").unwrap();
    fs::create_dir_all(root.join("default/i18n")).unwrap();
    fs::create_dir_all(root.join("fr")).unwrap();
    fs::write(
        root.join("default/i18n/locales.json"),
        r#"{"en":{},"fr":{"name":"Français","formatLocale":"fr-FR"}}"#,
    )
    .unwrap();
    fs::write(root.join("language"), "fr").unwrap();
    fs::write(root.join("fr/flea.json"), r#"{"keys.action.mute":"Silence","keys.label.8dd6857baf02":"Silence","settings.group.thumbnails":"Vignettes","settings.row.thumbnails":"Vignettes","settings.row.thumb_size":"Taille des vignettes","settings.hint.thumbs_kept":"Les vignettes existantes sont conservées."}"#).unwrap();
    std::sync::Arc::new(flea::i18n::I18n::load(flea::i18n::Paths {
        omarchy: root.clone(),
        language_file: root.join("language"),
        catalogs: vec![root],
    }))
}

#[gpui::test]
fn i50_settings_locale_settings_query(cx: &mut TestAppContext) {
    let (view, cx) = settings(cx);
    platform(cx, |input| input.replace_text_in_range(None, "thumb"));
    draw(cx);
    platform(cx, |input| {
        input.replace_and_mark_text_in_range(None, "語", None)
    });
    let before = view.update(cx, |view, _| {
        view.panel.hit = view
            .panel
            .results
            .iter()
            .map(|group| group.hits.len().max(1))
            .sum::<usize>()
            - 1;
        (view.panel.hit_target(), view.panel.results.len())
    });
    view.update(cx, |view, cx| {
        view.panel.set_catalog(french());
        cx.notify();
    });
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.query.as_deref(), Some("thumb"));
        assert_eq!(view.panel.catalog.language(), "fr");
        assert_eq!(view.panel.hit_target(), before.0);
        assert!(view.panel.results.len() < before.1);
        assert_eq!(view.editor.entry.text, "thumb語");
    });
    platform(cx, |input| {
        assert_eq!(input.marked_text_range(), Some(5..6));
        assert_eq!(input.selected_text_range(false).unwrap().range, 6..6);
    });
}
