use crate::ui_logic_op1::card::*;

// Title row height used by every fixture in this file.
const TITLE: f32 = 20.0;
// Body height a card wants at rest, far taller than any short window leaves room for.
const WANTED: f32 = 300.0;
// Share of the wanted body that the short window cuts, as the oracle bodyCut.
const BODY_CUT: f32 = 0.6;

// Frame, title, hairline, buttons and every gap with no body: the height a card never gives up.
fn chrome() -> f32 {
    2.0 * BORDER
        + PADDING
        + TITLE
        + GAP
        + RULE
        + GAP
        + GAP
        + BUTTONS_MARGIN
        + BUTTON_HEIGHT
        + PADDING
}

// A window the wanted body overflows, sized the way the oracle sizes its short window.
fn short_window() -> f32 {
    chrome() + WANTED - WANTED * BODY_CUT + 2.0 * PADDING
}

// The height a frame gives to its pinned title and buttons, leaving out the body.
fn pinned(f: Frame) -> f32 {
    f.height - f.body
}

#[test]
fn short_window_leaves_body_taller_than_its_room() {
    // oracle tests/dialog-board.qml:392
    let f = frame(TITLE, WANTED, short_window());
    // oracle tests/dialog-board.qml:392
    assert!(f.scrolls);
    // oracle tests/dialog-board.qml:392
    assert!(f.body < WANTED);
    // Negative: a roomy window fits the same body without scrolling.
    // oracle tests/dialog-board.qml:392
    assert!(!frame(TITLE, WANTED, 720.0).scrolls);
}

#[test]
fn scrolled_body_leaves_title_and_buttons_where_they_were() {
    // oracle tests/dialog-board.qml:404
    assert_eq!(pinned(frame(TITLE, WANTED, short_window())), chrome());
    // oracle tests/dialog-board.qml:404
    assert_eq!(
        pinned(frame(TITLE, 40.0, short_window())),
        pinned(frame(TITLE, WANTED, short_window()))
    );
    // Negative: a body scrolled to nothing still leaves the pinned chrome unchanged.
    // oracle tests/dialog-board.qml:404
    assert_eq!(pinned(frame(TITLE, 0.0, short_window())), chrome());
}

#[test]
fn scrolled_card_keeps_pinned_rows_inside_card() {
    // oracle tests/dialog-board.qml:405
    assert!(frame(TITLE, WANTED, short_window()).height + 2.0 * PADDING <= short_window());
    // Negative: a window shorter than the chrome cannot hold the pinned rows inside it.
    // oracle tests/dialog-board.qml:405
    assert!(frame(TITLE, WANTED, 100.0).height + 2.0 * PADDING > 100.0);
}

#[test]
fn half_done_transfer_fills_half_the_track() {
    let track = 200.0_f32;
    // oracle tests/dialog-board.qml:532
    assert_eq!(fill_width(track, 0.5) / track, 0.5);
    // Negative: a share past one never draws past the track.
    // oracle tests/dialog-board.qml:532
    assert_eq!(fill_width(track, 1.7), track);
    // Negative: an unset share draws no fill.
    // oracle tests/dialog-board.qml:532
    assert_eq!(fill_width(track, f64::NAN), 0.0);
}

// The DialogButton recipe in each state the boards and ui/js/Buttons.js state.
#[test]
fn a_secondary_button_rests_framed_and_takes_its_own_ink_under_the_pointer() {
    let rest = look(ButtonState::default());
    assert_eq!(
        (
            rest.frame,
            rest.frame_width,
            rest.label,
            rest.wash,
            rest.overlay,
            rest.alpha
        ),
        (
            Ink::MutedFrame,
            BORDER,
            Ink::Foreground,
            Ink::None,
            Ink::None,
            1.0
        )
    );
    let hover = look(ButtonState {
        hovered: true,
        ..Default::default()
    });
    assert_eq!(
        (hover.overlay, hover.overlay_alpha),
        (Ink::Foreground, HOVER_WASH)
    );
    let press = look(ButtonState {
        hovered: true,
        pressed: true,
        ..Default::default()
    });
    assert_eq!(
        (press.overlay, press.overlay_alpha, press.pressed),
        (Ink::Foreground, PRESS_WASH, true)
    );
    let focus = look(ButtonState {
        focused: true,
        ..Default::default()
    });
    assert_eq!(
        (focus.frame, focus.frame_width, focus.wash),
        (Ink::Foreground, BORDER, Ink::None)
    );
    // Negative: a hover never changes the frame, and a rest draws no pointer wash.
    assert_eq!(hover.frame, Ink::MutedFrame);
    assert_eq!(rest.overlay_alpha, 0.0);
}

#[test]
fn the_default_keeps_its_accent_wash_under_focus_hover_and_press() {
    let rest = look(ButtonState {
        primary: true,
        ..Default::default()
    });
    assert_eq!(
        (rest.frame, rest.wash, rest.wash_alpha),
        (Ink::AccentFrame, Ink::Accent, PRIMARY_WASH)
    );
    let focus = look(ButtonState {
        primary: true,
        focused: true,
        ..Default::default()
    });
    assert_eq!(
        (focus.frame, focus.wash, focus.wash_alpha),
        (Ink::Foreground, Ink::Accent, PRIMARY_WASH)
    );
    let hover = look(ButtonState {
        primary: true,
        hovered: true,
        ..Default::default()
    });
    assert_eq!(
        (hover.wash, hover.overlay, hover.overlay_alpha),
        (Ink::Accent, Ink::Foreground, HOVER_WASH)
    );
    // A theme whose accent frame is its foreground gets a ring two wide, so focus still shows.
    assert_eq!(
        look(ButtonState {
            primary: true,
            focused: true,
            rest_is_foreground: true,
            ..Default::default()
        })
        .frame_width,
        RING
    );
    // Negative: the thick ring is only for a focused default on such a theme.
    assert_eq!(
        look(ButtonState {
            focused: true,
            rest_is_foreground: true,
            ..Default::default()
        })
        .frame_width,
        BORDER
    );
    assert_eq!(
        look(ButtonState {
            primary: true,
            rest_is_foreground: true,
            ..Default::default()
        })
        .frame_width,
        BORDER
    );
}

#[test]
fn a_disabled_button_is_dimmed_and_takes_no_pointer() {
    let off = look(ButtonState {
        primary: true,
        disabled: true,
        hovered: true,
        pressed: true,
        focused: true,
        ..Default::default()
    });
    assert_eq!(
        (off.frame, off.wash, off.overlay, off.alpha, off.pressed),
        (Ink::MutedFrame, Ink::None, Ink::None, DISABLED_ALPHA, false)
    );
    // Negative: the label keeps its role, the 0.55 alone says it is unavailable, as .btn.dis draws it.
    assert_eq!(off.label, Ink::Foreground);
    assert_eq!(
        look(ButtonState {
            destructive: true,
            disabled: true,
            ..Default::default()
        })
        .label,
        Ink::ErrorOnSurface
    );
}

#[test]
fn a_press_insets_whole_pixels_along_the_curve() {
    assert_eq!(press_inset(30.0, 0.0), 0.0);
    assert_eq!(press_inset(30.0, 1.0), 1.0);
    assert_eq!(press_inset(80.0, 1.0), 2.0);
    // Negative: no inset is ever fractional, even part way.
    for step in 0..=10 {
        let inset = press_inset(97.0, step as f32 / 10.0);
        assert_eq!(inset, inset.round());
    }
}
