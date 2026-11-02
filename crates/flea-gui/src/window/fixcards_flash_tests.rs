//! EY-519: after Tab hands the flash pane's keys to the rail, Escape still closes the flash card, as v0.3.12 does.
use super::*;

#[gpui::test]
fn fixcards_ey519_escape_after_tab_closes_the_flash_card(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, "list", cx);
    for _ in 0..3 {
        frame(cx);
    }
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let effects = shell
                .operations
                .archive
                .flash
                .open_with_image("/files/archive-iso.iso");
            shell.archive_effects(0, effects, window, cx);
            window.focus(&shell.focus, cx);
            shell.keyboard.context = flea::keys::Context::Listing;
        })
    });
    frame(cx);
    assert!(shell.read_with(cx, |shell, _| shell.operations.archive.flash.shown()));
    cx.simulate_keystrokes("tab");
    frame(cx);
    cx.update(|window, cx| {
        shell.read_with(cx, |shell, cx| {
            assert!(
                shell.dual_focus.rail.contains_focused(window, cx),
                "Tab hands the flash pane's keys to the rail"
            );
        })
    });
    cx.simulate_keystrokes("escape");
    frame(cx);
    assert!(
        !shell.read_with(cx, |shell, _| shell.operations.archive.flash.shown()),
        "Escape from the rail closes the flash card covering its pane"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// The rail's Escape hands the keys to the focused pane, so a flash covering the other dual pane stays open.
#[gpui::test]
fn fixcards_ey519_escape_from_the_rail_leaves_the_other_panes_flash_open(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, "dual", cx);
    for _ in 0..3 {
        frame(cx);
    }
    shell.update_in(cx, |shell, window, cx| {
        let effects = shell
            .operations
            .archive
            .flash
            .open_with_image("/files/archive-iso.iso");
        shell.archive_effects(0, effects, window, cx);
        shell.focus_side = 1;
        shell.rail_focus(true, cx);
        window.focus(&shell.dual_focus.rail, cx);
        shell.keyboard.context = flea::keys::Context::Rail;
    });
    frame(cx);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.operations.archive.flash_side),
        0,
        "the flash covers the left pane"
    );
    cx.update(|window, cx| {
        shell.read_with(cx, |shell, cx| {
            assert!(shell.flash_covers(0), "the flash covers the left pane");
            assert!(
                shell.dual_focus.rail.contains_focused(window, cx),
                "the rail holds the keys"
            );
            assert_eq!(shell.keyboard.context, flea::keys::Context::Rail);
        })
    });
    cx.simulate_keystrokes("escape");
    frame(cx);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.keyboard.context),
        flea::keys::Context::Listing,
        "the rail's Escape ran and handed the keys back to the pane"
    );
    assert!(
        shell.read_with(cx, |shell, _| shell.operations.archive.flash.shown()),
        "Escape returns the keys to the right pane and leaves the left pane's flash open"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
