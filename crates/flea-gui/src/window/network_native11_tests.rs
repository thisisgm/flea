//! Native followups retain the source pane and expose the live browser through the real Root.
use super::*;
use gpui::VisualContext;

pub(super) fn shares_on_other_pane(
    cx: &mut TestAppContext,
    owner: usize,
) -> (Entity<Shell>, &mut VisualTestContext, [Lines; 2]) {
    let (shell, cx, lines) = open(Rc::new(Recorder::default()), cx);
    cx.replace_root_view(|_, _| crate::window::root::Root {
        shell: shell.clone(),
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(shell.view, crate::window::startup::View::Dual);
            let origin = shell.network_origin(owner);
            shell.focus_pane(1 - owner, cx);
            window.focus(&shell.focus, cx);
            shell.network_service(
                &Effect::Shares {
                    uri: "smb://origin.test/".into(),
                    label: "Origin".into(),
                    names: vec!["share".into(), "second".into()],
                    origin,
                },
                window,
                cx,
            );
        });
    });
    frame(cx);
    (shell, cx, lines)
}

#[gpui::test]
fn uish_misc_native11_other_pane_shares_do_not_take_current_list_focus(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = shares_on_other_pane(cx, 0);
    cx.update(|window, cx| {
        let shell = shell.read(cx);
        assert_eq!(shell.focus_side, 1);
        assert!(
            shell.focus.is_focused(window),
            "late shares must leave the other list's actual focus"
        );
    });
}

#[gpui::test]
fn uish_misc_native11_other_list_keys_leave_share_cursor_alone(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = shares_on_other_pane(cx, 0);
    cx.simulate_keystrokes("j");
    frame(cx);
    cx.update(|_, cx| {
        let shell = shell.read(cx);
        assert_eq!(shell.panes[1].as_ref().unwrap().pane.cursor, 1);
        assert_eq!(
            shell
                .rail
                .as_ref()
                .unwrap()
                .network
                .as_ref()
                .unwrap()
                .shares
                .read(cx)
                .model
                .cursor,
            0
        );
    });
}

#[gpui::test]
fn uish_misc_native11_pointer_on_shares_focuses_their_source_pane(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = shares_on_other_pane(cx, 0);
    let bounds = cx.update(|_, cx| {
        shell
            .read(cx)
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .shares
            .read(cx)
            .painted_bounds()
            .unwrap()
    });
    cx.simulate_mouse_down(
        bounds.origin + gpui::point(gpui::px(20.), gpui::px(18.)),
        gpui::MouseButton::Left,
        gpui::Modifiers::none(),
    );
    frame(cx);
    assert_eq!(shell.read_with(cx, |shell, _| shell.focus_side), 0);
}

pub(super) fn feedback_read(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    function: &str,
) -> String {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                crate::ipc::server::Event::Call {
                    request: flea::ipc::wire::Request::Call {
                        target: "flea".into(),
                        function: function.into(),
                        arguments: vec![],
                    },
                    answer,
                },
                window,
                cx,
            );
            receive.try_recv().unwrap().reply.stdout
        })
    })
}

pub(super) fn share_state(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Json {
    let value = feedback_read(shell, cx, "shareBrowserState");
    // {"active":true,"focused":false,"owner":0,"rect":"192 54 400 500","baseUri":"smb://origin.test/","cursor":0,"paneRects":[]}
    flea::jsondoc::parse(value.trim()).unwrap()
}

#[gpui::test]
fn uish_misc_native11_closed_browser_keeps_its_source_owner(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = shares_on_other_pane(cx, 0);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.focus_pane(0, cx);
            window.focus(&shell.focus, cx);
        });
    });
    cx.simulate_keystrokes("escape");
    frame(cx);
    let state = share_state(&shell, cx);
    assert_eq!(state.get("active"), Some(&Json::Bool(false)));
    assert_eq!(state.get("owner"), Some(&Json::Num("0".into())));
    assert_eq!(
        state.get("baseUri").and_then(Json::as_str),
        Some("smb://origin.test/")
    );
}

#[gpui::test]
fn uish_misc_native11_hidden_browser_retains_owner_without_taking_primary_keys(
    cx: &mut TestAppContext,
) {
    let (shell, cx, _lines) = shares_on_other_pane(cx, 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.set_view(crate::window::startup::View::List, window, cx)
        });
    });
    frame(cx);
    let state = share_state(&shell, cx);
    assert_eq!(state.get("active"), Some(&Json::Bool(true)));
    assert_eq!(state.get("owner"), Some(&Json::Num("1".into())));
    cx.update(|_, cx| {
        let shell = shell.read(cx);
        let token = shell
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .shares
            .read(cx)
            .model
            .origin;
        assert_eq!(shell.network_side(token), None);
        assert_eq!(shell.focus_side, 0);
    });
    cx.simulate_keystrokes("j");
    frame(cx);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor),
        1
    );
    assert_eq!(
        share_state(&shell, cx).get("cursor"),
        Some(&Json::Num("0".into()))
    );
}

#[gpui::test]
fn uish_misc_native11_share_state_reports_live_origin_uri_and_painted_rect(
    cx: &mut TestAppContext,
) {
    let (shell, cx, _lines) = shares_on_other_pane(cx, 0);
    let opening = share_state(&shell, cx);
    let Some(Json::Arr(panes)) = opening.get("paneRects") else {
        panic!("opening browser reader must expose both pane rectangles");
    };
    assert_eq!(opening.get("rect"), panes.first());
    // Settle the real opening reveal before comparing painted rows with the owner's listing slot.
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(181));
    frame(cx);
    let state = share_state(&shell, cx);
    assert_eq!(state.get("owner"), Some(&Json::Num("0".into())));
    assert_eq!(
        state.get("baseUri").and_then(Json::as_str),
        Some("smb://origin.test/")
    );
    assert_eq!(state.get("cursor"), Some(&Json::Num("0".into())));
    let Some(Json::Arr(panes)) = state.get("paneRects") else {
        panic!("live browser reader must expose both pane rectangles");
    };
    assert_eq!(panes.len(), 2);
    assert_eq!(state.get("rect"), panes.first());
    cx.update(|_, cx| {
        let browser = shell
            .read(cx)
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .shares
            .read(cx);
        let bounds = browser
            .painted_bounds()
            .expect("the actual browser was painted");
        assert_eq!(
            state.get("rect").and_then(Json::as_str),
            Some(
                crate::ipc::rows::rect_of(
                    f32::from(bounds.origin.x),
                    f32::from(bounds.origin.y),
                    f32::from(bounds.size.width),
                    f32::from(bounds.size.height),
                )
                .as_str()
            )
        );
    });
}
