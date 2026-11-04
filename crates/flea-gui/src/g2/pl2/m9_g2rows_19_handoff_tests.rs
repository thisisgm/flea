//! The mounted shell starts no host discovery and forwards no nearby reply to the card.
use super::*;

#[gpui::test]
fn m9_g2rows_19_window_never_forwards_nearby_replies(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (backend, mut lines) = crate::backend::Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|_, cx| {
        let mut shell = crate::state::tests::shell_with(backend, cx);
        let mut join = crate::window::rail::mount(
            shell.locale.catalog.clone(),
            &shell.launch.settings.data,
            0,
            cx,
        );
        join.network_runner = recorder.clone();
        shell.slots.rail = Some(gpui::AnyView::from(join.host.clone()));
        shell.rail = Some(join);
        shell
    });
    cx.simulate_resize(gpui::size(px(1100.), px(700.)));
    frame(cx);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.network_add(0, window, cx)));
    frame(cx);
    let card = cx.update(|_, cx| {
        shell
            .read(cx)
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .card
            .clone()
    });
    let sent: Vec<_> = std::iter::from_fn(&mut lines).collect();
    assert!(
        sent.iter().all(|line| !line.contains("nearby")),
        "opening the card starts no host discovery"
    );
    let id = cx.update(|_, cx| card.read(cx).model.provider_check.unwrap());
    for side in [1, 0] {
        cx.update(|_, cx| {
            shell.update(cx, |_, cx| {
                cx.emit(crate::g2::mn1::join::ProducerReply {
                    side,
                    line: reply(id, "found", HOSTS),
                });
            })
        });
    }
    frame(cx);
    cx.update(|_, cx| {
        let view = card.read(cx);
        assert!(
            view.model.form.host.is_empty(),
            "a nearby reply fills no field"
        );
        assert_eq!(view.model.focus, Focus::Host);
    });
    cx.simulate_keystrokes("down enter");
    frame(cx);
    cx.update(|_, cx| {
        let view = card.read(cx);
        assert!(
            view.model.form.uri().is_none(),
            "Down and Enter take no host from a list"
        );
        assert!(!view.model.connecting && !view.model.saving);
    });
    cx.update(|_, cx| card.update(cx, |card, cx| card.close(cx)));
    frame(cx);
    assert!(
        std::iter::from_fn(&mut lines).all(|line| !line.contains("nearby")),
        "closing the card stops no discovery"
    );
}
