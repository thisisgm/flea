//! The mounted shell must forward nearby replies to the existing network card.
use super::*;

#[gpui::test]
fn m9_g2rows_19_window_nearby_reply_populates_real_form(cx: &mut TestAppContext) {
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
    let id = cx.update(|_, cx| card.read(cx).model.discovery.id.unwrap());
    let sent: Vec<_> = std::iter::from_fn(&mut lines).collect();
    let started = format!(r#"{{"c":"nearby","op":"start","id":{id}}}"#);
    assert_eq!(sent.iter().filter(|line| **line == started).count(), 1);
    cx.update(|_, cx| {
        shell.update(cx, |_, cx| {
            cx.emit(crate::g2::mn1::join::ProducerReply {
                side: 1,
                line: reply(id, "found", HOSTS),
            });
        })
    });
    frame(cx);
    cx.update(|_, cx| assert!(card.read(cx).model.discovery.rows.is_empty()));
    cx.update(|_, cx| {
        shell.update(cx, |_, cx| {
            cx.emit(crate::g2::mn1::join::ProducerReply {
                side: 0,
                line: reply(id, "found", HOSTS),
            });
        })
    });
    frame(cx);
    cx.update(|_, cx| assert_eq!(card.read(cx).model.discovery.rows.len(), 3));
    cx.simulate_keystrokes("down enter");
    frame(cx);
    cx.update(|_, cx| {
        let view = card.read(cx);
        assert_eq!(
            view.model.form.uri().as_deref(),
            Some("sftp://east:2222/work")
        );
        assert_eq!(view.model.focus, Focus::Connect);
        assert!(!view.model.connecting && !view.model.saving);
    });
    assert!(!recorder.jobs.borrow().iter().any(|job| {
        job.argv.get(1).is_some_and(|arg| arg == "mount")
            && !job.argv.iter().any(|arg| arg == "-li")
    }));
    cx.simulate_keystrokes("enter");
    frame(cx);
    assert!(recorder
        .jobs
        .borrow()
        .iter()
        .any(|job| { job.argv == ["gio", "mount", "--", "sftp://east:2222/work"] }));
    cx.update(|_, cx| card.update(cx, |card, cx| card.close(cx)));
    frame(cx);
    let stopped = format!(r#"{{"c":"nearby","op":"stop","id":{id}}}"#);
    assert!(std::iter::from_fn(&mut lines).any(|line| line == stopped));
}
