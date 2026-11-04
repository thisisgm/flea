//! Element frames for the probes that read a drawn row: its name, its budget and a narrowed listing.
use super::check::Checks;
use super::fixture::{peek_row, row};
use super::frames::{frame, held, open, read, real_cx, set, View};
pub use super::text_oracle::{fitted_name, says};
pub use super::theme::test_theme;
use flea::protocol::RowFacts;
use flea_gui::columns::{
    geometry::{cells, name_budget, Slot, GAP},
    paint::{Clip, ClipMarks, Focus, Palette, TrackPaint, CLIP},
    peeks::PeekRow,
    TrackRows, TrackState, TrackView,
};
use gpui::{TestAppContext, Window, WindowHandle};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub const LONG: &str = concat!(
    "a-very-long-filename-that-must-elide-in-the-middle-",
    "to-keep-its-extension-visible-",
    "0123456789abcdef0123456789abcdef0123456789abcdef.png"
);
pub const SHORT: &str = "columnrow-geom.txt";

pub fn track(slot: Slot, x: f32, width: f32, rows: TrackRows, lifted: Option<&str>) -> TrackView {
    TrackView {
        slot,
        x,
        width,
        path: "/probe".into(),
        rows,
        dim: slot != Slot::Active && slot != Slot::Child,
        lifted: lifted.map(str::to_owned),
        lifted_at: None,
        divider: false,
        state: TrackState::Rows,
        enabled: true,
    }
}

pub fn active(width: f32) -> TrackView {
    track(Slot::Active, 0., width, TrackRows::Listing, None)
}

pub fn peeks(rows: &[(&str, bool)]) -> TrackRows {
    TrackRows::Peek(
        rows.iter()
            .map(|(name, dir)| peek_row(name, *dir))
            .collect::<Rc<[PeekRow]>>(),
    )
}

pub fn files(names: impl IntoIterator<Item = String>) -> Vec<RowFacts> {
    names.into_iter().map(|name| row(&name, false)).collect()
}

/// Inside one window update: the slot's paint state, the face's advance, and the window for shaping.
pub fn look<R>(
    cx: &mut TestAppContext,
    window: WindowHandle<View>,
    slot: Slot,
    see: impl FnOnce(&TrackPaint, f32, &Window) -> R,
) -> R {
    window
        .update(cx, |view, window, _| {
            let paint = view.painters.get(slot).cloned().expect("the slot is drawn");
            let advance = f32::from(paint.borrow_mut().body_advance(window));
            let paint = paint.borrow();
            see(&paint, advance, window)
        })
        .expect("the frame window is open")
}

pub fn misses(cx: &mut TestAppContext, window: WindowHandle<View>, slot: Slot) -> u64 {
    read(cx, window, |view| {
        view.painters
            .get(slot)
            .map_or(0, |paint| paint.borrow().cache.misses)
    })
}

pub fn visible(cx: &mut TestAppContext, window: WindowHandle<View>, slot: Slot) -> Vec<usize> {
    read(cx, window, |view| {
        view.painters
            .get(slot)
            .map_or(Vec::new(), |paint| paint.borrow().visible().collect())
    })
}

/// A theme distinct from Theme::default, so a palette that ignores the installed theme cannot match it.
/// Installs test_theme before the window opens, the way the app installs its theme at startup.
pub fn install_theme(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(test_theme()));
}

pub fn palette(cx: &mut TestAppContext, window: WindowHandle<View>) -> Palette {
    // The expectation comes from the installed theme, never from the element's own fallback expression.
    let theme = test_theme();
    window
        .update(cx, |_, window, _| {
            Palette::inherited(&window.text_style(), theme)
        })
        .expect("the frame window is open")
}

/// Every index in the range paints the name this gives it, at the track's plain budget.
pub fn paints(
    cx: &mut TestAppContext,
    window: WindowHandle<View>,
    width: f32,
    range: std::ops::Range<usize>,
    name: impl Fn(usize) -> String,
) -> bool {
    look(cx, window, Slot::Active, |paint, advance, window| {
        let budget = name_budget(width, true, false, advance);
        range.clone().all(|index| {
            paint.prepared(index).is_some_and(|row| {
                says(
                    window,
                    row.name(false),
                    &fitted_name(window, &name(index), budget, advance),
                )
            })
        }) && paint.visible().count() == range.len()
    })
}

#[test]
fn a_track_paints_with_the_installed_theme() {
    let mut cx = real_cx();
    install_theme(&mut cx);
    let tracks: Rc<[TrackView]> = Rc::from([active(400.)]);
    let rows = files((0..3).map(|i| format!("row-{i}.txt")));
    let window = open(&mut cx, tracks, held(rows), 400., 600.);
    frame(&mut cx, window);
    let painted = read(&mut cx, window, |view| {
        view.painters
            .get(Slot::Active)
            .and_then(|paint| paint.borrow().painted)
    });
    assert_eq!(
        painted,
        Some(palette(&mut cx, window)),
        "the track painted with a theme other than the installed one"
    );
}

#[test]
fn a_narrowed_listing_paints_its_own_rows_in_the_same_track() {
    let mut c = Checks::new("tests/columnnarrow.qml");
    let full = files((0..40).map(|i| format!("row-{i:02}.txt")));
    let narrowed: Vec<RowFacts> = full.iter().step_by(2).cloned().collect();
    let tracks: Rc<[TrackView]> = Rc::from([active(400.)]);
    let mut cx = real_cx();
    install_theme(&mut cx);
    let window = open(&mut cx, Rc::clone(&tracks), held(full.clone()), 400., 600.);
    frame(&mut cx, window);
    c.ok(
        114,
        "full: slot i paints row-i",
        paints(&mut cx, window, 400., 0..20, |i| format!("row-{i:02}.txt")),
    );
    let before = misses(&mut cx, window, Slot::Active);
    set(&mut cx, window, |view| view.held = held(narrowed.clone()));
    frame(&mut cx, window);
    c.ok(
        114,
        "narrowed: slot i paints row-2i",
        paints(&mut cx, window, 400., 0..20, |i| {
            format!("row-{:02}.txt", 2 * i)
        }),
    );
    let reshaped = misses(&mut cx, window, Slot::Active) - before;
    c.eq(
        131,
        "narrowing reshapes only the rows that now show another name",
        reshaped,
        19,
    );
    let palette = palette(&mut cx, window);
    let focus = Focus::default();
    let lit = (0..20).any(|index| {
        focus.plate(&palette, Slot::Active, index, false).is_some()
            || focus.edge(Slot::Active, index)
    });
    c.ok(
        139,
        "with no cursor no narrowed row paints a wash or the accent edge",
        !lit,
    );
    set(&mut cx, window, |view| view.held = held(full.clone()));
    frame(&mut cx, window);
    c.ok(
        151,
        "widened: 0..20 on a 600 px track",
        paints(&mut cx, window, 400., 0..20, |i| format!("row-{i:02}.txt")),
    );
    let built = read(&mut cx, window, |view| view.painters.constructed);
    c.eq(131, "narrowing keeps the one track paint state", built, 1);
    c.eq(154, "widening keeps it too", built, 1);
    let tall = open(&mut cx, tracks, held(narrowed), 400., 900.);
    frame(&mut cx, tall);
    c.eq(
        128,
        "a narrowed total of 20 paints no index past 19",
        visible(&mut cx, tall, Slot::Active),
        (0..20).collect::<Vec<_>>(),
    );
    set(&mut cx, tall, |view| view.held = held(full));
    frame(&mut cx, tall);
    c.eq(
        151,
        "widened to 40 paints 0..30 on a 900 px track",
        visible(&mut cx, tall, Slot::Active),
        (0..30).collect::<Vec<_>>(),
    );
    c.finish();
}

/// A clip double that answers one mark for one path and counts every question.
#[derive(Default)]
pub struct Marks {
    pub asked: Cell<usize>,
    pub held: RefCell<Option<(String, Clip)>>,
}

impl ClipMarks for Marks {
    fn is_empty(&self) -> bool {
        self.held.borrow().is_none()
    }
    fn mark(&self, dir: &str, name: &str) -> Clip {
        self.asked.set(self.asked.get() + 1);
        match &*self.held.borrow() {
            Some((path, clip)) if *path == format!("{dir}/{name}") => *clip,
            _ => Clip::None,
        }
    }
}

#[test]
fn a_long_name_takes_its_track_budget_in_every_track() {
    let mut c = Checks::new("tests/columnrow-geom.qml");
    let mut cx = real_cx();
    let one = open(
        &mut cx,
        Rc::from([active(366.)]),
        held(files([LONG.to_owned()])),
        366.,
        200.,
    );
    frame(&mut cx, one);
    c.eq(
        372,
        "the long-name row is drawn",
        visible(&mut cx, one, Slot::Active),
        vec![0],
    );
    let (prepared, whole) = look(&mut cx, one, Slot::Active, |paint, advance, window| {
        let want = fitted_name(
            window,
            LONG,
            name_budget(366., true, false, advance),
            advance,
        );
        (
            paint.prepared(0).is_some(),
            paint
                .prepared(0)
                .is_some_and(|row| says(window, row.name(false), &want)),
        )
    });
    c.ok(383, "the row has its name prepared", prepared);
    c.ok(
        410,
        "a 366 px active track paints the long name at its budget",
        whole,
    );
    let shaped = misses(&mut cx, one, Slot::Active);
    frame(&mut cx, one);
    c.eq(
        399,
        "the second frame draws the same row",
        visible(&mut cx, one, Slot::Active),
        vec![0],
    );
    // Drawn first at another width, the row reshapes once for 366 and never again for the same width.
    set(&mut cx, one, |view| view.tracks = Rc::from([active(500.)]));
    frame(&mut cx, one);
    let wide = misses(&mut cx, one, Slot::Active);
    set(&mut cx, one, |view| view.tracks = Rc::from([active(366.)]));
    frame(&mut cx, one);
    frame(&mut cx, one);
    c.eq(
        402,
        "a budget change reshapes the row exactly once",
        (wide - shaped, misses(&mut cx, one, Slot::Active) - wide),
        (1, 1),
    );
    // Peek and active tracks side by side at both widths: plain, short, chevron under the cursor and lifted.
    let rows = [(LONG, false), (SHORT, false), (LONG, true)];
    for width in [366., 853.] {
        let tracks: Rc<[TrackView]> = Rc::from([
            track(Slot::Parent, 0., width, peeks(&rows), Some(LONG)),
            track(Slot::Active, width, width, TrackRows::Listing, None),
        ]);
        let listing = rows.iter().map(|(name, dir)| row(name, *dir)).collect();
        let window = open(&mut cx, tracks, held(listing), 2. * width, 200.);
        set(&mut cx, window, |view| {
            view.focus = Rc::new(Focus {
                cursor: Some(2),
                ..Focus::default()
            })
        });
        frame(&mut cx, window);
        for (slot, shows_size) in [(Slot::Active, true), (Slot::Parent, false)] {
            let (named, fits) = look(&mut cx, window, slot, |paint, advance, window| {
                let mut named = true;
                let mut fits = true;
                for (index, (name, dir)) in rows.iter().enumerate() {
                    let chevron = *dir;
                    let budget = name_budget(width, shows_size, chevron, advance);
                    let run = paint.prepared(index).map(|row| row.name(chevron));
                    named &= run.is_some_and(|run| {
                        says(window, run, &fitted_name(window, name, budget, advance))
                    });
                    let geometry = cells(width, shows_size, chevron);
                    fits &= run.is_some_and(|run| {
                        f32::from(run.line().width) <= geometry.name_right - geometry.name_x
                    });
                }
                (named, fits)
            });
            c.ok(
                254,
                &format!("{slot:?} at {width}: plain, short and chevron names take their budgets"),
                named,
            );
            c.ok(
                259,
                &format!("{slot:?} at {width}: no drawn name runs past its slot"),
                fits,
            );
        }
    }
    // A file under the cursor and a lifted folder in the active track: plain for the file, chevron for the lifted folder.
    let lifted: Rc<[TrackView]> = Rc::from([track(
        Slot::Active,
        0.,
        366.,
        TrackRows::Listing,
        Some(LONG),
    )]);
    let listing = vec![row(LONG, false), row(SHORT, false), row(LONG, true)];
    let window = open(&mut cx, lifted, held(listing), 366., 200.);
    set(&mut cx, window, |view| {
        view.focus = Rc::new(Focus {
            cursor: Some(0),
            ..Focus::default()
        })
    });
    frame(&mut cx, window);
    let (file, folder) = look(&mut cx, window, Slot::Active, |paint, advance, window| {
        let plain = fitted_name(
            window,
            LONG,
            name_budget(366., true, false, advance),
            advance,
        );
        let chevron = fitted_name(
            window,
            LONG,
            name_budget(366., true, true, advance),
            advance,
        );
        (
            paint
                .prepared(0)
                .is_some_and(|row| says(window, row.name(false), &plain) && !row.dir),
            paint
                .prepared(2)
                .is_some_and(|row| says(window, row.name(true), &chevron) && row.dir),
        )
    });
    c.ok(254, "a file under the cursor keeps the plain budget", file);
    c.ok(
        254,
        "a lifted folder off the cursor takes the chevron budget",
        folder,
    );
    // A copied row gives the clip mark its cells.
    let marks = Rc::new(Marks::default());
    *marks.held.borrow_mut() = Some((format!("/probe/{LONG}"), Clip::Copy));
    let clip: Rc<dyn ClipMarks> = marks.clone();
    let window = open(
        &mut cx,
        Rc::from([active(366.)]),
        held(files([LONG.to_owned()])),
        366.,
        200.,
    );
    set(&mut cx, window, |view| view.clip = Some(clip));
    frame(&mut cx, window);
    let (clipped, mark) = look(&mut cx, window, Slot::Active, |paint, advance, window| {
        let take = ((GAP + CLIP) / advance).ceil() as usize;
        // A clipped row elides to the chevron budget, so its mark keeps one slot with or without a chevron.
        let want = fitted_name(
            window,
            LONG,
            name_budget(366., true, true, advance) - take,
            advance,
        );
        let geometry = cells(366., true, true);
        let slot = geometry.name_right - geometry.name_x - GAP - CLIP;
        let fits = paint.prepared(0).is_some_and(|row| {
            says(window, row.name(false), &want) && f32::from(row.name(false).line().width) <= slot
        });
        (fits, paint.clip(0))
    });
    c.eq(246, "the copied row reads its copy mark", mark, Clip::Copy);
    c.ok(254, "a clipped name gives the mark its cells", clipped);
    c.ok(259, "a clipped name stays short of the mark", clipped);
    c.covers(&[223]);
    c.finish();
}
