//! The ink check fails on each board-state breach, so a refactor cannot make it vacuous.
use super::frame_gate::{assert_inks, theme, DISABLED_OPACITY};
use gpui::{Edges, Hsla, Quad, ScaledPixels};

// A control sits under frame_gate.rs's card thresholds and a card above them.
const CONTROL: (f32, f32) = (40., 20.);
const CARD: (f32, f32) = (300., 100.);

fn frame((width, height): (f32, f32), ink: Hsla) -> Quad {
    let mut quad = Quad {
        border_color: ink,
        border_widths: Edges::all(ScaledPixels(1.)),
        ..Quad::default()
    };
    quad.bounds.size.width = ScaledPixels(width);
    quad.bounds.size.height = ScaledPixels(height);
    quad
}

#[test]
fn vf_frames_inks_listed_surface_matches_its_census_row() {
    let theme = theme();
    let quads = [
        frame(CONTROL, theme.foreground),
        frame(CARD, theme.muted_frame),
    ];
    assert_inks("collision", &quads, 1., theme);
}

#[test]
#[should_panic(expected = "board-state frames differ")]
fn vf_frames_inks_listed_surface_missing_its_state_fails() {
    let theme = theme();
    assert_inks("collision", &[frame(CARD, theme.muted_frame)], 1., theme);
}

#[test]
#[should_panic(expected = "board-state frames differ")]
fn vf_frames_inks_unlisted_surface_in_a_board_state_fails() {
    let theme = theme();
    assert_inks("unlisted", &[frame(CONTROL, theme.accent_frame)], 1., theme);
}

#[test]
#[should_panic(expected = "neither full nor disabled")]
fn vf_frames_inks_other_opacity_fails() {
    let theme = theme();
    let mut ink = theme.muted_frame;
    ink.a = 0.3;
    assert_inks("unlisted", &[frame(CONTROL, ink)], 1., theme);
}

#[test]
#[should_panic(expected = "has no HIG role")]
fn vf_frames_inks_disabled_accent_fails() {
    let theme = theme();
    let ink = Hsla {
        a: DISABLED_OPACITY,
        ..theme.accent_frame
    };
    assert_inks("unlisted", &[frame(CONTROL, ink)], 1., theme);
}

#[test]
#[should_panic(expected = "card frame must use muted_frame")]
fn vf_frames_inks_card_in_foreground_fails() {
    let theme = theme();
    assert_inks("unlisted", &[frame(CARD, theme.foreground)], 1., theme);
}
