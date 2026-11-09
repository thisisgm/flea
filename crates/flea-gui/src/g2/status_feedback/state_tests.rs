use super::hero_tests::{bounds, english, frame, run};
use super::mark::Glyph;
use super::state::{Facts, Kind, StateMessage, GRID, LIST};
use super::Ink;
use gpui::{px, size, Pixels, TestAppContext, WindowHandle};

fn open(cx: &mut TestAppContext) -> WindowHandle<StateMessage> {
    let handle = cx.add_window(|window, _| {
        window.set_rem_size(px(14.));
        StateMessage::new()
    });
    cx.run_until_parked();
    handle
}

fn renders(cx: &mut TestAppContext, handle: WindowHandle<StateMessage>) -> usize {
    handle
        .update(cx, |view, _, _| view.counters.renders.get())
        .unwrap()
}

#[test]
fn a_denied_listing_is_locked_with_its_mode_and_the_board_sentence() {
    let catalog = english();
    let locked = Facts::failed("scan", "Permission denied (os error 13)", 0o40700, &catalog);
    assert_eq!(locked.kind, Kind::Locked);
    assert_eq!(locked.glyph(), Glyph::Lock);
    assert_eq!(
        locked.sentence.as_ref(),
        "You don’t have permission to open this folder."
    );
    assert_eq!(locked.detail.as_ref(), "rwx------ · not yours");
    let unstated = Facts::failed("scan", "permission denied", 0, &catalog);
    assert_eq!(unstated.detail.as_ref(), "");
    let missing = Facts::failed("scan", "item not found", 0o40755, &catalog);
    assert_eq!((missing.kind, missing.glyph()), (Kind::Error, Glyph::Alert));
    assert_eq!(missing.sentence.as_ref(), "That folder could not be read.");
    assert_eq!(missing.detail.as_ref(), "");
    let waiting = Facts::waiting(&catalog);
    assert_eq!(
        (waiting.kind, waiting.glyph(), waiting.sentence.as_ref()),
        (
            Kind::Waiting,
            Glyph::Alert,
            "That folder is not responding."
        )
    );
}

/// grid-locked, locked, locked-menu: the padlock over the permission sentence wears the error ink, the footer message's ink, as the alert does.
#[test]
fn a_locked_mark_wears_the_error_ink() {
    let catalog = english();
    let ink = |value: u32| -> gpui::Hsla { gpui::rgb(value).into() };
    let inks = Ink {
        foreground: ink(0xcacccc),
        metadata: ink(0x808080),
        metadata_frame: ink(0x808080),
        muted: ink(0x808080),
        muted_frame: ink(0x808080),
        error: ink(0xf38ba8),
    };
    let locked = Facts::failed("scan", "Permission denied (os error 13)", 0o40700, &catalog);
    assert_eq!(
        locked.mark_ink(inks),
        inks.error,
        "the lock wears the error ink"
    );
    let missing = Facts::failed("scan", "item not found", 0o40755, &catalog);
    assert_eq!(
        missing.mark_ink(inks),
        inks.error,
        "the alert wears the error ink"
    );
}

#[test]
fn state_glyphs_carry_the_icons_js_paths() {
    for (glyph, key, path) in [
        (
            Glyph::Lock,
            "flea-state:lock",
            "M4 11h16v10H4z M8 11V7a4 4 0 0 1 8 0v4 M12 15h.01",
        ),
        (
            Glyph::Alert,
            "flea-state:alert",
            "M12 2 23 21H1z M12 10v4 M12 18L12.01 18",
        ),
        (
            Glyph::Search,
            "flea-state:search",
            "M16.7 16.7 21 21 M3 11A8 8 0 1 0 19 11A8 8 0 1 0 3 11Z",
        ),
    ] {
        let (name, data) = glyph.svg();
        assert_eq!(name.as_ref(), key);
        assert!(std::str::from_utf8(data)
            .unwrap()
            .contains(&format!(r#"<path d="{path}"/>"#)));
    }
}

#[gpui::test]
fn the_surface_repaints_only_when_its_facts_change(cx: &mut TestAppContext) {
    let catalog = english();
    let handle = open(cx);
    let facts = Facts::failed("scan", "Permission denied", 0o40750, &catalog);
    handle
        .update(cx, |view, _, cx| view.set(Some(facts.clone()), cx))
        .unwrap();
    run(cx, handle, 32);
    let drawn = renders(cx, handle);
    handle
        .update(cx, |view, _, cx| view.set(Some(facts.clone()), cx))
        .unwrap();
    assert_eq!(run(cx, handle, 320), 0);
    assert_eq!(renders(cx, handle), drawn);
    handle.update(cx, |view, _, cx| view.set(None, cx)).unwrap();
    run(cx, handle, 32);
    assert_eq!(renders(cx, handle), drawn + 1);
    assert!(handle
        .update(cx, |view, _, _| view.facts().is_none())
        .unwrap());
}

/// The list boards (error, locked) and the grid-locked board state different geometry, and each surface draws its own.
#[gpui::test]
fn each_surface_takes_its_boards_geometry_and_repaints_once_on_a_switch(cx: &mut TestAppContext) {
    assert_eq!(
        (LIST.top, LIST.gap, LIST.under_mark, LIST.detail_drop),
        (0., 10., 0., 2.)
    );
    assert_eq!(
        (LIST.sentence_line, LIST.detail_line),
        (14. * 1.2, 12. * 1.2)
    );
    assert_eq!(
        (GRID.top, GRID.gap, GRID.under_mark, GRID.detail_drop),
        (3., 9., 1., 0.)
    );
    assert_eq!((GRID.sentence_line, GRID.detail_line), (14. * 1.32, 22.));
    let catalog = english();
    let handle = open(cx);
    handle
        .update(cx, |view, _, cx| {
            view.set(
                Some(Facts::failed(
                    "scan",
                    "Permission denied",
                    0o40000,
                    &catalog,
                )),
                cx,
            )
        })
        .unwrap();
    run(cx, handle, 32);
    assert_eq!(
        handle.update(cx, |view, _, _| view.metrics()).unwrap(),
        LIST
    );
    let drawn = renders(cx, handle);
    handle
        .update(cx, |view, _, cx| view.set_metrics(GRID, cx))
        .unwrap();
    run(cx, handle, 32);
    handle
        .update(cx, |view, _, cx| view.set_metrics(GRID, cx))
        .unwrap();
    run(cx, handle, 32);
    assert_eq!(renders(cx, handle), drawn + 1);
}

/// The mark, sentence and detail as one surface lays them out (tops and heights), and the window's scale.
fn laid_out(cx: &mut TestAppContext, grid: bool) -> (Vec<(Pixels, Pixels)>, f32) {
    let catalog = english();
    let handle = open(cx);
    cx.simulate_window_resize(handle.into(), size(px(908.), px(619.)));
    handle
        .update(cx, |view, _, cx| {
            view.set(
                Some(Facts::failed(
                    "scan",
                    "Permission denied",
                    0o40000,
                    &catalog,
                )),
                cx,
            );
            if grid {
                view.set_metrics(GRID, cx);
            }
        })
        .unwrap();
    frame(cx, handle);
    frame(cx, handle);
    let scale = handle
        .update(cx, |_, window, _| window.scale_factor())
        .unwrap();
    let parts = bounds(
        cx,
        handle,
        &["fb1-state-mark", "fb1-state-sentence", "fb1-state-detail"],
    )
    .iter()
    .map(|part| (part.origin.y, part.size.height))
    .collect();
    (parts, scale)
}

/// The column's centre, within the half device pixel its snapped origin may move it.
pub(super) fn centred(drawn: Pixels, centre: f32, scale: f32) {
    let off = (f32::from(drawn) - centre).abs();
    assert!(
        off <= 0.5 / scale,
        "centre {drawn:?} is {off} px from {centre}"
    );
}

/// A line box as the window draws it: the board's height snapped to the nearest device pixel.
pub(super) fn snapped(height: f32, scale: f32) -> Pixels {
    px((height * scale).round() / scale)
}

/// locked board: mark 40, 10px, the sentence on 14px x 1.2, 12px down to the detail on 12px x 1.2, each line box on device pixels.
#[gpui::test]
fn the_drawn_list_state_has_the_locked_boards_spacing(cx: &mut TestAppContext) {
    let (parts, scale) = laid_out(cx, false);
    let (mark, sentence, detail) = (parts[0], parts[1], parts[2]);
    assert_eq!(mark.1, px(40.));
    assert_eq!(sentence.0 - (mark.0 + mark.1), px(10.));
    assert_eq!(sentence.1, snapped(14. * 1.2, scale));
    assert_eq!(detail.0 - (sentence.0 + sentence.1), px(10. + 2.));
    assert_eq!(detail.1, snapped(12. * 1.2, scale));
    centred(
        mark.0 + (detail.0 + detail.1 - mark.0) / 2.,
        619. / 2.,
        scale,
    );
}

/// grid-locked board: 3px down, mark 40 with 1px under it and a 9px gap, the sentence on 1.32, 9px, the detail on a 22px line.
#[gpui::test]
fn the_drawn_grid_state_has_the_grid_locked_boards_spacing(cx: &mut TestAppContext) {
    let (parts, scale) = laid_out(cx, true);
    let (mark, sentence, detail) = (parts[0], parts[1], parts[2]);
    assert_eq!(mark.1, px(40.));
    assert_eq!(sentence.0 - (mark.0 + mark.1), px(1. + 9.));
    assert_eq!(sentence.1, snapped(14. * 1.32, scale));
    assert_eq!(detail.0 - (sentence.0 + sentence.1), px(9.));
    assert_eq!(detail.1, px(22.));
    centred(
        mark.0 + (detail.0 + detail.1 - mark.0) / 2.,
        (619. + 3.) / 2.,
        scale,
    );
}
