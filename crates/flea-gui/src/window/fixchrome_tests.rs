//! EY-402 and EY-486 through the mounted toolbar: the ink each view mark paints, and the Back mark's scale while held.
use super::u07_tests::{open, read};
use crate::palette::Theme;
use crate::path::chrome::GLYPHS;
use gpui::{
    point, px, Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext,
};
use std::time::Duration;

/// Omarchy's catppuccin-latte colors.toml as /usr/share/omarchy/themes ships it.
pub(crate) const LATTE: &str = "mode = \"light\"\naccent = \"#1e66f5\"\nselection = \"#ccd0da\"\nmuted = \"#acb0be\"\nbackground = \"#eff1f5\"\ndark_background = \"#e3e4e8\"\ndarker_background = \"#d7d8dc\"\nlighter_background = \"#dce0e8\"\nforeground = \"#4c4f69\"\ndark_foreground = \"#9ca0b0\"\nlight_foreground = \"#5c5f77\"\nbright_foreground = \"#4c4f69\"\nred = \"#d20f39\"\nyellow = \"#df8e1d\"\ngreen = \"#40a02b\"\ncyan = \"#179299\"\nblue = \"#1e66f5\"\nmagenta = \"#ea76cb\"";
/// The sunset theme the ledger row was caught on.
pub(crate) const SUNSET: &str = "background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'\ndark_background='#150d0a'\nmuted='#68544b'";

/// The current mark in the foreground, the other three in the muted frame: lighter than it on a light ground, darker on a dark one.
pub(crate) fn assert_view_inks(
    name: &str,
    theme: &Theme,
    current: gpui::Hsla,
    others: &[gpui::Hsla],
) {
    assert_eq!(current, theme.foreground, "{name}: the current view mark");
    let light = theme.background.l > theme.foreground.l;
    for (index, ink) in others.iter().enumerate() {
        assert_eq!(*ink, theme.muted_frame, "{name}: view mark {index}");
        assert!(
            if light { ink.l > current.l } else { ink.l < current.l },
            "{name}: view mark {index} reads quieter than the current one ({ink:?} against {current:?})"
        );
    }
}

/// ui/js/Buttons.js:21: a held chrome mark draws at 96 percent, written out so a neutered press constant goes red.
const HELD_SCALE: f32 = 0.96;

/// The toolbar control drawing `glyph`.
fn control(glyph: &str) -> usize {
    GLYPHS
        .iter()
        .position(|name| *name == glyph)
        .expect("a toolbar glyph")
}

/// Two frames: a view notified while the first one draws repaints in the next, as it does on screen.
fn drawn(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
    }
}

#[gpui::test]
fn fixchrome_ey402_toolbar_view_marks_rest_with_list_current(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    for (name, theme) in [
        ("current", Theme::default()),
        ("catppuccin-latte", Theme::from_text(LATTE)),
        ("sunset", Theme::from_text(SUNSET)),
    ] {
        cx.update(|_, cx| cx.set_global(theme));
        drawn(cx);
        let chrome = shell.read_with(cx, |shell, _| shell.chrome.view.clone().unwrap());
        let inks = chrome.read_with(cx, |view, _| {
            assert_eq!(view.inputs.view, super::startup::View::List, "{name}");
            let frame = view.frame.borrow();
            ["list", "columns", "grid", "dual"].map(|glyph| {
                frame.controls[control(glyph)]
                    .mark_ink
                    .expect("view mark painted")
            })
        });
        assert_view_inks(name, &theme, inks[0], &inks[1..]);
    }
}

/// Back's drawn centre in window pixels.
fn back_centre(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) -> Point<Pixels> {
    // chromeButtonCentre replies "x y" in whole window pixels, e.g. "57 28".
    let centre = read(shell, cx, "chromeButtonCentre", &["arrow-left"]);
    let mut values = centre.split_whitespace().map(|v| v.parse::<f32>().unwrap());
    point(px(values.next().unwrap()), px(values.next().unwrap()))
}

/// One frame as the screen draws it: only views a notify dirtied render again.
fn framed(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

/// The drawn Back mark's width at rest, held 0.4 s and 0.4 s after release, and whether Back read as pressed while held.
fn held_back(cx: &mut TestAppContext, reduced: bool) -> (f32, f32, f32, bool) {
    let (shell, cx) = open(cx);
    cx.update(|_, cx| cx.set_reduce_motion(reduced));
    shell.update(cx, |shell, cx| {
        let history = &mut shell.panes[0].as_mut().unwrap().nav.history;
        history
            .back
            .push(crate::navigation::history::Entry::new("/older", "", 0));
        cx.notify();
    });
    drawn(cx);
    let at = back_centre(&shell, cx);
    let chrome = shell.read_with(cx, |shell, _| shell.chrome.view.clone().unwrap());
    // The release activates Back, so only the rest and held frames are asked to show it live.
    let width = |cx: &mut VisualTestContext, live: bool| {
        chrome.read_with(cx, |view, _| {
            let back = view.frame.borrow().controls[control("arrow-left")];
            assert!(!live || back.enabled, "Back is live with history");
            f32::from(back.mark_bounds.expect("Back painted").size.width)
        })
    };
    let rest = width(cx, true);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.executor().advance_clock(Duration::from_millis(400));
    drawn(cx);
    let held = width(cx, true);
    let pressed = chrome.read_with(cx, |view, _| {
        view.frame.borrow().controls[control("arrow-left")].pressed
    });
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.executor().advance_clock(Duration::from_millis(400));
    drawn(cx);
    (rest, held, width(cx, false), pressed)
}

#[gpui::test]
fn fixchrome_ey486_a_held_back_mark_draws_inset_to_ninety_six_percent(cx: &mut TestAppContext) {
    let (rest, held, released, pressed) = held_back(cx, false);
    assert!(pressed, "Back reads as pressed while held");
    assert!(
        (held - rest * HELD_SCALE).abs() < 0.01,
        "held Back draws {held} against rest {rest}"
    );
    assert!(
        (released - rest).abs() < 0.01,
        "released Back draws {released}"
    );
}

#[gpui::test]
fn fixchrome_ey486_reduced_motion_holds_back_at_full_size(cx: &mut TestAppContext) {
    let (rest, held, _, pressed) = held_back(cx, true);
    assert!(
        pressed,
        "the press registered, so only the scale is held back"
    );
    assert_eq!(held, rest);
}

/// EY-486: with unforced frames, the sprite paint_svg draws for a held Back shrinks in device pixels.
#[gpui::test]
fn fixchrome_ey486_a_held_back_sprite_draws_inset_in_device_pixels(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    drawn(cx);
    let at = back_centre(&shell, cx);
    let chrome = shell.read_with(cx, |shell, _| shell.chrome.view.clone().unwrap());
    let back = |cx: &mut VisualTestContext| {
        chrome.read_with(cx, |view, _| {
            let back = view.frame.borrow().controls[control("arrow-left")];
            (
                back.enabled,
                f32::from(back.mark_bounds.expect("Back painted").size.width),
            )
        })
    };
    let (_, rest) = back(cx);
    let rest_drawn = chrome.read_with(cx, |view, _| {
        view.frame.borrow().controls[control("arrow-left")]
            .mark_drawn
            .expect("Back painted")
    });
    shell.update(cx, |shell, cx| {
        let history = &mut shell.panes[0].as_mut().unwrap().nav.history;
        history
            .back
            .push(crate::navigation::history::Entry::new("/older", "", 0));
        cx.notify();
    });
    framed(cx);
    let (live, _) = back(cx);
    assert!(
        live,
        "Back is live in the first frame after it gains history"
    );
    cx.simulate_mouse_move(at, None, Modifiers::none());
    framed(cx);
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    for _ in 0..4 {
        cx.executor().advance_clock(Duration::from_millis(100));
        framed(cx);
    }
    let (_, held) = back(cx);
    assert!(
        (held - rest * HELD_SCALE).abs() < 0.01,
        "held Back draws {held} against rest {rest}"
    );
    let drawn = chrome.read_with(cx, |view, _| {
        view.frame.borrow().controls[control("arrow-left")]
            .mark_drawn
            .expect("Back painted")
    });
    assert!(
        (drawn - rest_drawn * HELD_SCALE).abs() < 0.01,
        "held Back fills {drawn} device px against {rest_drawn} at rest"
    );
}
