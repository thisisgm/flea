//! The column arm of previewTextState: a drawn column code view, its pending frame, and an empty column.
use super::{
    previewtypes_tests::{answer, ask, field, file, local, meta, open, pump},
    test_shell::land,
};
use crate::{
    state::Shell,
    window::{
        selection_preview::{docs::Doc, Mounted},
        u07_tests::read,
    },
};
use flea::jsondoc::{self, Json};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn state(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Json {
    jsondoc::parse(&read(shell, cx, "previewTextState", &["true"])).unwrap()
}

fn reply(token: u64, first: u64, text: &str) -> String {
    format!(
        r#"{{"t":"code","token":{token},"slot":"column","state":"styled","first":{first},"total":1,"lines":[[5,"fn",0," {text}()"]]}}"#
    )
}

#[gpui::test]
fn pb_preview_text_state_column_reads_the_drawn_code_and_its_wrap(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    land(
        &shell,
        cx,
        "/files",
        vec![file("main.rs", "text-x-rust", "Rust source code", 0o100644)],
        1,
    );
    let lines = local(&shell, cx, "/files");
    let code = ask(&lines, "code").to_owned();
    meta(&shell, cx, &lines, r#","lines":2"#);
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column","state":"styled","first":{},"total":2,"lines":[[5,"fn",0," drawn()"],[5,"fn",0," second()"]]}}"#,
            field(&code, "token"),
            field(&code, "first")
        ),
    );
    pump(cx);
    let value = state(&shell, cx);
    assert_eq!(
        value.get("path").and_then(Json::as_str),
        Some("/files/main.rs")
    );
    assert_eq!(
        value.get("ready").and_then(Json::as_bool),
        Some(true),
        "the drawn code is ready"
    );
    assert_eq!(value.get("wrap").and_then(Json::as_bool), Some(false));
    assert_eq!(value.get("total").and_then(Json::as_f64), Some(2.));
    assert_eq!(
        read(&shell, cx, "columnTextLines", &[]),
        "fn drawn()|fn second()"
    );
    let code = shell.read_with(cx, |shell, cx| {
        match &shell
            .selection_preview(0)
            .unwrap()
            .read(cx)
            .drawn
            .as_ref()
            .unwrap()
            .mounted
        {
            Mounted::Doc(Doc::Code(code)) => code.clone(),
            _ => panic!("the column draws code"),
        }
    });
    code.update(cx, |code, cx| code.set_wrap(true, cx));
    pump(cx);
    assert_eq!(
        state(&shell, cx).get("wrap").and_then(Json::as_bool),
        Some(true),
        "the live wrap key"
    );
}

#[gpui::test]
fn pb_preview_text_state_column_is_not_ready_while_a_pending_frame_waits(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    let rows = vec![
        file("a.rs", "text-x-rust", "Rust source code", 0o100644),
        file("b.rs", "text-x-rust", "Rust source code", 0o100644),
    ];
    land(&shell, cx, "/files", rows, 1);
    let lines = local(&shell, cx, "/files");
    let code = ask(&lines, "code").to_owned();
    meta(&shell, cx, &lines, r#","lines":1"#);
    answer(
        &shell,
        cx,
        &reply(field(&code, "token"), field(&code, "first"), "drawn"),
    );
    pump(cx);
    let value = state(&shell, cx);
    assert_eq!(
        value.get("path").and_then(Json::as_str),
        Some("/files/a.rs")
    );
    assert_eq!(value.get("ready").and_then(Json::as_bool), Some(true));
    cx.simulate_keystrokes("down");
    let lines = pump(cx);
    let next = ask(&lines, "code").to_owned();
    answer(
        &shell,
        cx,
        &reply(field(&next, "token"), field(&next, "first"), "pending"),
    );
    pump(cx);
    let value = state(&shell, cx);
    assert_eq!(
        value.get("path").and_then(Json::as_str),
        Some("/files/a.rs"),
        "the drawn view stays a.rs"
    );
    assert_eq!(
        value.get("ready").and_then(Json::as_bool),
        Some(false),
        "b.rs waits on its meta, so a.rs is not ready"
    );
    meta(&shell, cx, &lines, r#","lines":1"#);
    pump(cx);
    let value = state(&shell, cx);
    assert_eq!(
        value.get("path").and_then(Json::as_str),
        Some("/files/b.rs")
    );
    assert_eq!(
        value.get("ready").and_then(Json::as_bool),
        Some(true),
        "b.rs lands and is ready"
    );
}

#[gpui::test]
fn pb_preview_text_state_column_answers_the_empty_shape_before_any_frame(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    land(
        &shell,
        cx,
        "/files",
        vec![file("a.rs", "text-x-rust", "Rust source code", 0o100644)],
        1,
    );
    assert_eq!(read(&shell, cx, "previewTextState", &["true"]), "{}");
}

// Judge S batch 3241: Ctrl+Tab hands the keyboard to the column, and previewTextState(true) reads it focused, then not after Ctrl+Tab back.
#[gpui::test]
fn fixreaders_preview_text_state_column_reads_the_focused_preview(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    land(
        &shell,
        cx,
        "/files",
        vec![file("main.rs", "text-x-rust", "Rust source code", 0o100644)],
        1,
    );
    let lines = local(&shell, cx, "/files");
    let code = ask(&lines, "code").to_owned();
    meta(&shell, cx, &lines, r#","lines":1"#);
    answer(
        &shell,
        cx,
        &reply(field(&code, "token"), field(&code, "first"), "drawn"),
    );
    pump(cx);
    let focused = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        state(shell, cx).get("focused").and_then(Json::as_bool)
    };
    assert_eq!(focused(&shell, cx), Some(false));
    cx.simulate_keystrokes("ctrl-tab");
    pump(cx);
    assert_eq!(
        focused(&shell, cx),
        Some(true),
        "Ctrl+Tab focuses the column"
    );
    cx.simulate_keystrokes("ctrl-tab");
    pump(cx);
    assert_eq!(focused(&shell, cx), Some(false), "Ctrl+Tab hands it back");
    // A missing column flag is refused in Quickshell's words, never read as the Quick Look arm.
    assert!(read(&shell, cx, "previewTextState", &[])
        .starts_with("Too few arguments provided (1 required but 0 were provided.)"));
}

/// EY-507: SelectionPreview.qml takes the keyboard only from focusPreview, so a click on a too-large file's column leaves the listing focused and Right opens Quick Look.
#[gpui::test]
fn fixpreview_ey507_a_click_on_the_column_preview_keeps_the_listing_keys(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    let rows = vec![file(
        "huge.txt",
        "text-plain",
        "Plain text document",
        0o100644,
    )];
    land(&shell, cx, "/files", rows, 1);
    let lines = local(&shell, cx, "/files");
    let code = ask(&lines, "code").to_owned();
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column","state":"large","first":0,"total":0,"lines":[]}}"#,
            field(&code, "token")
        ),
    );
    pump(cx);
    let frame = cx
        .debug_bounds("column-preview")
        .expect("the column preview is drawn");
    cx.simulate_click(frame.center(), gpui::Modifiers::none());
    pump(cx);
    let preview_focused = cx.update(|window, cx| {
        let preview = shell
            .read(cx)
            .selection_preview(0)
            .expect("the column is mounted");
        let focused = preview.read(cx).focus.is_focused(window);
        focused
    });
    assert!(
        !preview_focused,
        "a click on the preview leaves the listing focused"
    );
    assert_eq!(read(&shell, cx, "focusView", &[]), "list");
    cx.simulate_keystrokes("right");
    pump(cx);
    assert_eq!(
        read(&shell, cx, "previewOpen", &[]),
        "true",
        "Right opens Quick Look on the file"
    );
}

#[gpui::test]
fn sol_preview_remainder_ey545_held_meta_keeps_frame_mark_name_and_facts(cx: &mut TestAppContext) {
    held_column(cx, "meta");
}

#[gpui::test]
fn sol_preview_remainder_ey545_held_html_keeps_frame_mark_name_and_facts(cx: &mut TestAppContext) {
    held_column(cx, "html");
}

fn held_column(cx: &mut TestAppContext, held: &str) {
    use super::previewtypes_kinds_tests::Root;
    cx.executor().allow_parking();
    for colors in [
        "background='#141414'\nforeground='#d0cbc7'",
        "background='#eff1f5'\nforeground='#4c4f69'",
    ] {
        for (name, icon, kind, hold) in [
            ("image-jpg.jpg", "image-jpeg", "JPEG image", "meta"),
            ("code-html.html", "text-html", "HTML document", "html"),
        ] {
            if hold != held {
                continue;
            }
            let root = Root::new("ey545");
            let dir = root.0.to_string_lossy().into_owned();
            let page = root.0.join("page.png");
            image::RgbImage::from_pixel(32, 24, image::Rgb([255; 3]))
                .save(&page)
                .unwrap();
            if hold == "html" {
                std::fs::write(root.0.join(name), "<p>sweep</p>").unwrap();
            } else {
                std::fs::write(
                    root.0.join(name),
                    include_bytes!("../../../tests/fixtures/ey545-image-jpg.jpg"),
                )
                .unwrap();
            }
            let (shell, cx) = super::previewtypes_tests::open_at(cx, &dir, Some(14.));
            cx.update(|_, cx| cx.set_global(crate::palette::Theme::from_text(colors)));
            let mut row = file(name, icon, kind, 0o100644);
            row.size = 4800;
            land(&shell, cx, &dir, vec![row], 1);
            let mut lines = local(&shell, cx, &dir);
            assert!(!ask(&lines, "meta").is_empty(), "meta request is held");
            if hold == "html" {
                meta(&shell, cx, &lines, "");
                lines.extend(pump(cx));
                assert!(ask(&lines, "html").contains(r#""slot":"column""#));
            }
            pump(cx);
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(1200));
            pump(cx);
            assert_eq!(read(&shell, cx, "previewColumnState", &[]), "loading");
            for selector in [
                "column-preview-frame",
                "column-preview-waiting",
                "column-preview-name",
                "column-preview-facts",
            ] {
                assert!(cx.debug_bounds(selector).is_some(), "{name}: {selector}");
            }
            let mark = cx
                .debug_bounds("column-preview-loading-mark")
                .expect("Omi mark");
            assert_eq!(f32::from(mark.size.width), crate::feedback::spinner::HERO);
            assert_eq!(f32::from(mark.size.height), crate::feedback::spinner::HERO);
            assert!(mark_shown(&shell, cx), "{name}: the Omi plays in its slot");
            let facts = read(&shell, cx, "previewFacts", &[]);
            assert_eq!(facts, format!("Kind={kind}|Size=4.8 kB|State=loading"));
            if hold == "html" {
                answer(
                    &shell,
                    cx,
                    &format!(
                        r#"{{"t":"html","token":{},"slot":"column","state":"ready","file":"{}","total":1,"width":32,"title":""}}"#,
                        field(ask(&lines, "html"), "token"),
                        page.display()
                    ),
                );
            } else {
                meta(&shell, cx, &lines, r#","w":320,"h":240"#);
                super::previewtypes_tests::thumbed(&shell, cx, &lines, 0, "");
            }
            pump(cx);
            assert!(cx.debug_bounds("column-preview-waiting").is_none());
            assert!(
                !mark_shown(&shell, cx),
                "{name}: the landed frame stops the Omi"
            );
            assert_ne!(read(&shell, cx, "previewColumnState", &[]), "loading");
            assert!(!read(&shell, cx, "previewFacts", &[]).contains("State=loading"));
        }
    }
}

/// Whether the column's shared Omi player is drawing, not just its slot being laid out.
pub(super) fn mark_shown(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    shell.read_with(cx, |shell, cx| {
        shell
            .selection_preview(0)
            .unwrap()
            .read(cx)
            .loading_mark
            .shown()
    })
}
