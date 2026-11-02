//! Track Elements on a real text system: one Element per drawn column, still frames that shape and allocate nothing per row.
use super::check::Checks;
use super::fixture::{drain_asks, reply, row, Pane, Producers, SETTINGS};
use super::frames::{allocations, frame, held, open, read, real_cx, set};
use super::text_oracle::{fitted_name, says};
use flea_gui::columns::{
    geometry::{name_budget, Slot},
    paint::{Clip, ClipMarks, Focus, CACHE_SCREENS},
    Area, TrackRows,
};
use std::{cell::Cell, rc::Rc};

const DEEP: &str = "/srv/a/b/c/d";

fn names(prefix: &str, count: usize) -> Vec<(String, bool)> {
    (0..count)
        .map(|ix| (format!("{prefix}-{ix:04}"), ix % 3 == 0))
        .collect()
}

/// An area over DEEP whose every asked column has answered, with the cursor on a folder.
fn answered(width: f32, height: f32, rows: usize) -> (Area, Pane, Producers) {
    let mut area = Area::new(width, height, Some(5.), 0., SETTINGS);
    let mut producers = Producers::default();
    let listing: Vec<_> = names("item", rows)
        .iter()
        .map(|(name, dir)| row(name, *dir))
        .collect();
    let pane = Pane::at(DEEP, listing);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    for path in drain_asks(&mut area) {
        let leaf = path.rsplit('/').next().unwrap_or("").to_owned();
        let mut peek = names(&leaf, rows);
        for name in ["a", "b", "c", "d", "srv"] {
            peek.push((name.to_owned(), true));
        }
        let peek: Vec<(&str, bool)> = peek
            .iter()
            .map(|(name, dir)| (name.as_str(), *dir))
            .collect();
        pane.with(|view| area.peeked(&reply(&path, &peek), &view, &mut producers.ports(), 1));
    }
    (area, pane, producers)
}

#[test]
fn every_drawn_column_is_one_element_and_a_hidden_one_has_none() {
    let mut c = Checks::new("tests/g1_07/elements");
    for (line, width, count) in [(1, 800., 2), (2, 1200., 3), (3, 1800., 4), (4, 2400., 5)] {
        let (mut area, pane, mut producers) = answered(width, 620., 40);
        c.eq(
            line,
            "the area draws the count its width holds",
            area.count(),
            count,
        );
        let mut cx = real_cx();
        let window = open(
            &mut cx,
            Rc::clone(area.tracks()),
            held(pane.rows.clone()),
            width,
            620.,
        );
        frame(&mut cx, window);
        let (built, constructed) = read(&mut cx, window, |view| {
            (view.painters.built(), view.painters.constructed)
        });
        c.eq(
            10 + line,
            "one Element paint state per drawn column",
            built,
            count,
        );
        c.eq(
            20 + line,
            "each drawn column constructed once",
            constructed,
            count,
        );
        area.resize(800., 620.);
        pane.with(|view| area.sync(&view, &mut producers.ports(), 2));
        let tracks = Rc::clone(area.tracks());
        set(&mut cx, window, |view| view.tracks = tracks);
        frame(&mut cx, window);
        let (built, dropped) = read(&mut cx, window, |view| {
            (view.painters.built(), view.painters.dropped)
        });
        c.eq(
            30 + line,
            "narrowing to two drops the hidden columns' Elements",
            built,
            2,
        );
        c.eq(
            40 + line,
            "every column no longer drawn is dropped",
            dropped,
            count - 2,
        );
    }
    c.finish();
}

struct Still {
    warm: u64,
    misses: u64,
    allocations: u64,
    peak: usize,
    capacity: usize,
}

// Four warm frames, then ten still frames and ten cursor moves inside the viewport over the same snapshot.
fn still_frames(rows: usize, cursor_moves: bool) -> Still {
    let height = rows as f32 * 31.;
    let (area, pane, _) = answered(1200., height, 200);
    let mut cx = real_cx();
    let window = open(
        &mut cx,
        Rc::clone(area.tracks()),
        held(pane.rows.clone()),
        1200.,
        height,
    );
    let heap = allocations();
    for _ in 0..4 {
        frame(&mut cx, window);
    }
    let warm = allocations() - heap;
    let misses = |cx: &mut gpui::TestAppContext| {
        read(cx, window, |view| {
            [Slot::Parent, Slot::Active, Slot::Child]
                .iter()
                .filter_map(|slot| view.painters.get(*slot))
                .map(|paint| paint.borrow().cache.misses)
                .sum::<u64>()
        })
    };
    let before = misses(&mut cx);
    let heap = allocations();
    for step in 0..10 {
        if cursor_moves {
            let focus = Rc::new(Focus {
                cursor: Some(step % rows.max(1)),
                ..Focus::default()
            });
            set(&mut cx, window, |view| view.focus = focus);
        }
        frame(&mut cx, window);
    }
    let spent = allocations() - heap;
    let after = misses(&mut cx);
    let (peak, capacity) = read(&mut cx, window, |view| {
        let paint = view
            .painters
            .get(Slot::Active)
            .expect("the active track is drawn")
            .borrow();
        (paint.cache.peak, paint.cache.capacity())
    });
    Still {
        warm,
        misses: after - before,
        allocations: spent,
        peak,
        capacity,
    }
}

#[test]
fn an_unchanged_snapshot_shapes_nothing_and_allocates_nothing_per_row() {
    let mut c = Checks::new("tests/g1_07/elements");
    let (few, many) = (still_frames(10, false), still_frames(40, false));
    c.ok(
        49,
        "the counter sees the warm frames' shaping, so a zero below is measured",
        few.warm > 0 && many.warm > few.warm,
    );
    c.eq(50, "a still frame shapes no row at 10 rows", few.misses, 0);
    c.eq(51, "a still frame shapes no row at 40 rows", many.misses, 0);
    c.eq(
        52,
        "still frames allocate the same at 10 and 40 visible rows",
        few.allocations,
        many.allocations,
    );
    let (few, many) = (still_frames(10, true), still_frames(40, true));
    c.eq(
        53,
        "a cursor move inside the viewport reshapes nothing",
        few.misses + many.misses,
        0,
    );
    c.ok(
        54,
        "the cursor's ink is chosen at paint, so its heap work does not follow the rows",
        few.allocations.abs_diff(many.allocations) <= 10,
    );
    c.ok(
        55,
        "each track's prepared rows stay within its bound",
        many.peak <= many.capacity && many.capacity == 40 * CACHE_SCREENS,
    );
    c.finish();
}

struct Marks {
    asked: Cell<usize>,
    empty: bool,
}

impl ClipMarks for Marks {
    fn is_empty(&self) -> bool {
        self.empty
    }
    fn mark(&self, _: &str, name: &str) -> Clip {
        self.asked.set(self.asked.get() + 1);
        if name.ends_with('1') {
            Clip::Cut
        } else {
            Clip::None
        }
    }
}

#[test]
fn an_empty_clipboard_is_asked_once_a_frame_and_a_cut_row_keeps_its_clip_cells() {
    let mut c = Checks::new("tests/g1_07/elements");
    let (area, pane, _) = answered(1200., 310., 40);
    let mut cx = real_cx();
    let window = open(
        &mut cx,
        Rc::clone(area.tracks()),
        held(pane.rows.clone()),
        1200.,
        310.,
    );
    let empty = Rc::new(Marks {
        asked: Cell::new(0),
        empty: true,
    });
    let clip: Rc<dyn ClipMarks> = empty.clone();
    set(&mut cx, window, |view| view.clip = Some(clip));
    frame(&mut cx, window);
    c.eq(
        60,
        "an empty clipboard is never asked per row",
        empty.asked.get(),
        0,
    );
    let marks = Rc::new(Marks {
        asked: Cell::new(0),
        empty: false,
    });
    let clip: Rc<dyn ClipMarks> = marks.clone();
    set(&mut cx, window, |view| view.clip = Some(clip));
    frame(&mut cx, window);
    c.ok(
        61,
        "a held clipboard is asked for the visible rows only",
        marks.asked.get() > 0 && marks.asked.get() <= 3 * 10,
    );
    let (cut, plain) = read(&mut cx, window, |view| {
        let paint = view
            .painters
            .get(Slot::Active)
            .expect("the active track is drawn")
            .borrow();
        (paint.clip(1), paint.clip(2))
    });
    c.eq(62, "a cut row carries its scissors", cut, Clip::Cut);
    c.eq(
        63,
        "a row off the clipboard carries no mark",
        plain,
        Clip::None,
    );
    c.finish();
}

#[test]
fn a_folder_row_shapes_its_chevron_budget_only_when_it_differs() {
    let mut c = Checks::new("tests/g1_07/elements");
    let long = "a-folder-name-long-enough-to-lose-cells-to-the-chevron-0123456789";
    let mut area = Area::new(1200., 310., None, 0., SETTINGS);
    let mut producers = Producers::default();
    let pane = Pane::at("/srv/work", vec![row(long, true), row("short", true)]);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    let _ = drain_asks(&mut area);
    let mut cx = real_cx();
    let width = area.layout.column_width;
    let window = open(
        &mut cx,
        Rc::clone(area.tracks()),
        held(pane.rows.clone()),
        1200.,
        310.,
    );
    frame(&mut cx, window);
    let budgets = window
        .update(&mut cx, |view, window, _| {
            let paint = view
                .painters
                .get(Slot::Active)
                .cloned()
                .expect("the active track");
            let advance = f32::from(paint.borrow_mut().body_advance(window));
            let paint = paint.borrow();
            let long_row = paint
                .prepared(0)
                .cloned()
                .expect("the long row is prepared");
            let short_row = paint
                .prepared(1)
                .cloned()
                .expect("the short row is prepared");
            let plain = name_budget(width, true, false, advance);
            let chevron = name_budget(width, true, true, advance);
            let plain_expected = fitted_name(window, long, plain, advance);
            let chevron_expected = fitted_name(window, long, chevron, advance);
            (
                plain,
                chevron,
                says(window, &long_row.plain, &plain_expected),
                long_row
                    .chevron
                    .as_ref()
                    .map(|run| says(window, run, &chevron_expected)),
                short_row.chevron.is_none(),
            )
        })
        .expect("the frame window is open");
    let (plain, chevron, plain_len, chevron_len, short_shares) = budgets;
    c.ok(
        70,
        "the chevron budget is smaller than the plain one",
        chevron < plain,
    );
    // Compare the exact glyphs with an exhaustive width oracle, including proportional fallback faces.
    c.eq(71, "the plain name fills its whole budget", plain_len, true);
    c.eq(
        72,
        "the chevron variant fills the smaller budget",
        chevron_len,
        Some(true),
    );
    c.ok(
        73,
        "a short folder name shapes one line for both",
        short_shares,
    );
    let active = area
        .tracks()
        .iter()
        .find(|view| view.slot == Slot::Active)
        .map(|view| view.rows.clone());
    c.eq(
        74,
        "the active track draws the pane's own listing",
        active,
        Some(TrackRows::Listing),
    );
    c.finish();
}
