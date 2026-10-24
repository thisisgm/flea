#[path = "g1_08/frames.rs"]
mod frames;
#[allow(dead_code)]
#[path = "g1_06/harness.rs"]
mod harness;

use flea::protocol::RowFacts;
use flea_gui::columns::{
    cache::{Row, Shaping, TrackCache},
    geometry::{cells, name_budget, GAP},
    paint::{Clip, Palette, CLIP, CUT},
};
use flea_gui::grid::{
    element::{Clip as GridClip, Decor},
    geometry::Metrics,
    FocusSnapshot, GridView,
};
use flea_gui::list::pane::ROW_HEIGHT;
use flea_gui::palette::Theme;
use flea_gui::row::{format::Words, marks::ClipMark};
use gpui::{point, px, size, Bounds, Hsla, TextRun, TextStyle};
use harness::allocations;
use std::rc::Rc;

fn named(index: usize) -> RowFacts {
    harness::row(&format!("g_{index:05}.txt"), "text-plain", 26, false)
}

fn board_view() -> GridView {
    GridView::new(Metrics::board(), (908., 640.))
}

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
fn m9_g1rows_09_copy_and_cut_share_one_alignment_rule() {
    let mut cx = harness::real_cx();
    let pane = harness::open(&mut cx, harness::held(fixtures(), 1), 900., 5.);
    pane.with_window(&mut cx, |state, held, window| {
        let mut cache = TrackCache::default();
        for width in [260., 400.] {
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
                    scale: window.scale_factor(),
                    words: Rc::new(Words::from_catalog(&harness::english())),
                };
                cache.begin(&shaping, held.rows.len());
                for (index, facts) in held.rows.iter().enumerate() {
                    let row = cache.get(index, Row::Listing(facts), true, None, &shaping, window);
                    let width = row.name(chevron).line().width;
                    let name = Bounds::new(
                        point(px(geometry.name_x), px(0.)),
                        size(
                            px(geometry.name_right - geometry.name_x - GAP - CLIP),
                            px(ROW_HEIGHT),
                        ),
                    );
                    let copy = ClipMark::Copy.bounds(name, width, px(CLIP), px(GAP));
                    let cut = ClipMark::Cut.bounds(name, width, px(CLIP), px(GAP));
                    assert_eq!(copy, cut);
                    assert!(
                        (f32::from(copy.left()) - geometry.clip_x(f32::from(width))).abs() < 0.001
                    );
                    assert!(copy.right() <= px(geometry.name_right));
                    assert_eq!(copy.size, size(px(12.), px(12.)));
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
    let mut legacy_error = (f32::MAX, f32::MIN);
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
                        band: false,
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
                            let font = window.text_system().resolve_font(&state.font);
                            let advance = window
                                .text_system()
                                .advance(font, px(body), '0')
                                .map_or(body * 0.6, |advance| f32::from(advance.width));
                            let cells = flea_gui::grid::captions::last_line_cells(
                                drawn.caption_text().unwrap(),
                            );
                            let legacy_x = drawn.caption.x
                                + ((drawn.caption.width + cells as f32 * advance) / 2.)
                                    .min(drawn.caption.width)
                                + 9.;
                            let error = legacy_x - bounds.x;
                            legacy_error.0 = legacy_error.0.min(error);
                            legacy_error.1 = legacy_error.1.max(error);
                            assert!((bounds.x - drawn.caption.x - right - 9.).abs() < 0.001);
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
        legacy_error.0.abs().max(legacy_error.1.abs()) > 0.01,
        "fixtures must distinguish estimated advance from drawn width"
    );
    eprintln!(
        "legacy Grid anchor minus drawn-name anchor: {:.3}..{:.3} px",
        legacy_error.0, legacy_error.1
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
