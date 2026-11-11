//! The card's chrome: a code file's bar wears its language logo, and an archive card opens with no bar at all.
use super::preview_tests::{body, close, draw, file, opened};
use super::quick_look_body_tests::{mounted, open_kind};
use super::test_shell::{land, set_cursor};
use crate::{
    g2::quick_look_overlay::{doc::View, Body},
    state::Shell,
    window::u07_tests::read,
};
use flea::{jsondoc, ui_logic_doc_preview::archive};
use gpui::{Entity, TestAppContext, VisualTestContext};

#[path = "quick_look_symlink_tests.rs"]
mod symlink;

#[path = "fixreaders_ey190_tests.rs"]
mod fixreaders_ey190;

/// The key of the mark the open card's bar paints, read from the drawn element, not the subject.
fn bar_mark(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Option<String> {
    let key = shell.read_with(cx, |shell, cx| {
        let host = shell.quick_look.as_ref()?;
        Some(host.view.read(cx).header_mark()?.svg().0.to_string())
    })?;
    // debug_bounds takes a 'static selector; a test leaks the few it builds.
    let selector: &'static str = Box::leak(format!("pv1-bar-mark:{key}").into_boxed_str());
    let drawn = cx.debug_bounds(selector).is_some();
    drawn.then_some(key)
}

#[gpui::test]
fn the_bar_wears_each_code_files_language_logo(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![
            file("main.rs", "text-x-rust"),
            file("app.py", "text-x-python"),
            file("app.js", "text-x-javascript"),
            file("run.sh", "text-x-script"),
            file("notes.txt", "text-plain"),
        ],
        1,
    );
    let expected = [
        (0, "flea-lang:rust"),
        (1, "flea-lang:python"),
        (2, "flea-lang:javascript"),
        (3, "flea-lang:gnubash"),
        (4, "flea-mark:file-text"),
    ];
    for (row, key) in expected {
        set_cursor(&shell, cx, row);
        cx.simulate_keystrokes("space");
        draw(cx);
        assert_eq!(
            bar_mark(&shell, cx).as_deref(),
            Some(key),
            "row {row} bar mark"
        );
        cx.simulate_keystrokes("escape");
        draw(cx);
    }
    close(&shell, cx);
}

#[gpui::test]
fn language_marks_off_keeps_each_kind_mark_in_the_bar(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = opened(cx);
    shell.update(cx, |shell, _| {
        shell.launch.settings.data = jsondoc::parse(r#"{"langMarks":false}"#).unwrap()
    });
    land(
        &shell,
        cx,
        "/files",
        vec![file("main.rs", "text-x-rust")],
        1,
    );
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    draw(cx);
    assert_eq!(bar_mark(&shell, cx).as_deref(), Some("flea-mark:file"));
    close(&shell, cx);
}

#[gpui::test]
fn the_archive_card_opens_on_its_name_and_entries_with_no_bar(cx: &mut TestAppContext) {
    let (shell, cx, _requests, _script) = mounted(cx);
    open_kind(&shell, cx, "pack.zip");
    let Some(Body::Doc(doc)) = body(&shell, cx) else {
        panic!("the archive mounted")
    };
    let View::Archive(view) = doc.view else {
        panic!("an archive view")
    };
    let meta = archive::Meta {
        entries: 1,
        unpacked: 156,
        failed: false,
        names: vec![("main.rs".into(), false)],
    };
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            assert!(view.answered("/files/pack.zip", meta.clone(), cx))
        })
    });
    draw(cx);
    assert!(
        cx.debug_bounds("pv1-bar").is_none(),
        "an archive card draws no bar"
    );
    let drawn: Vec<String> = cx
        .read(|cx| view.read(cx).drawn())
        .into_iter()
        .map(|(text, _)| text)
        .collect();
    let line = archive::line(&crate::test_catalogs::english(), &meta);
    assert_eq!(drawn, ["pack.zip".to_string(), line, "main.rs".to_string()]);
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(
        read(&shell, cx, "previewOpen", &[]),
        "false",
        "Escape closes the archive"
    );
    close(&shell, cx);
}

#[gpui::test]
fn an_unreadable_archive_draws_its_recipe_at_the_card_top_with_no_bar(cx: &mut TestAppContext) {
    let (shell, cx, _requests, _script) = mounted(cx);
    open_kind(&shell, cx, "pack.zip");
    let Some(Body::Doc(doc)) = body(&shell, cx) else {
        panic!("the archive mounted")
    };
    let View::Archive(view) = doc.view else {
        panic!("an archive view")
    };
    let meta = archive::Meta {
        entries: 0,
        unpacked: 0,
        failed: true,
        names: Vec::new(),
    };
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            assert!(view.answered("/files/pack.zip", meta.clone(), cx))
        })
    });
    draw(cx);
    assert!(
        cx.debug_bounds("pv1-bar").is_none(),
        "a failed archive draws no bar"
    );
    let card = cx.debug_bounds("pv1-card").expect("the card is drawn");
    let recipe = cx
        .debug_bounds("pv1-recipe")
        .expect("the failure recipe is drawn");
    let offset = recipe.origin.y - card.origin.y;
    assert!(
        offset >= gpui::px(0.) && offset <= gpui::px(1.),
        "the recipe starts at the card top, inside its frame: {offset:?}"
    );
    close(&shell, cx);
}

#[gpui::test]
fn the_view_words_sit_14_px_apart_and_close_follows_at_the_bar_gap(cx: &mut TestAppContext) {
    let (shell, cx, _requests, _script) = mounted(cx);
    open_kind(&shell, cx, "readme.md");
    let [rendered, source, close_slot] =
        ["pv1-word-0", "pv1-word-1", "pv1-close"].map(|selector| {
            cx.debug_bounds(selector)
                .unwrap_or_else(|| panic!("the bar draws {selector}"))
        });
    assert_eq!(
        f32::from(source.left() - rendered.right()),
        14.,
        "Rendered and Source keep the boards' word gap"
    );
    assert_eq!(
        f32::from(close_slot.left() - source.right()),
        9.,
        "close keeps the bar's gap after the last word"
    );
    close(&shell, cx);
}

#[gpui::test]
fn a_mounted_documents_bar_follows_the_language_marks_setting(cx: &mut TestAppContext) {
    // The mounted chrome bar reads the card's live langMarks, as the plain card's bar does.
    let (shell, cx, _requests, _script) = mounted(cx);
    open_kind(&shell, cx, "readme.md");
    assert_eq!(bar_mark(&shell, cx).as_deref(), Some("flea-lang:markdown"));
    cx.simulate_keystrokes("escape");
    draw(cx);
    shell.update(cx, |shell, _| {
        shell.launch.settings.data = jsondoc::parse(r#"{"langMarks":false}"#).unwrap()
    });
    open_kind(&shell, cx, "readme.md");
    let off = bar_mark(&shell, cx).expect("the mounted bar draws its mark");
    assert!(
        !off.starts_with("flea-lang:"),
        "langMarks off keeps the kind mark: {off}"
    );
    close(&shell, cx);
}
