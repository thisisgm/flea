use super::*;

#[test]
fn gm_prevsel_a_rendered_table_copies_tab_separated_rows() {
    use flea::ui_logic_doc_preview::markdown::markdown_model::Table;
    let table = Table {
        head: vec!["Kind".into(), "Value".into()],
        rows: vec![
            vec!["alpha".into(), "one".into()],
            vec!["beta".into(), "two".into()],
        ],
        cols: 2,
        measure: vec!["alpha".into(), "Value".into()],
        weights: vec![5, 5],
        words: vec![1, 1],
        ..Table::default()
    };
    let (cx, handle, view) = mount(
        false,
        false,
        "Before\n\n| Kind | Value |\n| --- | --- |\n| alpha | one |\n| beta | two |\n",
        vec![Block::run("Before"), Block::Table(table)],
    );
    let (start, end) = view.read_with(&cx, |view, _| {
        let first = view.selection.spans.first().expect("first selectable run");
        let last = view.selection.spans.last().expect("last selectable run");
        (
            point(first.bounds.left(), first.bounds.center().y),
            point(last.bounds.right(), last.bounds.center().y),
        )
    });
    let mut visual = VisualTestContext::from_window(handle.into(), &cx);
    visual.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    visual.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    visual.simulate_keystrokes("ctrl-c");
    assert_eq!(
        visual
            .update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
            .as_deref(),
        Some("Before\nKind\tValue\nalpha\tone\nbeta\ttwo"),
        "cells joined by a tab, one line per row"
    );
}

#[test]
fn gm_prevsel_an_inline_code_wash_bridges_the_chip_padding() {
    let (cx, handle, view) = mount(
        false,
        false,
        "plain `chip` tail\n",
        vec![Block::run("plain <code>chip</code> tail")],
    );
    let (start, end, pieces) = view.read_with(&cx, |view, _| {
        let spans = &view.selection.spans;
        let pieces: Vec<_> = spans.iter().map(|s| (s.bounds, s.reach)).collect();
        (
            point(spans[0].bounds.left(), spans[0].bounds.center().y),
            point(
                spans.last().unwrap().bounds.right(),
                spans[0].bounds.center().y,
            ),
            pieces,
        )
    });
    assert_eq!(pieces.len(), 3, "plain, chip and tail are three runs");
    assert!(
        pieces[1].0.left() > pieces[0].0.right(),
        "the chip's padding leaves a gap between the runs"
    );
    for pair in pieces.windows(2) {
        assert_eq!(
            pair[0].1,
            pair[1].0.left(),
            "each run's wash reaches the next run"
        );
    }
    let mut visual = VisualTestContext::from_window(handle.into(), &cx);
    visual.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    visual.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    view.read_with(&visual, |view, _| {
        let s = &view.selection;
        let spans = &s.spans;
        assert_eq!(
            s.wash_end(&spans[0], 0..spans[0].len),
            Some(spans[1].bounds.left())
        );
        assert_eq!(
            s.wash_end(&spans[1], 0..spans[1].len),
            Some(spans[2].bounds.left())
        );
        assert_eq!(
            s.wash_end(&spans[2], 0..spans[2].len),
            None,
            "the range's last run ends at its own glyphs"
        );
    });
}
