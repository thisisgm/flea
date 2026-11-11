//! Same-update handoffs update future origins while queued requests keep their captured origin.
use super::*;

#[gpui::test]
fn gcomp39_real_rail_focus_handoff_preserves_queued_origins_invalid_targets_and_left_tab(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(false, cx);
    rows(
        &shell,
        vec![flea::ui_logic_rail_places::order::trash_entry()],
        cx,
    );
    let heard = Rc::new(RefCell::new(Vec::<Request>::new()));
    let sink = heard.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, request: &Request, _| {
            sink.borrow_mut().push(request.clone());
        })
    });
    let producer = producer(&shell, cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.view = crate::window::startup::View::Dual;
            shell.panes[1] = Some(crate::window::pane::PaneHost::new(
                "/right".into(),
                None,
                620.,
                &shell.locale,
            ));
            producer.update(cx, |producer, cx| producer.activate(0, cx));
            shell.focus_pane(1, cx);
            producer.update(cx, |producer, cx| producer.activate(0, cx));
            shell.focus_pane(9, cx);
            assert_eq!(
                shell.focus_side, 1,
                "an invalid focus target changed ownership"
            );
            producer.update(cx, |producer, cx| producer.key_action("menu", cx));
            window.focus(&shell.focus, cx);
            assert!(shell.dual_tab(true, window, cx));
            assert_eq!(
                shell.focus_side, 0,
                "Dual rail Tab must still belong to the left pane"
            );
            assert_eq!(shell.keyboard.context, flea::keys::Context::Rail);
            assert!(shell.dual_focus.rail.contains_focused(window, cx));
            producer.update(cx, |producer, cx| producer.activate(0, cx));
        });
    });
    cx.run_until_parked();
    frame(cx);
    let requests = heard.borrow();
    assert_eq!(
        requests
            .iter()
            .map(|request| (request.side, request.id))
            .collect::<Vec<_>>(),
        [(0, 1), (1, 2), (0, 4)],
        "focus handoff lagged a new request or rewrote a captured origin",
    );
    assert_eq!(requests[0].event, Event::Trash);
    assert_eq!(requests[1].event, Event::Trash);
    // PB-RAIL-2: the Trash row's menu is a handled order now (window/dispatch/rail_menu.rs), so it no longer reaches observers.
    assert_eq!(requests[2].event, Event::Trash);
}

#[gpui::test]
fn uish_misc_l_on_a_rail_row_opens_it_as_enter_does(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    rows(
        &shell,
        vec![flea::ui_logic_rail_places::order::trash_entry()],
        cx,
    );
    let heard = Rc::new(RefCell::new(Vec::<Request>::new()));
    let sink = heard.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, request: &Request, _| {
            sink.borrow_mut().push(request.clone());
        })
    });
    let producer = producer(&shell, cx);
    cx.update(|_, cx| {
        producer.update(cx, |producer, cx| {
            producer.key_action("cursorFirst", cx);
            producer.key_action("pageForward", cx);
        });
    });
    cx.run_until_parked();
    frame(cx);
    let requests = heard.borrow();
    assert_eq!(
        requests
            .iter()
            .map(|request| &request.event)
            .collect::<Vec<_>>(),
        [&Event::Trash],
        "l on the rail did not open the row under the cursor"
    );
}
