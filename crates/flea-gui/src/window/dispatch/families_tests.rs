//! The listing and window families run their orders on the production Shell, and an order nobody ran is still emitted.
use super::{
    test_shell::{land, open, row},
    Order,
};
use crate::window::input::ActionEvent;
use flea::keys::Context;
use gpui::{KeyDownEvent, Keystroke, Modifiers, TestAppContext};
use std::{cell::RefCell, rc::Rc};

#[gpui::test]
fn slash_add_network_and_an_unclaimed_network_key_take_their_families(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    let rows = vec![row("alpha", true), row("notes.txt", false)];
    land(&shell, cx, "/files", rows, 1);
    let emitted = Rc::new(RefCell::new(Vec::new()));
    let sink = emitted.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, event: &ActionEvent, _| {
            sink.borrow_mut().push(event.action)
        })
    });
    cx.simulate_event(KeyDownEvent {
        keystroke: Keystroke {
            key: "/".into(),
            key_char: Some("/".into()),
            modifiers: Modifiers::none(),
            is_keypad: false,
            us_key: None,
        },
        is_held: false,
        prefer_character_input: false,
    });
    cx.run_until_parked();
    let up = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().filter.owner.up()
    });
    assert!(up, "/ opens the pane's filter through the listing family");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let add = ActionEvent::bare("addNetwork", Context::Rail);
            shell.dispatch(Order::Action(add), window, cx)
        })
    });
    cx.run_until_parked();
    assert!(
        shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)),
        "addNetwork opens Add a Network Share through the window family"
    );
    assert_eq!(*emitted.borrow(), Vec::<&str>::new(), "both orders ran");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let open = ActionEvent::bare("open", Context::Menu);
            shell.dispatch(Order::Network(open), window, cx)
        })
    });
    assert_eq!(
        *emitted.borrow(),
        ["open"],
        "a network key with no menu or browser open runs nowhere and is emitted"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
    cx.update(|window, _| window.remove_window());
    cx.run_until_parked();
}
