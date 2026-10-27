//! Exercise dual layout through live shell dispatch in a GPUI window.
use crate::{backend::Backend, ipc::server::Event, state::tests::shell_with};
use flea::{
    ipc::wire::{Reply, Request},
    jsondoc::Json,
};
use gpui::{AppContext, TestAppContext};

#[gpui::test]
fn u26_dual_reader_observes_retained_sessions_and_refuses_unknown_calls(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            shell.panes[0].as_mut().unwrap().pane.cursor = 4;
            shell.panes[0].as_mut().unwrap().selected = vec![2, 4];
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: "dualState".into(),
                        arguments: vec![],
                    },
                    answer,
                },
                window,
                cx,
            );
            let reply = receive.try_recv().unwrap().reply;
            assert_eq!(
                reply.code, 0,
                "dualState must read the live shell: {}",
                reply.stderr
            );
            let state = flea::jsondoc::parse(&reply.stdout).unwrap();
            assert_eq!(state.get("active").and_then(Json::as_bool), Some(false));
            let panes = state.get("panes").and_then(Json::as_array).unwrap();
            assert_eq!(panes[0].get("cursor").and_then(Json::as_f64), Some(4.));
            assert_eq!(
                panes[0]
                    .get("selected")
                    .and_then(Json::as_array)
                    .unwrap()
                    .len(),
                2
            );
            assert_eq!(panes[1], Json::Null);
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: "railState".into(),
                        arguments: vec![],
                    },
                    answer,
                },
                window,
                cx,
            );
            assert_eq!(
                receive.try_recv().unwrap().reply,
                Reply::error(3, "FLEA_IPC_NOT_PORTED flea.railState")
            );
        });
    });
}

#[gpui::test]
fn u26_auto_hide_uses_overlay_room_for_both_listings(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, _| {
            shell.view = super::startup::View::Dual;
            shell.launch.settings = super::startup::Settings::from_json(
                flea::jsondoc::parse(r#"{"places":{"autoHide":true}}"#).unwrap(),
            );
            let geometry = shell.geometry(window);
            assert_eq!(
                geometry.panes[0].x, 0.,
                "an overlay rail reserves no pane width"
            );
            assert!((geometry.panes[0].width - geometry.panes[1].width).abs() <= 1.);
        });
    });
}
