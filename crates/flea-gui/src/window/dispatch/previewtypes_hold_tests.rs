//! An unloaded manual preview draws the empty frame until the operator requests the selection.
use super::previewtypes_tests::{
    answer, file, local, meta, open_at, open_with_preview, pump, thumbed,
};
use crate::{state::Shell, window::u07_tests::read};
use gpui::{px, Entity, TestAppContext, VisualTestContext};

fn manual(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    open_with_preview(cx, "/files", Some(14.), "manual")
}

fn assert_empty(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    assert_eq!(read(shell, cx, "columnManualHold", &[]), "true");
    assert_eq!(
        read(shell, cx, "previewFacts", &[]),
        "",
        "an unloaded selection has no facts"
    );
    assert_eq!(read(shell, cx, "previewColumnState", &[]), "unsupported");
    assert!(cx.debug_bounds("column-preview-name").is_none());
    assert!(cx.debug_bounds("column-preview-facts").is_none());
    assert!(
        cx.debug_bounds("column-preview-manual-empty").is_some(),
        "the empty state contains the document mark and no-preview sentence"
    );
    assert_eq!(
        read(shell, cx, "columnStripCentre", &[]),
        "",
        "an unloaded clip mounts no transport"
    );
    let frame = cx
        .debug_bounds("column-preview-frame")
        .expect("the empty frame");
    let width = f32::from(frame.size.width);
    assert_eq!(
        frame.size.height,
        px((width * 10. / 16.).round()),
        "even an unloaded document keeps the empty frame's aspect ratio"
    );
    let (quads, scale, ink) = cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let roles = crate::list::Palette::inherited(&window.text_style(), theme);
        (
            window.painted_quads(),
            window.scale_factor(),
            roles.muted_frame,
        )
    });
    assert!(
        quads
            .iter()
            .any(|q| q.bounds == frame.scale(scale) && q.border_color == ink),
        "the empty frame paints the muted frame role"
    );
}

#[gpui::test]
fn manual_preview_walk_keeps_the_empty_frame_without_name_facts_or_transport(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = manual(cx);
    let rows = vec![
        file("photo.png", "image-png", "PNG image", 0o100644),
        file("notes.txt", "text-plain", "Plain text", 0o100644),
        file("clip.webm", "video-webm", "WebM video", 0o100644),
    ];
    super::test_shell::land(&shell, cx, "/files", rows, 1);
    let mut lines = local(&shell, cx, "/files");
    for index in 0..3 {
        assert_empty(&shell, cx);
        if index < 2 {
            cx.simulate_keystrokes("down");
            lines.extend(pump(cx));
        }
    }
    for command in ["meta", "code"] {
        let tag = format!(r#""c":"{command}""#);
        assert!(
            !lines.iter().any(|line| line.contains(&tag)),
            "manual browsing asks no {command}: {lines:?}"
        );
    }
    for line in lines {
        let request = flea::jsondoc::parse(&line).expect("a backend request");
        if request.get("c").and_then(flea::jsondoc::Json::as_str) == Some("thumb") {
            assert!(
                request.get("cacheOnly").is_some(),
                "only viewport thumbnails may load: {line}"
            );
        }
    }
}

#[gpui::test]
fn manual_preview_load_restores_the_selected_file_and_next_move_returns_to_empty(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = manual(cx);
    let rows = vec![
        file("photo.png", "image-png", "PNG image", 0o100644),
        file("next.png", "image-png", "PNG image", 0o100644),
    ];
    super::test_shell::land(&shell, cx, "/files", rows, 1);
    local(&shell, cx, "/files");
    assert_empty(&shell, cx);
    cx.simulate_keystrokes("ctrl-space");
    let lines = pump(cx);
    meta(&shell, cx, &lines, r#","w":800,"h":600"#);
    thumbed(&shell, cx, &lines, 0, "");
    pump(cx);
    assert_eq!(read(&shell, cx, "columnManualHold", &[]), "false");
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "image");
    assert!(read(&shell, cx, "previewFacts", &[]).starts_with("Kind=PNG image|"));
    assert!(cx.debug_bounds("column-preview-name").is_some());
    assert!(cx.debug_bounds("column-preview-facts").is_some());
    assert!(cx.debug_bounds("column-preview-manual-empty").is_none());
    cx.simulate_keystrokes("down");
    pump(cx);
    assert_empty(&shell, cx);
}

#[gpui::test]
fn automatic_preview_with_a_storage_policy_hold_keeps_the_listing_facts(cx: &mut TestAppContext) {
    let (shell, cx) = open_at(cx, "/files", Some(14.));
    let rows = vec![file("photo.png", "image-png", "PNG image", 0o100644)];
    super::test_shell::land(&shell, cx, "/files", rows, 1);
    answer(
        &shell,
        cx,
        r#"{"t":"fsinfo","path":"/files","fs":"nfs","dev":1,"total":100,"free":90,"class":"network"}"#,
    );
    pump(cx);
    assert_eq!(read(&shell, cx, "columnManualHold", &[]), "true");
    assert!(read(&shell, cx, "previewFacts", &[]).starts_with("Kind=PNG image|"));
    assert!(cx.debug_bounds("column-preview-name").is_some());
    assert!(cx.debug_bounds("column-preview-facts").is_some());
    assert!(cx.debug_bounds("column-preview-manual-empty").is_none());
}
