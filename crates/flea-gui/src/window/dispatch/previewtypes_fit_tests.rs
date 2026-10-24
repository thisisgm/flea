//! A tall preview keeps its name and whole facts inside the column at every window height (P11).
use super::previewtypes_tests::{answer, ask, field, file, local, meta, open_at, pump};
use crate::window::u07_tests::read;
use gpui::{px, size, Bounds, Pixels, TestAppContext, VisualTestContext};

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
}

/// The bottom edge of an IPC rect "x y w h".
fn bottom(rect: &str) -> f32 {
    let n: Vec<f32> = rect
        .split(' ')
        .map(|n| n.parse().unwrap_or_else(|_| panic!("a rect, not {rect:?}")))
        .collect();
    n[1] + n[3]
}

fn assert_fits(shell: &gpui::Entity<crate::state::Shell>, cx: &mut VisualTestContext, at: &str) {
    let column = bounds(cx, "column-preview");
    let name = bounds(cx, "column-preview-name");
    let facts = bounds(cx, "column-preview-facts");
    let frame = bottom(&read(shell, cx, "columnFrameRect", &[]));
    let (column_bottom, name_top) = (f32::from(column.bottom()), f32::from(name.top()));
    let (name_bottom, facts_top) = (f32::from(name.bottom()), f32::from(facts.top()));
    let facts_bottom = f32::from(facts.bottom());
    assert!(frame <= name_top + 0.5, "{at}: the frame ends above the name: {frame} > {name_top}");
    assert!(name_bottom <= facts_top + 0.5, "{at}: the facts sit under the name: {name_bottom} > {facts_top}");
    let pad = shell.read_with(cx, |shell, cx| {
        let face = shell.selection_preview(0).expect("the column is mounted").read(cx).face.clone();
        crate::list::Metrics::for_typeface(&face).pad
    });
    assert!(
        facts_bottom <= column_bottom - pad + 0.5,
        "{at}: the whole facts line keeps the column's bottom margin {pad}: {facts_bottom} vs {column_bottom}"
    );
}

#[gpui::test]
fn a_code_preview_keeps_its_name_and_facts_inside_the_column_at_every_height(cx: &mut TestAppContext) {
    let (shell, cx) = open_at(cx, "/files", Some(14.));
    let rows = vec![file("code-rs.rs", "text-x-rust", "Rust source code", 0o100644)];
    super::test_shell::land(&shell, cx, "/files", rows, 1);
    let lines = local(&shell, cx, "/files");
    meta(&shell, cx, &lines, r#","lines":3"#);
    let code = ask(&lines, "code").to_owned();
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column","state":"styled","first":1,"total":3,"lines":[[5,"fn",0," a()"],[0,"}}"],[]]}}"#,
            field(&code, "token")
        ),
    );
    pump(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "code");
    // Compact facts first, then the full table that i turns on.
    for facts in ["compact", "full"] {
        for (w, h) in [(1500., 1013.), (2400., 1300.), (1100., 480.), (900., 360.)] {
            cx.simulate_resize(size(px(w), px(h)));
            pump(cx);
            assert_fits(&shell, cx, &format!("{facts} {w}x{h}"));
        }
        cx.simulate_keystrokes("i->i");
        pump(cx);
    }
}
