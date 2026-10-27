//! Modifier-only events relabel the drawn run button before a pointer press chooses its action.
use super::{
    answering, card_buttons, card_title, quit, record_run, run_launched, set_cursor, shell,
    SCRIPT_ASK,
};
use crate::state::Shell;
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};

fn open_script(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = shell(cx);
    set_cursor(&shell, cx, 1);
    answering(Some(4), SCRIPT_ASK);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    shell.update(cx, |shell, _| {
        shell
            .run_card
            .as_mut()
            .expect("the opener's run ask opened its card")
            .launch = record_run;
    });
    (shell, cx)
}

fn launch_label(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    card_buttons(shell, cx)
        .expect("the card is open")
        .pop()
        .expect("the launch button exists")
}

#[gpui::test]
fn releasing_shift_without_a_key_relabels_before_a_click(cx: &mut TestAppContext) {
    let (shell, cx) = open_script(cx);
    cx.simulate_modifiers_change(Modifiers::shift());
    cx.simulate_keystrokes("shift-tab");
    cx.run_until_parked();
    assert_eq!(launch_label(&shell, cx), "Run");
    let held = cx.debug_bounds("op3-button-2").expect("Run is drawn");

    // The finding's exact release has no key-down between Shift+Tab and the pointer press.
    cx.simulate_modifiers_change(Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        launch_label(&shell, cx),
        "Run in Terminal",
        "Shift release relabels before the click"
    );
    let released = cx
        .debug_bounds("op3-button-2")
        .expect("Run in Terminal is drawn");
    assert!(
        released.size.width > held.size.width,
        "the drawn button repaints with its longer label"
    );
    cx.simulate_click(released.center(), Modifiers::none());
    cx.run_until_parked();
    let ran = run_launched();
    assert_eq!(ran.len(), 1);
    assert_eq!(ran[0][1], "--run-terminal");
    assert_eq!(ran[0][2], "/files/hello.sh");
    assert_eq!(card_title(&shell, cx), None);
    quit(&shell, cx);
}

#[gpui::test]
fn shift_alone_relabels_on_press_and_release_and_the_click_follows_it(cx: &mut TestAppContext) {
    let (shell, cx) = open_script(cx);
    assert_eq!(launch_label(&shell, cx), "Run in Terminal");
    let unshifted = cx
        .debug_bounds("op3-button-2")
        .expect("Run in Terminal is drawn");
    cx.simulate_modifiers_change(Modifiers::shift());
    cx.run_until_parked();
    assert_eq!(
        launch_label(&shell, cx),
        "Run",
        "Shift alone relabels without another key"
    );
    let shifted = cx.debug_bounds("op3-button-2").expect("Run is drawn");
    assert!(
        shifted.size.width < unshifted.size.width,
        "the drawn button repaints with its shorter label"
    );
    cx.simulate_modifiers_change(Modifiers::none());
    cx.run_until_parked();
    assert_eq!(launch_label(&shell, cx), "Run in Terminal");
    cx.simulate_modifiers_change(Modifiers::shift());
    cx.run_until_parked();
    assert_eq!(launch_label(&shell, cx), "Run");
    let button = cx.debug_bounds("op3-button-2").expect("Run is drawn");
    cx.simulate_click(button.center(), Modifiers::shift());
    cx.run_until_parked();
    let ran = run_launched();
    assert_eq!(ran.len(), 1);
    assert_eq!(ran[0][1], "--run");
    assert_eq!(ran[0][2], "/files/hello.sh");
    assert_eq!(card_title(&shell, cx), None);
    quit(&shell, cx);
}

// The opener reads the window's Shift when its ask returns, so a Shift held before the card opens labels Run.
#[gpui::test]
fn shift_held_before_the_card_opens_labels_run_and_the_click_runs(cx: &mut TestAppContext) {
    let (shell, cx) = shell(cx);
    cx.simulate_modifiers_change(Modifiers::shift());
    cx.run_until_parked();
    cx.update(|window, cx| {
        let shift = window.modifiers().shift;
        shell.update(cx, |shell, cx| {
            shell.open_run_card(0, "/files/hello.sh".into(), SCRIPT_ASK.into(), shift, cx)
        })
    });
    cx.run_until_parked();
    shell.update(cx, |shell, _| {
        shell
            .run_card
            .as_mut()
            .expect("the run ask opened its card")
            .launch = record_run;
    });
    assert_eq!(
        launch_label(&shell, cx),
        "Run",
        "a Shift held before the card opened labels its button"
    );
    let button = cx.debug_bounds("op3-button-2").expect("Run is drawn");
    cx.simulate_click(button.center(), Modifiers::shift());
    cx.run_until_parked();
    let ran = run_launched();
    assert_eq!(ran.len(), 1);
    assert_eq!(
        ran[0][1], "--run",
        "the click runs what the drawn button says"
    );
    assert_eq!(ran[0][2], "/files/hello.sh");
    quit(&shell, cx);
}
