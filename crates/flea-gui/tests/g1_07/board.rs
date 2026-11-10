//! The Columns boards' own numbers: track rects, dividers, row cells and each row state's plate and inks.
use super::fixture::row;
use super::fixture::words;
use super::frames::{frame, held, open, real_cx, set, typeface};
use super::track_frames::{self, look};
use flea::protocol::RowFacts;
use flea_gui::columns::cache::size_text;
use flea_gui::columns::geometry::{
    cells, mark_box, Layout, Slot, CHEVRON, MARK, MARK_SLOT, PAD_X, SIZE_W,
};
use flea_gui::columns::paint::Palette;
use flea_gui::columns::{TrackRows, TrackState, TrackView};
use flea_gui::navigation::sort::{DirSizes, Size};
use flea_gui::palette::Theme;
use flea_gui::pane::states::hero_span;
use flea_gui::pane::ROW_HEIGHT;
use gpui::{Hsla, TestAppContext, TextRun};
use std::rc::Rc;

// The 1100 x 700 boards: a 192 rail leaves the columns 908 wide; the four-column boards are 2560 wide, 2368 after the rail.
const AREA: f32 = 908.;
const AREA_WIDE: f32 = 2368.;

fn rects(layout: &Layout) -> Vec<(Slot, f32, f32)> {
    layout.tracks().map(|t| (t.slot, t.x, t.width)).collect()
}

fn view(slot: Slot, x: f32, width: f32, divider: bool) -> TrackView {
    TrackView {
        slot,
        x,
        width,
        path: String::new(),
        rows: TrackRows::Blank,
        dim: false,
        lifted: None,
        lifted_at: None,
        divider,
        state: TrackState::Rows,
        enabled: true,
    }
}

pub(super) fn palette() -> Palette {
    // The theme roles stand in: every assertion below wires roles to roles, never absolute inks.
    // Each role blends the theme foreground at its own opacity, so a role drawn in another's place changes the ink.
    let theme = Theme::default();
    let foreground = theme.foreground;
    Palette {
        background: theme.background,
        foreground,
        muted: foreground.opacity(0.5),
        muted_frame: foreground.opacity(0.4),
        accent: foreground.opacity(0.8),
        accent_frame: foreground.opacity(0.7),
        metadata: foreground.opacity(0.6),
    }
}

#[test]
fn the_palette_fixture_gives_each_role_its_own_ink() {
    let palette = palette();
    let roles = [
        palette.background,
        palette.foreground,
        palette.muted,
        palette.muted_frame,
        palette.accent,
        palette.accent_frame,
        palette.metadata,
    ];
    for (at, role) in roles.iter().enumerate() {
        assert!(
            roles[at + 1..].iter().all(|other| other != role),
            "role {at} shares its ink with a later role"
        );
    }
}

pub(super) fn alpha(ink: Hsla, a: f32) -> Hsla {
    Hsla { a, ..ink }
}

#[test]
fn the_default_board_draws_three_tracks_302_302_304_with_two_hairlines() {
    let layout = Layout::new(AREA, None, 0., None);
    let want = vec![
        (Slot::Parent, 0., 302.),
        (Slot::Active, 302., 302.),
        (Slot::Child, 604., 304.),
    ];
    assert_eq!(
        rects(&layout),
        want,
        "columns.html: surfaces at left 0, 302 and 604, 302, 302 and 304 wide"
    );
    assert_eq!(
        view(Slot::Parent, 0., 302., true).divider_x(),
        Some(301.),
        "the parent's 1 px hair at right 0"
    );
    assert_eq!(
        view(Slot::Active, 302., 302., true).divider_x(),
        Some(603.),
        "the active track's 1 px hair at right 0"
    );
    assert_eq!(
        view(Slot::Child, 604., 304., false).divider_x(),
        None,
        "the third column draws no hair"
    );
}

#[test]
fn two_and_four_column_boards_keep_their_rects_and_hero_spans() {
    let two = Layout::new(AREA, Some(2.), 0., None);
    assert_eq!(
        rects(&two),
        vec![(Slot::Active, 0., 454.), (Slot::Child, 454., 454.)],
        "cols2-*: active and third"
    );
    // cols2-hero-*: the hero box at left 0, 908 wide, since the active track starts at 0.
    assert_eq!(hero_span(AREA, two.active_x(), 140.), (0., AREA));
    let four = Layout::new(AREA_WIDE, Some(4.), 0., None);
    let want = vec![
        (Slot::Grandparent, 0., 592.),
        (Slot::Parent, 592., 592.),
        (Slot::Active, 1184., 592.),
        (Slot::Child, 1776., 592.),
    ];
    assert_eq!(rects(&four), want, "cols4-*: four tracks 592 wide");
    // cols4-hero-*: the hero box at left 1184, 1184 wide, clear of both ancestor tracks.
    assert_eq!(hero_span(AREA_WIDE, four.active_x(), 140.), (1184., 1184.));
    // cols-insideempty and cols-filter-nomatch: a 3-column hero centres on the area, which keeps it inside the active track.
    let three = Layout::new(AREA, None, 0., None);
    let (start, span) = hero_span(AREA, three.active_x(), 160.);
    assert!(start + span / 2. - 80. >= three.active_x() && start + span / 2. + 80. <= three.edge());
}

#[test]
fn a_row_keeps_the_board_cells_in_every_track() {
    assert_eq!(ROW_HEIGHT, 31., ".row height 31");
    // v0.3.12 ColumnRow.qml: Glyph caps the 23 slot at Theme.markSize, bodySmall 13 x 1.45, the list's own mark.
    let mark_size = (13. * 1.45_f64) as f32;
    assert_eq!(
        (PAD_X, MARK_SLOT, MARK),
        (14., 23., mark_size),
        "14 left padding, 23 glyph box, an 18.85 mark"
    );
    assert_eq!(
        mark_box(31.),
        (
            14. + (23. - mark_size) / 2.,
            (31. - mark_size) / 2.,
            mark_size
        ),
        "the painted mark centres its 18.85 square in the slot and on the row"
    );
    // The active 302 track: name at 46, the 70 wide size ending 9 before the 28 right padding, at 265.
    let active = cells(302., true, false);
    assert_eq!((active.mark_x, active.name_x), (14., 46.));
    assert_eq!(active.size_right, Some(302. - 28. - 9.));
    assert_eq!(active.name_right, 302. - 28. - 9. - SIZE_W - 9.);
    // The cursor folder in the active track: size then 9, then the 12 chevron ending at the 28 padding.
    let cursor_dir = cells(302., true, true);
    assert_eq!(cursor_dir.chevron_x, Some(302. - 28. - CHEVRON));
    assert_eq!(cursor_dir.size_right, Some(302. - 28. - CHEVRON - 9.));
    // The lifted ancestor folder: no size, its chevron at the same 28 padding.
    let lifted = cells(302., false, true);
    assert_eq!((lifted.size_right, lifted.chevron_x), (None, Some(262.)));
    // A peek row: no size, no chevron, the name runs to the padding.
    let peek = cells(304., false, false);
    assert_eq!(
        (peek.size_right, peek.chevron_x, peek.name_right),
        (None, None, 304. - 28.)
    );
}

#[test]
fn a_folder_size_reads_its_answered_walk() {
    let folder = row("Docs", true);
    // cols-filter-rest: Docs 22.1 kB, Locked >60 B; before an answer a folder shows the dot, a file its own size.
    assert_eq!(
        size_text(
            &folder,
            Some(Size {
                bytes: 22_100,
                partial: false
            }),
            &words()
        ),
        "22.1 kB"
    );
    assert_eq!(
        size_text(
            &folder,
            Some(Size {
                bytes: 60,
                partial: true
            }),
            &words()
        ),
        ">60 B"
    );
    assert_eq!(size_text(&folder, None, &words()), "\u{b7}");
    let file = row("notes.txt", false);
    assert_eq!(
        size_text(
            &file,
            Some(Size {
                bytes: 99,
                partial: false
            }),
            &words()
        ),
        "13 B"
    );
    let link = RowFacts {
        mode: 0o120777,
        ..row("A", true)
    };
    assert_eq!(
        size_text(
            &link,
            Some(Size {
                bytes: 462,
                partial: false
            }),
            &words()
        ),
        "link"
    );
}

#[test]
fn the_active_track_draws_the_walk_and_reshapes_once_when_it_lands() {
    let mut cx = real_cx();
    let rows = vec![row("A", true), row("Locked", true), row("notes.txt", false)];
    let tracks: Rc<[TrackView]> = Rc::from(vec![track_frames::active(302.)]);
    let window = open(&mut cx, tracks, held(rows), 302., 200.);
    frame(&mut cx, window);
    let caption = |cx: &mut TestAppContext, index: usize, want: &str| {
        look(cx, window, Slot::Active, |paint, _, window| {
            let Some(run) = paint.prepared(index).and_then(|row| row.size.as_ref()) else {
                return false;
            };
            let face = typeface();
            let style = TextRun {
                len: want.len(),
                font: face.font.clone(),
                color: Hsla::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = window
                .text_system()
                .layout_line(want, face.caption, &[style], None);
            let ids = |runs: &[gpui::ShapedRun]| {
                runs.iter()
                    .flat_map(|r| r.glyphs.iter().map(|g| g.id))
                    .collect::<Vec<_>>()
            };
            run.line().len == line.len && ids(&run.line().runs) == ids(&line.runs)
        })
    };
    assert!(
        caption(&mut cx, 0, "\u{b7}"),
        "an unasked folder draws the dot"
    );
    let before = track_frames::misses(&mut cx, window, Slot::Active);
    let mut sizes = DirSizes::default();
    sizes.remember(
        0,
        Size {
            bytes: 462,
            partial: false,
        },
        8,
    );
    sizes.remember(
        1,
        Size {
            bytes: 60,
            partial: true,
        },
        8,
    );
    sizes.remember(
        2,
        Size {
            bytes: 5,
            partial: false,
        },
        8,
    );
    set(&mut cx, window, |view| view.sizes = Some(Rc::new(sizes)));
    frame(&mut cx, window);
    assert!(
        caption(&mut cx, 0, "462 B") && caption(&mut cx, 1, ">60 B"),
        "answered walks draw in the size cell"
    );
    assert!(
        caption(&mut cx, 2, "13 B"),
        "a file keeps its own size whatever the walk table says"
    );
    assert_eq!(
        track_frames::misses(&mut cx, window, Slot::Active) - before,
        2,
        "only the two answered folders reshape"
    );
    frame(&mut cx, window);
    assert_eq!(
        track_frames::misses(&mut cx, window, Slot::Active) - before,
        2,
        "an unchanged walk reshapes nothing"
    );
}
