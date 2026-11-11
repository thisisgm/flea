use super::*;

#[test]
fn views_read_their_names_and_flip() {
    assert_eq!(View::from_name("source"), View::Source);
    assert_eq!(View::from_name("rendered"), View::Rendered);
    assert_eq!(View::from_name("anything"), View::Rendered);
    assert_eq!(View::Source.name(), "source");
    assert_eq!(View::Rendered.other(), View::Source);
    assert_eq!(View::default(), View::Rendered);
}

#[test]
fn keys_flip_the_view_wrap_walk_close_and_scroll() {
    assert_eq!(
        action("markdownView", View::Rendered, false),
        Effect::View(View::Source)
    );
    assert_eq!(
        action("markdownView", View::Source, false),
        Effect::View(View::Rendered)
    );
    assert_eq!(action("toggleWrap", View::Source, false), Effect::Wrap);
    assert_eq!(
        action("focusNext", View::Rendered, false),
        Effect::CloseFocus(true)
    );
    assert_eq!(
        action("focusPrevious", View::Rendered, true),
        Effect::CloseFocus(false)
    );
    assert_eq!(action("open", View::Rendered, true), Effect::Close);
    assert_eq!(action("open", View::Rendered, false), Effect::None);
    assert_eq!(
        action("cursorDown", View::Rendered, false),
        Effect::Scroll(1)
    );
    assert_eq!(action("cursorUp", View::Source, false), Effect::Scroll(-1));
    assert_eq!(action("delete", View::Rendered, false), Effect::None);
}

#[test]
fn a_stroke_goes_sideways_when_its_sideways_travel_wins_or_shift_turns_it() {
    assert!(sideways(10.0, 2.0, false, false));
    assert!(!sideways(2.0, 10.0, false, false));
    assert!(!sideways(5.0, 5.0, false, false));
    assert!(sideways(0.0, 10.0, true, false));
    assert_eq!(across(0.0, 10.0, true), 10.0);
    assert_eq!(across(3.0, 10.0, true), 3.0);
    assert!(!sideways(10.0, 0.0, false, true));
    assert!(!sideways(0.0, 0.0, true, false));
}
