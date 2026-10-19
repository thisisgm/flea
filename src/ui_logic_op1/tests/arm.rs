// Ported from tests/arm-prompt.qml: the dd arm, the prompt it draws on the status bar, and the Trash view's arm.
use crate::ui_logic_op1::testcat::Catalog;
use crate::ui_logic_op1::trash::{self, Arm, Press, Transient};

const NOTICE: &str = "Renamed to notes.txt";
// Board wording for 0.4.0: "or press" where the oracle pins "or Delete on its own".
const PROMPT: &str = "Press d again to move it to Trash, or press Delete on its own.";
const REVIEW: &str = "Press d again to review permanent deletion, or press Delete on its own.";
const T0: u64 = 10_000;
// Oracle clockLeftMs: how much of the arm's clock is left when the expiry leg arms.
const LEFT_MS: u64 = 100;
// Oracle clockGraceMs: how long past the clock the harness waits before calling the arm stuck.
const GRACE_MS: u64 = 1000;

#[test]
fn disarm_leg_replaces_notice_and_clears_prompt_at_once() {
    let cat = Catalog::english();
    let prompt = trash::trash_prompt(&cat.i18n, "default", 1);
    let mut arm = Arm::default();
    // oracle tests/arm-prompt.qml:81
    assert_eq!(trash::transient(&[], NOTICE, &prompt, false).text, NOTICE);
    arm.press(T0);
    // oracle tests/arm-prompt.qml:83
    assert_eq!(
        trash::transient(&[], NOTICE, &prompt, arm.live(T0)).text,
        PROMPT
    );
    // oracle tests/arm-prompt.qml:84
    assert_eq!(
        trash::transient(&[], NOTICE, &prompt, arm.live(T0)),
        Transient {
            text: PROMPT.into(),
            error: false,
            detail: String::new()
        }
    );
    arm.disarm();
    // oracle tests/arm-prompt.qml:86
    assert_eq!(
        (
            trash::transient(&[], NOTICE, &prompt, arm.live(T0)).text == PROMPT,
            arm == Arm::default()
        ),
        (false, true)
    );
    // oracle tests/arm-prompt.qml:87
    assert_eq!(trash::transient(&[], "", &prompt, arm.live(T0)).text, "");
}

#[test]
fn clock_leg_shows_prompt_then_clock_ends_it_no_earlier_than_arm_ms() {
    let cat = Catalog::english();
    let prompt = trash::trash_prompt(&cat.i18n, "default", 1);
    let mut arm = Arm::default();
    arm.press(T0);
    let now = T0 + trash::ARM_MS - LEFT_MS;
    // oracle tests/arm-prompt.qml:94
    assert_eq!(
        (
            arm.left(now),
            trash::transient(&[], NOTICE, &prompt, arm.live(now)).text
        ),
        (LEFT_MS, PROMPT.to_string())
    );
    // oracle tests/arm-prompt.qml:95
    assert_eq!(
        trash::transient(&[], NOTICE, &prompt, arm.live(now)),
        Transient {
            text: PROMPT.into(),
            error: false,
            detail: String::new()
        }
    );
    let end = T0 + trash::ARM_MS;
    // oracle tests/arm-prompt.qml:99
    assert_eq!((arm.live(end - 1), arm.live(end)), (true, false));
    // oracle tests/arm-prompt.qml:100
    assert_eq!(trash::transient(&[], "", &prompt, arm.live(end)).text, "");
    // oracle tests/arm-prompt.qml:101 is owner stamp clearing, QML wiring with no pure counterpart; see port-arm.tsv.
}

#[test]
fn trash_view_first_d_arms_review_and_second_d_spends_it() {
    let cat = Catalog::english();
    let review = trash::review_prompt(&cat.i18n, "default");
    let mut arm = Arm::default();
    let first = arm.press(T0);
    // oracle tests/arm-prompt.qml:120
    assert_eq!(
        (first == Press::Armed && arm.stamp > 0, review == REVIEW),
        (true, true)
    );
    let second = arm.press(T0 + 100);
    // oracle tests/arm-prompt.qml:123
    assert_eq!((second, arm.stamp), (Press::Act, 0));
    // oracle tests/arm-prompt.qml:124
    assert_eq!(
        (
            trash::transient(&[], "", &review, arm.live(T0 + 100)).text,
            arm == Arm::default()
        ),
        (String::new(), true)
    );
    // oracle tests/arm-prompt.qml:125 is the view opening its review card, Trash view wiring; see port-arm.tsv.
}

#[test]
fn stuck_arm_is_not_live_once_its_clock_and_grace_pass() {
    let mut arm = Arm::default();
    arm.press(T0);
    // oracle tests/arm-prompt.qml:138
    assert!(!arm.live(T0 + trash::ARM_MS + GRACE_MS));
}
