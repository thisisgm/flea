//! Empty-folder Omi observations through the existing IPC reader.
use super::*;

#[gpui::test]
fn u05_empty_omi_observes_the_mounted_player_and_its_lifecycle(cx: &mut TestAppContext) {
    use crate::window::root::Root;
    use gpui::AppContext;
    fn tick(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
        handle
            .update(cx, |_, window, cx| {
                window.simulate_next_frame(cx);
            })
            .unwrap();
        cx.run_until_parked();
    }
    fn observed(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> flea::jsondoc::Json {
        handle
            .update(cx, |root, window, cx| {
                root.shell.update(cx, |shell, cx| {
                    let (answer, receive) = std::sync::mpsc::sync_channel(1);
                    shell.ipc(
                        Event::Call {
                            request: Request::Call {
                                target: "flea".into(),
                                function: "emptyHeroState".into(),
                                arguments: vec![],
                            },
                            answer,
                        },
                        window,
                        cx,
                    );
                    let reply = receive.try_recv().unwrap().reply;
                    assert_eq!(reply.code, 0);
                    flea::jsondoc::parse(reply.stdout.trim()).unwrap()
                })
            })
            .unwrap()
    }
    let theme = crate::palette::Theme {
        accent: crate::palette::Theme::from_text("accent='#729df2'").accent,
        ..Default::default()
    };
    cx.update(|cx| cx.set_global(theme));
    let (backend, requests) = Backend::test_channel();
    let handle = cx.add_window(|window, cx| {
        window.set_rem_size(gpui::px(14.));
        Root {
            shell: cx.new(|cx| {
                cx.on_release(move |_, _| drop(requests)).detach();
                let mut shell = crate::state::tests::shell_with(backend, cx);
                shell.panes[0].as_mut().unwrap().pane.state.kind = crate::pane::states::Kind::Empty;
                shell
            }),
        }
    });
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(1100.), gpui::px(700.)));
    for _ in 0..700 {
        tick(cx, handle);
    }
    let before = observed(cx, handle);
    assert_eq!(
        before.get("markColor").unwrap().as_str(),
        Some(crate::ipc::rows::colour(theme.accent).as_str())
    );
    let omi = before
        .get("omi")
        .expect("the mounted empty hero must report its actual Omi");
    assert_eq!(omi.get("mode").unwrap().as_str(), Some("thinking"));
    assert_eq!(
        omi.get("label").unwrap().as_str(),
        Some(crate::feedback::spinner::Mode::Thinking.label())
    );
    assert_eq!(omi.get("shown").unwrap().as_bool(), Some(true));
    assert_eq!(omi.get("sampled").unwrap().as_bool(), Some(true));
    let slot = gpui::VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds("fb1-hero-mark")
        .unwrap();
    assert_eq!(
        omi.get("slot").unwrap().as_str(),
        Some(
            crate::ipc::rows::rect_of(
                f32::from(slot.origin.x),
                f32::from(slot.origin.y),
                f32::from(slot.size.width),
                f32::from(slot.size.height)
            )
            .as_str()
        )
    );
    for _ in 0..64 {
        tick(cx, handle);
    }
    let moving = observed(cx, handle);
    let moving = moving.get("omi").unwrap();
    for key in ["frames", "writes", "requests", "sampledSeconds"] {
        assert!(
            moving.get(key).unwrap().as_f64().unwrap() > omi.get(key).unwrap().as_f64().unwrap(),
            "{key} must reflect the playing instance"
        );
    }
    assert_eq!(moving.get("heroRenders"), omi.get("heroRenders"));
    cx.update(|cx| cx.set_reduce_motion(true));
    handle.update(cx, |_, window, _| window.refresh()).unwrap();
    tick(cx, handle);
    let rest = observed(cx, handle);
    let rest = rest.get("omi").unwrap();
    assert_eq!(rest.get("reducedMotion").unwrap().as_bool(), Some(true));
    assert_eq!(rest.get("sampledSeconds").unwrap().as_f64(), Some(0.));
    for _ in 0..64 {
        tick(cx, handle);
    }
    let after = observed(cx, handle);
    let after = after.get("omi").unwrap();
    for key in ["frames", "writes", "requests", "heroRenders"] {
        assert_eq!(after.get(key), rest.get(key), "rest {key}");
    }
    cx.update(|cx| cx.set_reduce_motion(false));
    handle.update(cx, |_, window, _| window.refresh()).unwrap();
    tick(cx, handle);
    let resumed = observed(cx, handle);
    let resumed = resumed.get("omi").unwrap();
    assert_eq!(resumed.get("reducedMotion").unwrap().as_bool(), Some(false));
    for _ in 0..4 {
        tick(cx, handle);
    }
    let playing = observed(cx, handle);
    let playing = playing.get("omi").unwrap();
    for key in ["frames", "writes", "requests", "sampledSeconds"] {
        assert!(
            playing.get(key).unwrap().as_f64().unwrap()
                > resumed.get(key).unwrap().as_f64().unwrap(),
            "normal motion must resume before hidden-window proof: {key}"
        );
    }
    cx.simulate_window_visibility_change(handle.into(), gpui::WindowVisibility::Hidden);
    tick(cx, handle);
    let hidden = observed(cx, handle);
    for _ in 0..64 {
        tick(cx, handle);
    }
    let after = observed(cx, handle);
    assert_eq!(after.get("omi"), hidden.get("omi"));
}
