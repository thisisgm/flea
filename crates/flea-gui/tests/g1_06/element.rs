use super::harness::{allocations, english, held, metrics, open, real_cx, row, uniform, Pane};
use flea::jsondoc::parse;
use flea_gui::list::{columns::Column, Metrics};
use flea_gui::navigation::sort::Size;
use flea_gui::row::{
    format::Words,
    marks::{ClipMark, Mark},
};
use gpui::px;
use std::hint::black_box;

const CACHE_SCREENS: usize = 3;

struct Counts {
    still_allocations: u64,
    still_shaped: u64,
    step_shaped: u64,
    back_shaped: u64,
    held_shaped: usize,
    peak: usize,
}

// One listing at this many rows: ten still frames, twenty one-row steps, then five rows back.
fn count_frames(rows: usize) -> Counts {
    let mut cx = real_cx();
    let pane = open(&mut cx, held(uniform(2_000), 1), 900., rows as f32);
    let shaped = |pane: &Pane| pane.list.borrow().rows.misses;
    let row = metrics().row;
    let mut y = 0.;
    for _ in 0..4 {
        pane.frame(&mut cx, y);
    }
    let (heap, made) = (allocations(), shaped(&pane));
    for _ in 0..10 {
        pane.frame(&mut cx, y);
    }
    let (still_allocations, still_shaped) = (allocations() - heap, shaped(&pane) - made);
    let made = shaped(&pane);
    for _ in 0..20 {
        y += row;
        pane.frame(&mut cx, y);
    }
    let step_shaped = shaped(&pane) - made;
    let made = shaped(&pane);
    for _ in 0..5 {
        y -= row;
        pane.frame(&mut cx, y);
    }
    let state = pane.list.borrow();
    Counts {
        still_allocations,
        still_shaped,
        step_shaped,
        back_shaped: state.rows.misses - made,
        held_shaped: state.rows.len(),
        peak: state.rows.peak,
    }
}

#[test]
fn still_frames_shape_nothing_and_allocate_alike_at_ten_and_forty_rows() {
    let before = allocations();
    black_box(Box::new(7u8));
    assert!(
        allocations() > before,
        "the allocation count is not counting"
    );
    let (few, many) = (count_frames(10), count_frames(40));
    assert_eq!(
        (few.still_shaped, many.still_shaped),
        (0, 0),
        "a still frame shaped a row"
    );
    assert_eq!(
        few.still_allocations, many.still_allocations,
        "a still frame allocated per visible row"
    );
}

#[test]
fn twenty_one_row_steps_shape_twenty_and_the_way_back_shapes_none_inside_three_screens() {
    for rows in [10usize, 40] {
        let counts = count_frames(rows);
        assert_eq!(
            counts.step_shaped, 20,
            "twenty one-row steps at {rows} rows"
        );
        assert_eq!(
            counts.back_shaped, 0,
            "scrolling back reshaped a cached row at {rows} rows"
        );
        let capacity = rows * CACHE_SCREENS;
        assert!(
            counts.peak <= capacity && counts.held_shaped <= capacity,
            "the row cache outgrew {capacity} at {rows} rows: {}",
            counts.peak
        );
    }
}

#[test]
fn a_fractional_scroll_clips_the_partial_row_to_the_list_under_the_header() {
    let mut cx = real_cx();
    let pane = open(&mut cx, held(uniform(40), 1), 900., 7.);
    let m = metrics();
    pane.frame(&mut cx, 15.);
    let state = pane.list.borrow();
    let header = state.header.drawn.bounds;
    let list = state.drawn.bounds;
    assert_eq!(f32::from(header.size.height), m.header);
    assert_eq!(
        header.bottom(),
        list.top(),
        "the list does not start where the header ends"
    );
    assert_eq!(
        state.drawn.mask, list,
        "rows paint outside the list's own box"
    );
    let shown: Vec<usize> = state.visible().iter().map(|shown| shown.index).collect();
    assert_eq!(
        shown,
        (0..8).collect::<Vec<_>>(),
        "both partial rows are drawn"
    );
    let first_top = f32::from(list.top()) - 15.;
    assert!(
        first_top < f32::from(header.bottom()),
        "the first row is not partly under the header"
    );
}

#[test]
fn text_20_takes_the_board_geometry_from_the_v0312_formulas() {
    // list-textsize.html at zoom 20/14: row 29.4, header 25.9 and slot 22.4 resolve to 42, 37 and 32; fix2.rs pins the mark.
    let at = Metrics::at(px(20.));
    assert_eq!((at.row, at.header, at.slot), (42., 37., 32.));
    assert_eq!(
        (at.pad, at.gap, at.lane, at.edge, at.header_top),
        (20., 13., 20., 3., 6.)
    );
    assert_eq!(
        (at.clip, at.handle, at.hairline),
        (12., 9., 1.),
        "the clip mark, the grab zone and hairlines stay fixed"
    );
    let base = Metrics::at(px(14.));
    assert_eq!(
        (
            base.row,
            base.header,
            base.header_top,
            base.pad,
            base.slot,
            base.gap,
            base.edge,
            base.lane
        ),
        (31., 27., 4., 14., 23., 9., 2., 14.)
    );
    // Text 12, Omarchy's stock: bodySmall 11, line box 20, padding 3 each side, chrome 23.
    let stock = Metrics::at(px(12.));
    assert_eq!((stock.row, stock.header, stock.slot), (26., 23., 20.));
}

#[test]
fn a_hidden_list_parks_every_shaped_row_and_title() {
    let mut cx = real_cx();
    let pane = open(&mut cx, held(uniform(100), 1), 900., 10.);
    pane.frame(&mut cx, 0.);
    assert!(!pane.list.borrow().rows.is_empty());
    let titles = pane.list.borrow().header.drawn.shaped;
    pane.list.borrow_mut().park();
    {
        let state = pane.list.borrow();
        assert_eq!(
            (state.rows.len(), state.visible().len()),
            (0, 0),
            "a parked list still holds rows"
        );
    }
    pane.frame(&mut cx, 0.);
    assert_eq!(
        pane.list.borrow().header.drawn.shaped,
        titles + 1,
        "a parked header kept its titles"
    );
}

#[test]
fn a_language_change_reshapes_every_row_and_title_once() {
    let mut cx = real_cx();
    let pane = open(&mut cx, held(uniform(100), 1), 900., 10.);
    pane.frame(&mut cx, 0.);
    let (misses, titles) = {
        let state = pane.list.borrow();
        (state.rows.misses, state.header.drawn.shaped)
    };
    let english = Words::from_catalog(&english());
    let german = Words {
        name: "Name".into(),
        size: "Gr\u{f6}\u{df}e".into(),
        modified: "Ge\u{e4}ndert".into(),
        ..english
    };
    pane.list.borrow_mut().set_words(german);
    assert!(
        pane.list.borrow().rows.is_empty(),
        "rows shaped in the old language survived"
    );
    pane.frame(&mut cx, 0.);
    pane.frame(&mut cx, 0.);
    let state = pane.list.borrow();
    // The old cache went whole, so the fresh one counts from zero: ten visible rows, each shaped once over two frames.
    assert_eq!(
        (misses, state.rows.misses),
        (10, 10),
        "each visible row reshapes exactly once"
    );
    assert_eq!(
        state.header.drawn.shaped,
        titles + 1,
        "the titles reshape exactly once"
    );
    assert_eq!(
        flea_gui::list::header::titles(&state.view),
        "Name|Mode|Gr\u{f6}\u{df}e|Ge\u{e4}ndert|Kind"
    );
}

#[test]
fn cut_and_copy_marks_follow_the_clipboard_reshape_only_their_rows_and_vanish_with_it() {
    let mut cx = real_cx();
    let pane = open(&mut cx, held(uniform(20), 1), 900., 10.);
    pane.frame(&mut cx, 0.);
    let misses = pane.list.borrow().rows.misses;
    {
        let mut state = pane.list.borrow_mut();
        state.view.clip.scope("/home/user/work");
        state.view.clip.set(
            &[
                "/home/user/work/file-000002.txt".into(),
                "/home/user/other/file-000003.txt".into(),
            ],
            true,
        );
    }
    pane.frame(&mut cx, 0.);
    {
        let state = pane.list.borrow();
        let clip = |index: usize| {
            state
                .visible()
                .iter()
                .find(|shown| shown.index == index)
                .unwrap()
                .clip
        };
        assert_eq!(
            (clip(2), clip(3)),
            (Some(ClipMark::Cut), None),
            "a path in another folder marked a row here"
        );
        assert_eq!(
            state.rows.misses,
            misses + 1,
            "only the marked row reshaped for its narrower name"
        );
    }
    pane.list
        .borrow_mut()
        .view
        .clip
        .set(&["/home/user/work/file-000002.txt".into()], false);
    pane.frame(&mut cx, 0.);
    assert_eq!(pane.list.borrow().visible()[2].clip, Some(ClipMark::Copy));
    // An emptied clipboard frees its set and answers every row without a lookup.
    pane.list.borrow_mut().view.clip.set(&[], false);
    let lookups = pane.list.borrow().view.clip.lookups.get();
    pane.frame(&mut cx, 0.);
    let state = pane.list.borrow();
    assert_eq!(
        state.view.clip.lookups.get(),
        lookups,
        "an empty clipboard cost a lookup per row"
    );
    assert_eq!(state.view.clip.held(), 0, "an empty clipboard kept its set");
    assert!(
        state.visible().iter().all(|shown| shown.clip.is_none()),
        "a mark outlived the clipboard"
    );
}

#[test]
fn language_marks_follow_the_setting_and_a_folder_walk_reshapes_only_its_row() {
    let mut cx = real_cx();
    let rows = vec![
        row("main.rs", "text-x-script", 26, false),
        row("src", "folder", 4096, true),
        row("notes.txt", "text-plain", 9, false),
    ];
    let pane = open(&mut cx, held(rows, 1), 900., 10.);
    pane.frame(&mut cx, 0.);
    let mark = |pane: &Pane, index: usize| {
        pane.list.borrow().visible()[index]
            .prepared
            .as_ref()
            .unwrap()
            .mark
    };
    assert!(matches!(mark(&pane, 0), Mark::Language(language) if language.slug() == "rust"));
    assert_eq!(
        (mark(&pane, 1), mark(&pane, 2)),
        (Mark::Folder, Mark::FileText)
    );
    pane.list
        .borrow_mut()
        .view
        .apply_settings(&parse(r#"{"langMarks":false}"#).unwrap(), 0);
    pane.frame(&mut cx, 0.);
    assert_eq!(
        mark(&pane, 0),
        Mark::Code,
        "a language mark stayed with the setting off"
    );
    let misses = pane.list.borrow().rows.misses;
    pane.list.borrow_mut().view.folder_size(
        1,
        1,
        Size {
            bytes: 18_000,
            partial: true,
        },
    );
    pane.frame(&mut cx, 0.);
    let state = pane.list.borrow();
    assert_eq!(
        state.rows.misses,
        misses + 1,
        "a walked size reshaped more than its folder's row"
    );
    assert!(state.drawn.layout.shown(Column::Size));
}
