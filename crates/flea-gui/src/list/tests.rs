use super::bench::{allocations, fixture, frame, held, open, real_cx};
use super::pane::{visible_rows, ROW_HEIGHT};
use super::CACHE_SCREENS;
use std::rc::Rc;

fn list_of(
    cx: &mut gpui::TestAppContext,
    window: gpui::WindowHandle<super::bench::View>,
) -> Rc<std::cell::RefCell<super::ListState>> {
    window
        .update(cx, |view, _, _| Rc::clone(&view.list))
        .expect("the bench window is open")
}

#[test]
fn list_element_paints_one_fixed_row_per_visible_name() {
    for count in [0usize, 1, 7, 20] {
        let mut cx = real_cx();
        let rows = fixture("2k").into_iter().take(count).collect();
        let height = count.max(1) as f32 * ROW_HEIGHT;
        let window = open(&mut cx, held(rows), 900., height);
        frame(&mut cx, window, 0.);
        let list = list_of(&mut cx, window);
        let first = {
            let state = list.borrow();
            assert_eq!(state.visible.len(), count);
            for (slot, shown) in state.visible.iter().enumerate() {
                assert_eq!(shown.index, slot);
                assert!(shown.prepared.is_some());
            }
            state.visible.first().and_then(|row| row.prepared.clone())
        };
        frame(&mut cx, window, 0.);
        let state = list.borrow();
        assert_eq!(
            state.rows.misses, count as u64,
            "a visible row was shaped twice"
        );
        if let Some(first) = first {
            assert!(Rc::ptr_eq(
                &first,
                state.visible[0].prepared.as_ref().unwrap()
            ));
        }
    }
    // A scrolled viewport paints the partial rows at both edges and nothing past the end.
    let mut cx = real_cx();
    let window = open(
        &mut cx,
        held(fixture("2k").into_iter().take(20).collect()),
        900.,
        7. * ROW_HEIGHT,
    );
    frame(&mut cx, window, 15.);
    let indices: Vec<usize> = list_of(&mut cx, window)
        .borrow()
        .visible
        .iter()
        .map(|row| row.index)
        .collect();
    assert_eq!(indices, (0..8).collect::<Vec<_>>());
    frame(&mut cx, window, 13. * ROW_HEIGHT);
    let indices: Vec<usize> = list_of(&mut cx, window)
        .borrow()
        .visible
        .iter()
        .map(|row| row.index)
        .collect();
    assert_eq!(indices, (13..20).collect::<Vec<_>>());
}

struct Counts {
    still_allocations: u64,
    still_built: u64,
    step_allocations: u64,
    step_built: u64,
    back_built: u64,
    held_shaped: usize,
    peak: usize,
}

// One plain listing at this many rows: ten still frames, twenty one-row scroll steps, then five rows back.
fn count_frames(rows: usize) -> Counts {
    let mut cx = real_cx();
    let window = open(
        &mut cx,
        held(fixture("uniform")),
        900.,
        rows as f32 * ROW_HEIGHT,
    );
    let list = list_of(&mut cx, window);
    let built = || list.borrow().rows.misses;
    let mut y = 0.;
    for _ in 0..4 {
        frame(&mut cx, window, y);
    }
    let (heap, made) = (allocations(), built());
    for _ in 0..10 {
        frame(&mut cx, window, y);
    }
    let (still_allocations, still_built) = (allocations() - heap, built() - made);
    for _ in 0..4 {
        y += ROW_HEIGHT;
        frame(&mut cx, window, y);
    }
    let (heap, made) = (allocations(), built());
    for _ in 0..20 {
        y += ROW_HEIGHT;
        frame(&mut cx, window, y);
    }
    let (step_allocations, step_built) = (allocations() - heap, built() - made);
    let made = built();
    for _ in 0..5 {
        y -= ROW_HEIGHT;
        frame(&mut cx, window, y);
    }
    let state = list.borrow();
    Counts {
        still_allocations,
        still_built,
        step_allocations,
        step_built,
        back_built: state.rows.misses - made,
        held_shaped: state.rows.len(),
        peak: state.rows.peak,
    }
}

#[test]
fn a_still_frame_builds_and_allocates_nothing_per_row() {
    let (few, many) = (count_frames(10), count_frames(40));
    assert_eq!(
        (few.still_built, many.still_built),
        (0, 0),
        "a still frame shaped a row"
    );
    assert_eq!(
        few.still_allocations, many.still_allocations,
        "a still frame allocated per visible row"
    );
}

#[test]
fn a_scroll_step_shapes_only_the_rows_that_enter_and_allocates_per_row_entering() {
    let (few, many) = (count_frames(10), count_frames(40));
    for (rows, counts) in [(10, &few), (40, &many)] {
        assert_eq!(
            counts.step_built, 20,
            "twenty one-row steps shape twenty rows at {rows} rows"
        );
        assert_eq!(
            counts.back_built, 0,
            "scrolling back reshaped a cached row at {rows} rows"
        );
        let capacity = visible_rows(rows as f32 * ROW_HEIGHT) * CACHE_SCREENS;
        assert!(
            counts.peak <= capacity && counts.held_shaped <= capacity,
            "the row cache outgrew {capacity}"
        );
    }
    // A step's heap work follows the rows entering, not the rows on screen: at most one allocation a step apart.
    assert!(
        few.step_allocations.abs_diff(many.step_allocations) <= 20,
        "allocations per step grew with the viewport: {} at 10 rows, {} at 40",
        few.step_allocations,
        many.step_allocations
    );
}

fn glyph_ids(line: &gpui::LineLayout) -> Vec<u32> {
    line.runs
        .iter()
        .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.id.0))
        .collect()
}

// A window reply re-states its rows within the same listing: a changed row repaints its new facts, an equal one is reused.
#[test]
fn a_same_listing_window_refresh_reshapes_changed_rows_and_reuses_equal_ones_without_allocating() {
    let mut cx = real_cx();
    let window = open(&mut cx, held(fixture("uniform")), 900., 10. * ROW_HEIGHT);
    frame(&mut cx, window, 0.);
    let list = list_of(&mut cx, window);
    let before: Vec<_> = list
        .borrow()
        .visible
        .iter()
        .map(|row| row.prepared.clone().unwrap())
        .collect();
    let misses = list.borrow().rows.misses;
    let heap = allocations();
    frame(&mut cx, window, 0.);
    let still = allocations() - heap;

    let copy = held(fixture("uniform"));
    window
        .update(&mut cx, |view, _, _| view.held = copy)
        .unwrap();
    let heap = allocations();
    frame(&mut cx, window, 0.);
    assert_eq!(
        allocations() - heap,
        still,
        "an equal refresh allocated where a still frame does not"
    );
    {
        let state = list.borrow();
        assert_eq!(
            state.rows.misses, misses,
            "an equal refresh shaped a row again"
        );
        for (slot, shown) in state.visible.iter().enumerate() {
            assert!(
                Rc::ptr_eq(&before[slot], shown.prepared.as_ref().unwrap()),
                "row {slot} was not reused"
            );
        }
    }

    let mut rows = fixture("uniform");
    rows[0].size = 12_345_678;
    rows[1].mtime += 400 * 86_400;
    rows[2].mode = 0o100755;
    rows[3].icon = "image-x-generic".into();
    rows[4].link = Some("elsewhere.txt".into());
    let changed = held(rows);
    window
        .update(&mut cx, |view, _, _| view.held = changed)
        .unwrap();
    frame(&mut cx, window, 0.);
    let state = list.borrow();
    assert_eq!(
        state.rows.misses,
        misses + 5,
        "only the five changed rows are shaped again"
    );
    let now: Vec<_> = state
        .visible
        .iter()
        .map(|row| row.prepared.clone().unwrap())
        .collect();
    for slot in 0..5 {
        assert!(
            !Rc::ptr_eq(&before[slot], &now[slot]),
            "row {slot} kept the row shaped from older facts"
        );
    }
    for slot in 5..now.len() {
        assert!(
            Rc::ptr_eq(&before[slot], &now[slot]),
            "unchanged row {slot} was shaped again"
        );
    }
    assert_ne!(
        glyph_ids(before[0].size.line()),
        glyph_ids(now[0].size.line()),
        "the new size is not drawn"
    );
    assert_ne!(
        glyph_ids(before[1].date.line()),
        glyph_ids(now[1].date.line()),
        "the new date is not drawn"
    );
    assert_eq!(
        (before[2].ink, now[2].ink),
        (
            crate::row::NameInk::Foreground,
            crate::row::NameInk::Executable
        )
    );
    assert_eq!(
        (before[3].mark, now[3].mark),
        (
            crate::row::marks::Mark::FileText,
            crate::row::marks::Mark::Image
        )
    );
    assert!(
        now[4].name.line().len > before[4].name.line().len,
        "the link target is not drawn"
    );
}

// A row's glyph runs look their tiles up once: still frames and scroll steps repaint them from the tiles they kept.
#[test]
fn a_painted_row_looks_its_glyph_tiles_up_once_however_often_it_is_painted_or_moved() {
    let mut cx = real_cx();
    let window = open(&mut cx, held(fixture("uniform")), 900., 10. * ROW_HEIGHT);
    let list = list_of(&mut cx, window);
    let mut seen: Vec<Rc<crate::row::cache::Prepared>> = Vec::new();
    let mut paint = |cx: &mut gpui::TestAppContext, y: f32| {
        frame(cx, window, y);
        for shown in &list.borrow().visible {
            let prepared = shown.prepared.as_ref().expect("a visible row is prepared");
            if !seen.iter().any(|kept| Rc::ptr_eq(kept, prepared)) {
                seen.push(Rc::clone(prepared));
            }
        }
    };
    let mut y = 0.;
    for _ in 0..10 {
        paint(&mut cx, y);
    }
    for _ in 0..20 {
        y += ROW_HEIGHT;
        paint(&mut cx, y);
        paint(&mut cx, y);
    }
    assert_eq!(
        seen.len(),
        10 + 20,
        "ten rows on screen, then one entering per step"
    );
    for (slot, prepared) in seen.iter().enumerate() {
        let looked_up = [
            prepared.name.resolves(),
            prepared.size.resolves(),
            prepared.date.resolves(),
        ];
        assert_eq!(
            looked_up,
            [1, 1, 1],
            "row {slot} looked its glyph tiles up again after its first paint"
        );
    }
}
