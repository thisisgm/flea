use super::carry::{Answer, Carry, LiftOut, Source};
use crate::i18n::I18n;

fn source(window: u64, side: usize, listing: u64) -> Source {
    Source {
        window,
        side,
        folder: "/a".into(),
        listing,
    }
}

fn riding(carry: &mut Carry, i18n: &I18n, paths: &[&str]) {
    assert_eq!(
        carry.lift(i18n, &[0, 1, 2], source(1, 0, 7), false),
        LiftOut::Ask(r#"{"c":"paths","rows":[0,1,2]}"#.into())
    );
    let paths = paths.iter().map(|path| (*path).to_owned()).collect();
    assert_eq!(carry.resolved(i18n, 1, 0, paths, 7), Answer::Rides);
}

#[test]
fn m9_g2rows_25_core_lift_resolves_rows_and_rides_until_cancel() {
    let i18n = crate::i18n::test_catalogs::english();
    let mut carry = Carry::new();
    assert_eq!(
        carry.lift(&i18n, &[], source(1, 0, 7), false),
        LiftOut::Nothing
    );
    assert!(!carry.active(), "no rows lift nothing");
    assert!(
        matches!(
            carry.lift(&i18n, &[3], source(1, 0, 7), true),
            LiftOut::Busy(_)
        ),
        "a held paths slot refuses"
    );
    assert!(!carry.active());
    riding(&mut carry, &i18n, &["/a/x", "/a/y", "/a/z"]);
    assert_eq!(carry.riding().unwrap().paths, ["/a/x", "/a/y", "/a/z"]);
    assert_eq!(
        carry.status(&i18n),
        Some((
            "Carrying 3 items".into(),
            "click where they go, esc cancels".into()
        ))
    );
    assert!(carry.cancel());
    assert!(carry.status(&i18n).is_none());
    assert!(!carry.cancel(), "a second esc has nothing to drop");
}

#[test]
fn m9_g2rows_25_core_answers_belong_to_their_asker_and_stale_listings_end() {
    let i18n = crate::i18n::test_catalogs::english();
    let mut carry = Carry::new();
    assert!(matches!(
        carry.lift(&i18n, &[0], source(1, 0, 7), false),
        LiftOut::Ask(_)
    ));
    assert_eq!(
        carry.resolved(&i18n, 1, 1, vec!["/b/q".into()], 7),
        Answer::Foreign,
        "the other side's answer is not the carry's"
    );
    assert_eq!(
        carry.resolved(&i18n, 2, 0, vec!["/b/q".into()], 7),
        Answer::Foreign,
        "another window's answer is not the carry's"
    );
    let Answer::Stale(stale) = carry.resolved(&i18n, 1, 0, vec!["/a/x".into()], 8) else {
        panic!("a moved listing is the stale answer");
    };
    assert_eq!(stale.text, "Selected items changed; copy or cut again.");
    assert!(!carry.active(), "a stale answer lifts nothing");
    assert!(matches!(
        carry.lift(&i18n, &[0], source(1, 0, 7), false),
        LiftOut::Ask(_)
    ));
    assert!(carry.failed(1, 0));
    assert!(!carry.active(), "a refused paths request ends the lift");
}

#[test]
fn m9_g2rows_25_core_only_the_accepted_serial_ends_the_ride() {
    let i18n = crate::i18n::test_catalogs::english();
    let mut carry = Carry::new();
    riding(&mut carry, &i18n, &["/a/x"]);
    let serial = carry.riding().unwrap().serial;
    assert!(
        !carry.landed(serial + 1),
        "an older or foreign serial ends nothing"
    );
    assert!(carry.riding().is_some());
    assert!(carry.landed(serial));
    assert!(!carry.active());
    assert!(
        !carry.landed(serial),
        "a landing reported twice ends nothing more"
    );
}

#[test]
fn m9_g2rows_25_core_lifecycle_ends_only_the_source_window_or_pane() {
    let i18n = crate::i18n::test_catalogs::english();
    let mut carry = Carry::new();
    riding(&mut carry, &i18n, &["/a/x"]);
    assert!(
        !carry.end_window(2),
        "another window closing keeps the carry"
    );
    assert!(
        !carry.end_source(1, 1),
        "the other pane ending keeps the carry"
    );
    assert!(carry.riding().is_some());
    assert!(carry.end_source(1, 0));
    riding(&mut carry, &i18n, &["/a/x"]);
    assert!(carry.end_window(1));
    assert!(!carry.active());
}

#[test]
fn m9_g2rows_25_core_window_end_keeps_an_owed_answer_claimed() {
    let i18n = crate::i18n::test_catalogs::english();
    let mut carry = Carry::new();
    assert!(matches!(
        carry.lift(&i18n, &[0], source(1, 0, 7), false),
        LiftOut::Ask(_)
    ));
    assert!(carry.end_window(1));
    assert!(!carry.active() && carry.pending(1, 0));
    assert!(matches!(
        carry.lift(&i18n, &[1], source(2, 1, 8), false),
        LiftOut::Busy(_)
    ));
    assert_eq!(
        carry.resolved(&i18n, 1, 0, vec!["/a/old".into()], 7),
        Answer::Drained
    );
    assert!(!carry.pending(1, 0));
}

#[test]
fn m9_g2rows_25_core_cancel_drains_before_any_new_resolver() {
    let i18n = crate::i18n::test_catalogs::english();
    let mut carry = Carry::new();
    assert!(matches!(
        carry.lift(&i18n, &[0], source(1, 0, 7), false),
        LiftOut::Ask(_)
    ));
    assert!(matches!(
        carry.lift(&i18n, &[1], source(2, 1, 8), false),
        LiftOut::Busy(_)
    ));
    assert!(carry.cancel());
    assert!(!carry.active());
    assert!(carry.pending(1, 0));
    assert!(matches!(
        carry.lift(&i18n, &[1], source(2, 1, 8), false),
        LiftOut::Busy(_)
    ));
    assert!(!carry.cancel());
    assert_eq!(
        carry.resolved(&i18n, 1, 0, vec!["/a/old".into()], 7),
        Answer::Drained,
        "a cancelled lift's own answer is consumed as the carry's, never handed on as a foreign one"
    );
    assert!(!carry.pending(1, 0));
    assert!(carry.riding().is_none());
    assert!(matches!(
        carry.lift(&i18n, &[1], source(2, 1, 8), false),
        LiftOut::Ask(_)
    ));
    assert!(carry.end_source(2, 1));
    assert!(!carry.pending(2, 1));
}

#[test]
fn m9_g2rows_25_core_refused_cancelled_ask_releases_its_slot() {
    let i18n = crate::i18n::test_catalogs::english();
    let mut carry = Carry::new();
    carry.lift(&i18n, &[0], source(1, 0, 7), false);
    carry.cancel();
    assert!(carry.pending(1, 0));
    assert!(carry.failed(1, 0));
    assert!(!carry.pending(1, 0));
    assert!(!carry.active());
}

#[test]
fn m9_g2rows_25_core_standing_hint_returns_after_error_and_keeps_newer_notice() {
    use crate::ui_logic_status_feedback::{messages::Messages, status::Hint};
    let catalog = crate::i18n::test_catalogs::english();
    let mut messages = Messages::new("vim");
    messages.say("Carrying 1 item", false, "", Hint::Carry, 0);
    let serial = messages.notice_serial();
    assert!(!messages.tick(10000).notice_cleared);
    let line = messages.line(&catalog, "");
    assert_eq!(line.primary, "Carrying 1 item");
    assert_eq!(line.secondary, " · click where they go, esc cancels");
    assert!(line.undo.is_empty());
    assert!(!line.error);
    messages.say("failure", true, "detail", Hint::None, 10001);
    assert_eq!(messages.line(&catalog, "").primary, "failure");
    assert!(!messages.tick(20000).notice_cleared);
    messages.dismiss(20001);
    assert_eq!(messages.line(&catalog, "").primary, "Carrying 1 item");
    messages.say("newer notice", false, "", Hint::None, 20002);
    messages.withdraw(serial, 20003);
    assert_eq!(messages.line(&catalog, "").primary, "newer notice");
    assert!(messages.tick(24002).notice_cleared);
}

#[test]
fn m9_g2rows_25_core_carry_key_is_gui_only_for_every_preset() {
    use crate::keys::{lookup, Context, Frontend, Input, Match, PRESETS, SHIFT};
    for preset in PRESETS {
        let input = Input {
            key: "x",
            text: "X",
            modifiers: SHIFT,
        };
        assert_eq!(
            lookup(preset, Context::Listing, Frontend::Gui, input),
            Match::Action("carry"),
            "{preset}"
        );
        assert_eq!(
            lookup(preset, Context::Listing, Frontend::Tui, input),
            Match::Unbound,
            "{preset}"
        );
        for context in [
            Context::Menu,
            Context::Editor,
            Context::Panel,
            Context::Preview,
            Context::Rail,
        ] {
            assert_ne!(
                lookup(preset, context, Frontend::Gui, input),
                Match::Action("carry"),
                "{preset} {context:?}"
            );
        }
    }
}
