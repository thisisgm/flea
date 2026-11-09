// #331: Quick Look's ground consumes every pointer event; only a left click outside the card closes it.
use super::ground::*;

const LEFT: Pointer = Pointer::Press { button: Button::Left, clicks: 1 };
const LEFT_UP: Pointer = Pointer::Release { button: Button::Left };

#[test]
fn a_left_click_outside_the_card_closes_and_inside_does_not() {
    let mut g = Ground::default();
    assert_eq!(g.handle(LEFT, false, true), Effect::Swallow);
    assert_eq!(g.handle(LEFT_UP, false, true), Effect::Close);
    assert_eq!(g.handle(LEFT, true, true), Effect::Swallow);
    assert_eq!(g.handle(LEFT_UP, true, true), Effect::Swallow);
    // a press inside released outside is a drag out of the card, not a click on the ground
    assert_eq!(g.handle(LEFT, true, true), Effect::Swallow);
    assert_eq!(g.handle(LEFT_UP, false, true), Effect::Swallow);
}

#[test]
fn right_middle_double_wheel_and_pinch_are_only_swallowed() {
    let mut g = Ground::default();
    for button in [Button::Right, Button::Middle, Button::Other] {
        for inside in [true, false] {
            assert_eq!(g.handle(Pointer::Press { button, clicks: 1 }, inside, true), Effect::Swallow);
            assert_eq!(g.handle(Pointer::Release { button }, inside, true), Effect::Swallow);
        }
    }
    for inside in [true, false] {
        assert_eq!(g.handle(Pointer::Press { button: Button::Left, clicks: 2 }, inside, true), Effect::Swallow);
        assert_eq!(g.handle(LEFT_UP, inside, true), Effect::Swallow);
        assert_eq!(g.handle(Pointer::Wheel, inside, true), Effect::Swallow);
        assert_eq!(g.handle(Pointer::Pinch, inside, true), Effect::Swallow);
        assert_eq!(g.handle(Pointer::Move, inside, true), Effect::RevealStrip);
    }
}

#[test]
fn the_hold_shield_reveals_and_keeps_its_release() {
    let mut g = Ground::default();
    assert_eq!(g.handle(Pointer::Wheel, true, false), Effect::Reveal);
    assert_eq!(g.handle(LEFT, false, false), Effect::Reveal);
    // revealed by the press, the release still clicks nothing, so no click closes a card that never drew
    assert_eq!(g.handle(LEFT_UP, false, true), Effect::Swallow);
    assert_eq!(g.handle(LEFT, false, true), Effect::Swallow);
    assert_eq!(g.handle(LEFT_UP, false, true), Effect::Close);
}

#[test]
fn a_reset_ground_keeps_no_shield() {
    let mut g = Ground::default();
    g.handle(LEFT, false, false);
    g.reset();
    assert_eq!(g, Ground::default());
}
