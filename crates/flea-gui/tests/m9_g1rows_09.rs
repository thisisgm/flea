#[path = "g1_08/frames.rs"]
mod frames;
#[allow(dead_code)]
#[path = "g1_06/harness.rs"]
mod harness;

use flea::protocol::RowFacts;
use flea_gui::columns::{
    cache::{Row, Shaping, TrackCache},
    geometry::{cells, name_budget, CHEVRON, GAP},
    paint::{Clip, Palette, CLIP, CUT},
};
use flea_gui::grid::{
    element::{Clip as GridClip, Decor},
    geometry::Metrics,
    FocusSnapshot, GridView,
};
use flea_gui::palette::Theme;
use flea_gui::row::{format::Words, marks::ClipMark};
use gpui::{px, Hsla, TextRun, TextStyle};
use harness::allocations;
use std::rc::Rc;

fn named(index: usize) -> RowFacts {
    harness::row(&format!("g_{index:05}.txt"), "text-plain", 26, false)
}

fn board_view() -> GridView {
    GridView::new(Metrics::board(), (908., 640.))
}

/// A two-track window's active track; a clipped row this narrow has no cells left for a location.
const NARROW_TRACK: f32 = 224.;

fn fixtures() -> Vec<RowFacts> {
    [
        "a.txt".into(),
        "very-long-name-".repeat(12),
        "文書-e\u{301}.txt".into(),
    ]
    .iter()
    .map(|name: &String| harness::row(name, "text-plain", 26, false))
    .collect()
}

#[test]
fn m9_g1rows_09_columns_mark_slot_ignores_chevron_and_name_length() {
    let mut cx = harness::real_cx();
    let pane = harness::open(&mut cx, harness::held(fixtures(), 1), 900., 5.);
    let mut rows = fixtures();
    rows.extend(
        [
            "src/deep/folder/a.txt".to_owned(),
            format!(
                "{}/{}",
                "far-away-folder".repeat(6),
                "very-long-name-".repeat(6)
            ),
        ]
        .iter()
        .map(|name| harness::row(name, "text-plain", 26, false)),
    );
    pane.with_window(&mut cx, |state, _, window| {
        let mut cache = TrackCache::default();
        for width in [NARROW_TRACK, 260., 400.] {
            for shows_size in [false, true] {
                let plain = cells(width, shows_size, false);
                assert_eq!(plain.clip_x(), cells(width, shows_size, true).clip_x());
                assert_eq!(plain.clip_x() + CLIP, plain.name_right - CHEVRON);
            }
            for locating in [false, true] {
                for chevron in [false, true] {
                    let geometry = cells(width, true, chevron);
                    let advance = 8.4;
                    let shaping = Shaping {
                        font: state.typeface.font.clone(),
                        body: px(14.),
                        caption: px(12.),
                        plain_cells: name_budget(width, true, false, advance),
                        chevron_cells: name_budget(width, true, true, advance),
                        clip_cells: ((GAP + CLIP) / advance).ceil() as usize,
                        size: true,
                        locating,
                        languages: true,
                        scale: window.scale_factor(),
                        words: Rc::new(Words::from_catalog(&harness::english())),
                        query: "".into(),
                    };
                    cache.begin(&shaping, rows.len());
                    for (index, facts) in rows.iter().enumerate() {
                        let row =
                            cache.get(index, Row::Listing(facts), true, None, &shaping, window);
                        let name = row.name(chevron).line().width;
                        assert!(name > px(0.));
                        assert_eq!(
                            row.location.is_some(),
                            locating && facts.name.contains('/') && width > NARROW_TRACK,
                            "{width} px, {:?}: location shown",
                            facts.name
                        );
                        let location = row
                            .location
                            .as_ref()
                            .map_or(0., |run| GAP + f32::from(run.line().width));
                        let end = geometry.name_x + f32::from(name) + location;
                        assert!(
                            end <= geometry.clip_x() - GAP + 0.001,
                            "{width} px, {:?}: name and location end at {end}, the mark starts at {}",
                            facts.name,
                            geometry.clip_x()
                        );
                        // Unclipped, the same row keeps its location, so only the mark slot hides it.
                        let unclipped =
                            cache.get(index, Row::Listing(facts), false, None, &shaping, window);
                        assert_eq!(
                            unclipped.location.is_some(),
                            locating && facts.name.contains('/'),
                            "{width} px, {:?}: unclipped location shown",
                            facts.name
                        );
                    }
                }
            }
        }
    });
}

struct Clipboard(Option<ClipMark>);

impl Decor for Clipboard {
    fn clip(&self, _: usize) -> Option<GridClip> {
        let mark = self.0?;
        let (key, svg) = mark.svg();
        Some(GridClip {
            cut: mark == ClipMark::Cut,
            key,
            svg,
        })
    }
}

#[test]
fn m9_g1rows_09_marks_do_not_shift_with_selection_or_elision() {
    let mut cx = frames::real_cx();
    let window = frames::open(&mut cx, frames::held(fixtures()));
    let grid = frames::state(&mut cx, window);
    let mut drifting_error = (f32::MAX, f32::MIN);
    for body in [12., 14., 20.] {
        frames::with(&mut cx, window, |host| {
            host.view.metrics.body_small = body;
            host.view.metrics.caption_line = (body * 1.26).round();
        });
        let mut copied = Vec::new();
        for mark in [ClipMark::Copy, ClipMark::Cut] {
            frames::with(&mut cx, window, |host| {
                host.decor = Rc::new(Clipboard(Some(mark)))
            });
            for rung in 0..4 {
                frames::with(&mut cx, window, |host| {
                    host.focus = Rc::new(FocusSnapshot {
                        cursor: if rung == 2 { 0 } else { usize::MAX },
                        selected: if rung == 3 { vec![0, 1, 2] } else { Vec::new() },
                        ..Default::default()
                    });
                    host.view.hovered = (rung == 1).then_some(0);
                });
                frames::frame(&mut cx, window);
                window
                    .update(&mut cx, |_, window, _| {
                        let state = grid.borrow();
                        for drawn in state.drawn() {
                            let bounds = drawn.clip_bounds(&state).unwrap();
                            let last = drawn
                                .caption_text()
                                .unwrap()
                                .split('\n')
                                .nth(drawn.lines - 1)
                                .unwrap();
                            let line = window.text_system().layout_line(
                                last,
                                px(body),
                                &[TextRun {
                                    len: last.len(),
                                    font: state.font.clone(),
                                    color: Hsla::default(),
                                    background_color: None,
                                    underline: None,
                                    strikethrough: None,
                                }],
                                None,
                            );
                            let trimmed = last
                                .trim_end_matches(|ch: char| ch.is_whitespace() || ch == '\u{feff}')
                                .len();
                            let right =
                                ((drawn.caption.width + f32::from(line.x_for_index(trimmed))) / 2.)
                                    .min(drawn.caption.width);
                            let drifting_x = drawn.caption.x + right + 9.;
                            let error = drifting_x - bounds.x;
                            drifting_error.0 = drifting_error.0.min(error);
                            drifting_error.1 = drifting_error.1.max(error);
                            assert!((bounds.x - drawn.caption.right() - 9.).abs() < 0.001);
                            assert_eq!((bounds.width, bounds.height), (12., 12.));
                            assert!(bounds.right() <= drawn.caption.right() + 9. + 12.);
                            let want_y = drawn.caption.y
                                + drawn.lines as f32 * state.layout.unwrap().metrics.caption_line
                                - (state.layout.unwrap().metrics.caption_line + 12.) / 2.
                                - 1.;
                            assert!((bounds.y - want_y).abs() < 0.001);
                            if mark == ClipMark::Copy && rung == 0 {
                                copied.push(bounds);
                            } else {
                                assert_eq!(bounds, copied[drawn.listing]);
                            }
                        }
                    })
                    .unwrap();
            }
        }
        let misses = grid.borrow().shaped().1;
        frames::frame(&mut cx, window);
        assert_eq!(grid.borrow().shaped().1, misses);
        assert!(grid.borrow().cached_tiles() <= 3);
        assert!(grid.borrow().drawn().iter().any(|drawn| drawn.elided()));
    }
    assert!(
        drifting_error.0.abs().max(drifting_error.1.abs()) > 0.01,
        "fixtures must distinguish filename endings from the reserved slot"
    );
    eprintln!(
        "filename-ending Grid anchor minus reserved-slot anchor: {:.3}..{:.3} px",
        drifting_error.0, drifting_error.1
    );
    frames::with(&mut cx, window, |host| {
        host.decor = Rc::new(Clipboard(None))
    });
    frames::frame(&mut cx, window);
    assert!(grid
        .borrow()
        .drawn()
        .iter()
        .all(|drawn| drawn.clip_bounds(&grid.borrow()).is_none()));
}

#[test]
fn m9_g1rows_09_cut_names_keep_the_copy_contrast() {
    let palette = Palette::inherited(&TextStyle::default(), Theme::default());
    for (cursor, lifted, dim) in [
        (false, false, false),
        (false, false, true),
        (false, true, true),
        (true, false, true),
    ] {
        let copy = palette.row_ink(Clip::Copy, cursor, lifted, dim);
        let cut = palette.row_ink(Clip::Cut, cursor, lifted, dim);
        assert_eq!(cut.name, copy.name, "cut names must retain their contrast");
        assert_eq!(cut.clip, copy.clip);
        assert_eq!(cut.chevron, copy.chevron);
        assert_eq!(cut.mark, copy.mark.opacity(CUT));
    }
}
