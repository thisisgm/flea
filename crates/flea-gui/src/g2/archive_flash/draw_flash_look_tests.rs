//! design-hig flash.png as geometry: the strip, the caption titles at the 14 inset, the 31 rows and the 37 step row.
use super::*;
use crate::g2::archive_flash::flash::{SEL_CURSOR_EDGE, SEL_EMPTY_MARK, SEL_IMAGE_HINT};
use crate::g2::archive_flash::{cards::ROW_WASH, draw_step::DISABLED_OPACITY};
use gpui::{Bounds, Hsla, Pixels};

fn fit() -> wire::TargetRow {
    wire::TargetRow {
        dev: "sda".into(),
        seq: 2,
        serial: "2003007157171692".into(),
        label: "128GB".into(),
        model: "General USB Flash Disk".into(),
        bytes: 124623257600,
        refused: String::new(),
    }
}

fn small() -> wire::TargetRow {
    wire::TargetRow {
        dev: "sdb".into(),
        seq: 3,
        serial: "001122334455".into(),
        label: "SanDisk".into(),
        model: "SanDisk Cruzer Blade".into(),
        bytes: 2000000000,
        refused: "small".into(),
    }
}

/// A file the backend probed with no kind: the image row is refused.
fn not_an_image() -> LiveFlash {
    let mut live = LiveFlash::with_targets(vec![fit()], false);
    live.flash.open_with_image("/scope/notes.txt");
    live.flash.receive_image(
        "/scope/notes.txt",
        &wire::FlashImage {
            kind: String::new(),
            bytes: 12,
            unpacked: 0,
            known: false,
            member: String::new(),
        },
    );
    live.flash.receive_targets(vec![fit()]);
    live
}

/// The pane's slot in a 1100 by 700 window: right of the 192 rail, down to the status bar.
fn drawn(
    live: LiveFlash,
) -> (
    gpui::TestAppContext,
    VisualTestContext,
    gpui::WindowHandle<LiveFlash>,
) {
    let mut cx = real_cx();
    let window = cx.open_window(size(px(908.), px(673.)), move |_, _| live);
    let visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    (cx, visual, window)
}

pub(super) fn at(visual: &mut VisualTestContext, selector: &str) -> Bounds<Pixels> {
    // debug_bounds takes a 'static selector; the leak is a few bytes per test.
    visual
        .debug_bounds(Box::leak(selector.to_owned().into_boxed_str()))
        .unwrap_or_else(|| panic!("{selector} is drawn"))
}

fn rect(b: Bounds<Pixels>) -> (f32, f32, f32, f32) {
    (
        f32::from(b.origin.x),
        f32::from(b.origin.y),
        f32::from(b.size.width),
        f32::from(b.size.height),
    )
}

#[test]
fn the_board_state_lays_out_as_flash_png() {
    let mut live = LiveFlash::with_targets(vec![fit(), small()], false);
    let view = live.flash.view(&live.i18n);
    assert!(view.drives[0].cursor && view.drives[1].dimmed, "{view:?}");
    assert_eq!(
        view.step.action, "Flash 128GB",
        "the button names the drive"
    );
    assert_eq!(view.step.line, "Erases everything on 128GB");
    assert_eq!(view.status, "2 drives");
    assert!(
        !view.step.resting,
        "a live Flash leaves the warning at full ink"
    );
    live.radius = 8.;
    let (_cx, mut visual, _) = drawn(live);
    // The pane's own strip: 27 tall, Back then an unavailable Up.
    assert_eq!(
        rect(at(&mut visual, "op2-flash-strip")),
        (0., 0., 908., 27.)
    );
    assert_eq!(rect(at(&mut visual, "op2-flash-back")).0, 14.);
    assert_eq!(rect(at(&mut visual, "op2-flash-up")).0, 47.);
    // Section titles sit at the 14 inset, 30 tall, at the board's tops under the strip.
    for (index, top) in [(0, 9.), (1, 84.), (2, 221.)] {
        let title = rect(at(&mut visual, &format!("op2-flash-title-{index}")));
        assert_eq!((title.1 - 27., title.3), (top, 30.), "title {index}");
    }
    assert_eq!(
        rect(at(&mut visual, SEL_IMAGE_ROW)),
        (0., 27. + 42., 908., 31.)
    );
    assert_eq!(
        rect(at(&mut visual, "op2-flash-drive-0")),
        (0., 27. + 117., 908., 31.)
    );
    assert_eq!(
        rect(at(&mut visual, "op2-flash-drive-1")),
        (0., 27. + 148., 908., 31.)
    );
    // The refusal line under the too-small drive: 9 above, a 15 line, 7 below.
    assert_eq!(
        rect(at(&mut visual, "op2-flash-drive-hint-1")),
        (0., 27. + 179., 908., 31.)
    );
    // The 37 step row centres the warning and the button on the board's 272.5.
    assert_eq!(
        rect(at(&mut visual, "op2-flash-step")),
        (0., 27. + 254., 908., 37.)
    );
    let button = rect(at(&mut visual, "op2-flash-action"));
    assert_eq!(
        button.0 + button.2,
        908. - 14.,
        "the button ends at the inset"
    );
    assert_eq!(button.1 + button.3 / 2., 27. + 272.5);
    // The cursor rung's edge stands flush on the pane's left, 2 wide, over the chosen drive.
    assert_eq!(
        rect(at(&mut visual, SEL_CURSOR_EDGE)),
        (0., 27. + 117., 2., 31.)
    );
}

#[test]
fn no_drive_plugged_in_draws_the_empty_row_and_a_greyed_step() {
    let live = LiveFlash::with_targets(Vec::new(), false);
    let view = live.flash.view(&live.i18n);
    assert!(view.step.resting && !view.step.live, "{:?}", view.step);
    assert_eq!(view.status, "0 drives");
    let (_cx, mut visual, _) = drawn(live);
    assert_eq!(
        rect(at(&mut visual, SEL_EMPTY_MARK)),
        (0., 27. + 117., 908., 31.)
    );
    let flash = rect(at(&mut visual, "op2-flash-title-2"));
    assert_eq!(flash.1 - 27., 117. + 31. + 11.);
    assert!(
        visual.debug_bounds("op2-flash-action").is_some(),
        "the greyed Flash stays drawn"
    );
}

#[test]
fn a_file_that_is_not_a_disk_image_says_so_under_its_row() {
    let live = not_an_image();
    let view = live.flash.view(&live.i18n);
    assert!(view.image.refused && !view.step.live, "{:?}", view.image);
    assert_eq!(view.image.hint, "This file is not a disk image.");
    let (_cx, mut visual, _) = drawn(live);
    assert_eq!(
        rect(at(&mut visual, SEL_IMAGE_HINT)),
        (0., 27. + 73., 908., 31.)
    );
    let drives = rect(at(&mut visual, "op2-flash-title-1"));
    assert_eq!(drives.1 - 27., 73. + 31. + 11.);
}

#[test]
fn the_keyboard_cursor_draws_its_flush_edge_on_each_stop() {
    let mut live = LiveFlash::with_targets(vec![fit(), small()], false);
    // Image, the chosen drive, then the Flash step.
    let mut tops = Vec::new();
    for _ in 0..3 {
        let view = live.flash.view(&live.i18n);
        tops.push(if view.image.cursor {
            42.
        } else if view.step.cursor {
            254.
        } else {
            117.
        });
        live.flash.move_cursor(1);
    }
    assert_eq!(
        tops,
        [117., 254., 254.],
        "down from the chosen drive skips the refused one"
    );
    live.flash.move_cursor(-1);
    live.flash.move_cursor(-1);
    assert!(live.flash.view(&live.i18n).image.cursor);
    let (mut cx, mut visual, window) = drawn(live);
    assert_eq!(
        rect(at(&mut visual, SEL_CURSOR_EDGE)),
        (0., 27. + 42., 2., 31.)
    );
    window
        .update(&mut cx, |live, _, cx| {
            live.flash.move_cursor(1);
            live.flash.move_cursor(1);
            cx.notify();
        })
        .unwrap();
    visual.run_until_parked();
    let step = rect(at(&mut visual, "op2-flash-step"));
    assert_eq!(
        rect(at(&mut visual, SEL_CURSOR_EDGE)),
        (0., step.1, 2., 37.)
    );
}

#[test]
fn the_erase_warning_takes_the_theme_error_role() {
    let palette = Palette::inherited(&gpui::TextStyle::default(), Theme::default());
    assert_ne!(
        palette.error, palette.accent,
        "the fixture theme tells the two roles apart"
    );
    assert_eq!(
        super::super::cards::Tokens::of(&palette).error,
        palette.error
    );
}

/// Catppuccin Latte's own colours: a theme whose muted and metadata frames differ, as on the desktop.
pub(super) fn latte() -> Theme {
    Theme::from_text(
        "background='#eff1f5'\nforeground='#4c4f69'\naccent='#1e66f5'\ncolor1='#d20f39'\nmuted='#9ca0b0'",
    )
}

/// The flash drawn under `theme`, with the quads the frame actually painted and the palette it read.
pub(super) fn painted(
    live: LiveFlash,
    theme: Theme,
) -> (VisualTestContext, Vec<gpui::Quad>, Palette, f32) {
    let mut cx = real_cx();
    cx.update(|cx| cx.set_global(theme));
    let window = cx.open_window(size(px(908.), px(673.)), move |_, _| live);
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let (quads, palette, scale) = visual.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let palette = Palette::inherited(&window.text_style(), theme);
        (window.painted_quads(), palette, window.scale_factor())
    });
    (visual, quads, palette, scale)
}

pub(super) fn quads_at(quads: &[gpui::Quad], b: Bounds<Pixels>, scale: f32) -> Vec<&gpui::Quad> {
    let want = rect(b);
    let want = [want.0, want.1, want.2, want.3].map(|v| v * scale);
    quads
        .iter()
        .filter(|q| {
            let got = [
                q.bounds.origin.x,
                q.bounds.origin.y,
                q.bounds.size.width,
                q.bounds.size.height,
            ]
            .map(|v| v.as_f32());
            got.iter()
                .zip(want)
                .all(|(got, want)| (got - want).abs() <= 1.)
        })
        .collect()
}

#[test]
fn the_chosen_drive_keeps_the_selection_plate_while_the_cursor_is_elsewhere() {
    let mut live = LiveFlash::with_targets(vec![fit(), small()], false);
    live.flash.move_cursor(-1);
    let view = live.flash.view(&live.i18n);
    assert!(
        view.image.cursor && view.drives[0].selected && !view.drives[0].cursor,
        "{view:?}"
    );
    let (mut visual, quads, palette, scale) = painted(live, latte());
    // FlashDriveRows.qml:48-49: off the cursor the chosen drive draws Style.selectionFill, the marked rows' plate.
    let drive = at(&mut visual, "op2-flash-drive-0");
    let plates: Vec<_> = quads_at(&quads, drive, scale)
        .iter()
        .map(|q| q.background.as_solid())
        .collect();
    assert!(
        plates.contains(&Some(palette.marked)),
        "chosen drive plates {plates:?}, want {:?}",
        palette.marked
    );
}

#[test]
fn the_flash_frames_take_the_muted_frame_role() {
    let live = LiveFlash::with_targets(Vec::new(), false);
    let (mut visual, quads, palette, scale) = painted(live, latte());
    assert_ne!(
        palette.muted_frame, palette.metadata_frame,
        "the fixture theme tells the two frames apart"
    );
    // Flash*.qml draw the empty row's mark and the greyed Flash frame in Theme.color.mutedFrame.
    let button = at(&mut visual, "op2-flash-action");
    let frames: Vec<_> = quads_at(&quads, button, scale)
        .iter()
        .filter(|q| q.border_widths.top.0 > 0.)
        .map(|q| q.border_color)
        .collect();
    let same = |got: &Hsla, want: Hsla| {
        (got.h - want.h).abs() < 1e-3
            && (got.s - want.s).abs() < 1e-3
            && (got.l - want.l).abs() < 1e-3
    };
    assert!(
        !same(&palette.metadata_frame, palette.muted_frame),
        "the two frames differ in ink, not only alpha"
    );
    assert!(
        !frames.is_empty() && frames.iter().all(|f| same(f, palette.muted_frame)),
        "greyed Flash frames {frames:?}, want {:?}",
        palette.muted_frame
    );
}

#[test]
fn the_greyed_flash_step_still_takes_the_keyboard_cursor() {
    let mut live = LiveFlash::with_targets(Vec::new(), false);
    // FlashView.qml:144 walks to the step row whether or not Flash is live.
    live.flash.move_cursor(1);
    let view = live.flash.view(&live.i18n);
    assert!(view.step.cursor && !view.step.live, "{:?}", view.step);
    assert!(
        live.flash.context_press().is_empty(),
        "Enter on a greyed Flash sends nothing"
    );
    // A chosen drive under a refused image: only the live guard keeps Enter from flashing it.
    let mut refused = not_an_image();
    for _ in 0..3 {
        refused.flash.move_cursor(1);
    }
    let seen = refused.flash.view(&refused.i18n);
    assert!(
        seen.step.cursor && seen.drives[0].selected && !seen.step.live,
        "{seen:?}"
    );
    assert!(
        refused.flash.context_press().is_empty(),
        "Enter on a greyed Flash over a chosen drive sends nothing"
    );
    let (_cx, mut visual, _) = drawn(live);
    let step = rect(at(&mut visual, "op2-flash-step"));
    assert_eq!(
        rect(at(&mut visual, SEL_CURSOR_EDGE)),
        (0., step.1, 2., 37.)
    );
}

#[test]
fn the_greyed_flash_step_dims_its_cursor_plate_and_edge() {
    let mut live = LiveFlash::with_targets(Vec::new(), false);
    live.flash.move_cursor(1);
    let (mut visual, quads, palette, scale) = painted(live, latte());
    // FlashStepRow.qml:46 dims the whole greyed row, its cursor plate and edge with it.
    let alphas = |b| -> Vec<f32> {
        quads_at(&quads, b, scale)
            .iter()
            .filter_map(|q| q.background.as_solid())
            .map(|c| c.a)
            .collect()
    };
    let near = |got: &[f32], want: f32| got.iter().any(|a| (a - want).abs() < 1e-3);
    let plate = alphas(at(&mut visual, "op2-flash-step"));
    assert!(
        near(&plate, ROW_WASH * DISABLED_OPACITY),
        "step plate alphas {plate:?}"
    );
    let edge = alphas(at(&mut visual, SEL_CURSOR_EDGE));
    assert!(
        near(&edge, palette.accent_frame.a * DISABLED_OPACITY),
        "step edge alphas {edge:?}"
    );
}
