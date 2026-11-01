//! Native view changes retain the browser slot and the user's newer keyboard focus.
use super::native11_tests::{feedback_read, share_state};
use super::*;
use gpui::VisualContext;

#[gpui::test]
fn uish_misc_native11_reopened_secondary_browser_escape_returns_to_its_listing(
    cx: &mut TestAppContext,
) {
    let (shell, cx, _lines) = open(Rc::new(Recorder::default()), cx);
    cx.replace_root_view(|_, _| crate::window::root::Root {
        shell: shell.clone(),
    });
    cx.update(|_, cx| shell.update(cx, |shell, cx| shell.focus_pane(1, cx)));
    super::handoff_tests::focus_rail(&shell, cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let origin = shell.network_origin(1);
            shell.network_service(
                &Effect::Shares {
                    uri: "smb://secondary.test/".into(),
                    label: "Secondary".into(),
                    names: vec!["share".into(), "second".into()],
                    origin,
                },
                window,
                cx,
            );
        });
    });
    frame(cx);
    assert_eq!(
        share_state(&shell, cx).get("owner"),
        Some(&Json::Num("1".into()))
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.set_view(crate::window::startup::View::List, window, cx);
        })
    });
    frame(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.set_view(crate::window::startup::View::Dual, window, cx);
        })
    });
    frame(cx);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.view),
        crate::window::startup::View::Dual
    );
    cx.simulate_keystrokes("tab");
    frame(cx);
    assert_eq!(shell.read_with(cx, |shell, _| shell.focus_side), 1);
    cx.simulate_keystrokes("escape");
    frame(cx);
    cx.update(|window, cx| {
        let shell = shell.read(cx);
        assert_eq!(shell.keyboard.context, flea::keys::Context::Listing);
        assert!(shell.focus.is_focused(window));
    });
    cx.simulate_keystrokes("j");
    frame(cx);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[1].as_ref().unwrap().pane.cursor),
        1
    );
}

#[gpui::test]
fn uish_misc_native11_lazy_views_place_shares_in_their_actual_listing_slot(
    cx: &mut TestAppContext,
) {
    for view in [
        crate::window::startup::View::List,
        crate::window::startup::View::Grid,
        crate::window::startup::View::Columns,
    ] {
        let (shell, cx, _lines) = open(Rc::new(Recorder::default()), cx);
        cx.replace_root_view(|_, _| crate::window::root::Root {
            shell: shell.clone(),
        });
        cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(view, window, cx)));
        frame(cx);
        assert_eq!(shell.read_with(cx, |shell, _| shell.view), view);
        let slot = feedback_read(&shell, cx, "listAreaRect").trim().to_owned();
        assert!(!slot.is_empty(), "active {view:?} must have a painted slot");
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                let origin = shell.network_origin(0);
                shell.network_service(
                    &Effect::Shares {
                        uri: "smb://views.test/".into(),
                        label: "Views".into(),
                        names: vec!["share".into()],
                        origin,
                    },
                    window,
                    cx,
                );
            })
        });
        frame(cx);
        assert_eq!(
            share_state(&shell, cx).get("rect").and_then(Json::as_str),
            Some(slot.as_str()),
            "active {view:?} owns the browser viewport"
        );
    }
}
