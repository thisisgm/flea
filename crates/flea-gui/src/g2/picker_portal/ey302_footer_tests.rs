//! EY-302: the chooser's status bar at the width the defect was found, as PickerWindow.qml lays it out.
use super::ey313_tests::{draw, three};
use super::tests::fixture::Fixture;
use gpui::{px, size, TestAppContext};

const UNREACHABLE: &str = "/home/flea-sandbox/fn.6fbe43ug/arms/pd.sa-d3.path-unreachable-wording.current.gpui/fixture/work/A/missing-sa-d3 does not exist or cannot be reached.";

// The status bar, its message and its hints, as drawn.
fn footer(f: &mut Fixture) -> [gpui::Bounds<gpui::Pixels>; 3] {
    draw(f);
    draw(f);
    [
        "picker-status-band",
        "picker-status-message",
        "picker-status-hints",
    ]
    .map(|part| {
        f.cx.debug_bounds(part)
            .unwrap_or_else(|| panic!("{part} is drawn"))
    })
}

// PickerWindow.qml's footer: rowPaddingX at each end, spacing.gap between message and hints.
const BAR_PADDING: f32 = 14.;
const STATUS_GAP: f32 = 9.;
// Layout lands on whole pixels, so a half-bar split may round by one.
const ROUNDING: f32 = 1.;
// The EY-302 scene's window, where the sentence ran into the hints.
const SCENE_WIDTH: f32 = 875.;
const SCENE_HEIGHT: f32 = 600.;
// "enter open · esc cancel" at caption size is wider than this; a vanished hint is not.
const HINTS_FLOOR: f32 = 50.;
// One letter at caption size is narrower than this; a message stretched over the bar is not.
const ONE_LETTER_CEILING: f32 = 20.;
// Hints this many times over run past the whole bar, so they must give way.
const HINT_REPEATS: usize = 12;

fn near(a: gpui::Pixels, b: f32) -> bool {
    (f32::from(a) - b).abs() <= ROUNDING
}

#[gpui::test]
fn ey302_a_long_message_keeps_a_gap_before_hints_of_their_own_width(cx: &mut TestAppContext) {
    let mut f = three(cx);
    f.cx.simulate_resize(size(px(SCENE_WIDTH), px(SCENE_HEIGHT)));
    let [_, _, quiet] = footer(&mut f);
    assert!(
        quiet.size.width > px(HINTS_FLOOR),
        "the hints draw their own width: {quiet:?}"
    );
    f.update(|picker, cx| picker.say(UNREACHABLE, true, cx));
    let [bar, message, hints] = footer(&mut f);
    assert_eq!(
        hints.size.width, quiet.size.width,
        "short hints keep their own width beside a long message"
    );
    assert_eq!(
        hints.left() - message.right(),
        px(STATUS_GAP),
        "PickerWindow.qml keeps Theme.spacing.gap between the message and the hints"
    );
    assert_eq!(
        bar.right() - hints.right(),
        px(BAR_PADDING),
        "the hints end on the bar's padding"
    );
    assert_eq!(
        message.left(),
        bar.left() + px(BAR_PADDING),
        "the message keeps the rest of the bar"
    );
}

#[gpui::test]
fn ey302_long_hints_give_way_until_the_message_holds_half_the_bar(cx: &mut TestAppContext) {
    let mut f = three(cx);
    f.cx.simulate_resize(size(px(SCENE_WIDTH), px(SCENE_HEIGHT)));
    f.update(|picker, _| {
        picker.labels.hints = "enter open · esc cancel · ".repeat(HINT_REPEATS).into()
    });
    f.update(|picker, cx| picker.say(UNREACHABLE, true, cx));
    let [bar, message, hints] = footer(&mut f);
    let width = f32::from(bar.size.width);
    assert!(
        near(message.size.width, width / 2.),
        "a long message keeps half the bar: {message:?} of {bar:?}"
    );
    assert!(
        near(
            hints.size.width,
            width - 2. * BAR_PADDING - STATUS_GAP - width / 2.
        ),
        "long hints take the rest: {hints:?}"
    );
    f.update(|picker, cx| picker.say("x", false, cx));
    let [bar, message, hints] = footer(&mut f);
    assert!(
        message.size.width < px(ONE_LETTER_CEILING)
            && hints.left() - message.right() == px(STATUS_GAP),
        "a short message leaves the long hints the room: {message:?} {hints:?}"
    );
    assert_eq!(bar.right() - hints.right(), px(BAR_PADDING));
}
