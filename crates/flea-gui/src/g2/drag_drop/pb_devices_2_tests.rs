//! Files dropped on a painted tab reach that folder through the mounted transfer owner.
use super::mounted_tests::{self as mounted, Root};
use crate::tabs::state::{Tab, Tabs};
use gpui::{point, px, NativeDragAction, Pixels, Point, VisualTestContext};
use std::sync::Arc;

pub(super) fn tabs(shell: &mut crate::state::Shell, dest: &str) {
    let src = shell.panes[0].as_ref().unwrap().path.clone();
    let mut first = Tab::at(src);
    first.identity = 1;
    let mut second = Tab::at(dest);
    second.identity = 2;
    shell.strip.tabs[0].tabs = Some(Tabs {
        items: vec![first, second],
        index: 0,
        pending: Default::default(),
    });
}

pub(super) fn tab_point(shell: &crate::state::Shell) -> Point<Pixels> {
    let bounds = shell.strip.bounds.get().expect("the tab strip painted");
    let width = shell.strip.snapshot.as_ref().unwrap().frame.tab_width;
    point(
        bounds.origin.x + px(crate::tabs::bar::PAD_X + width * 1.5),
        bounds.origin.y + bounds.size.height / 2.,
    )
}

fn transfer(
    handle: gpui::WindowHandle<crate::state::Shell>,
    cx: &mut VisualTestContext,
    lines: &mut impl FnMut() -> Option<String>,
) -> String {
    let sent = mounted::drain(lines);
    let probe = sent
        .iter()
        .find(|line| line.contains(r#""c":"collisions""#));
    assert!(
        probe.is_some(),
        "the tab drop sent no collision request: {sent:?}"
    );
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                crate::backend::Event::Operations(
                    r#"{"t":"collisions","id":1,"total":0,"names":[],"mergeable":false}"#.into(),
                ),
                window,
                cx,
            );
        })
        .unwrap();
    cx.run_until_parked();
    mounted::drain(lines)
        .into_iter()
        .chain(sent)
        .find(|line| line.contains(r#""c":"transfer""#))
        .expect("one transfer")
}

#[test]
fn pb_devices_2_pointer_file_drop_on_an_inactive_tab_moves_into_its_folder() {
    let root = Root::new("tab-move");
    let dest = format!("{}/alpha", root.src());
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut lines) = mounted::open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, window, cx| {
            tabs(shell, &dest);
            shell.follow_file_drags(window, cx);
        })
        .unwrap();
    mounted::frame(&mut cx, handle);
    let file = mounted::centre(&mut cx, handle, 1);
    let target = handle
        .update(&mut cx, |shell, _, _| tab_point(shell))
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    mounted::drag(&mut vcx, file, target);
    vcx.run_until_parked();
    let sent = transfer(handle, &mut vcx, &mut lines);
    assert!(sent.contains(r#""op":"move""#), "{sent}");
    assert!(sent.contains(&format!(r#""dest":"{dest}""#)), "{sent}");
    handle
        .update(&mut vcx, |shell, _, _| {
            assert_eq!(
                shell.strip.tabs[0].current(),
                0,
                "a direct drop does not select the tab"
            );
        })
        .unwrap();
}

#[test]
fn pb_devices_2_native_file_drop_on_an_inactive_tab_copies_into_its_folder() {
    native_drop_on_tab(false, false);
}

#[test]
fn pb_devices_2_inactive_tab_accepts_a_drop_while_the_current_tab_loads() {
    native_drop_on_tab(true, false);
}

#[test]
fn pb_devices_2_current_loading_tab_refuses_a_drop() {
    native_drop_on_tab(true, true);
}

fn native_drop_on_tab(loading: bool, current: bool) {
    let root = Root::new("tab-native");
    let dest = format!("{}/alpha", root.src());
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut lines) = mounted::open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, window, cx| {
            tabs(shell, &dest);
            shell.follow_file_drags(window, cx);
        })
        .unwrap();
    mounted::frame(&mut cx, handle);
    let target = handle
        .update(&mut cx, |shell, window, cx| {
            let mut point = tab_point(shell);
            if current {
                point.x -= px(shell.strip.snapshot.as_ref().unwrap().frame.tab_width);
            }
            if loading {
                shell
                    .step(
                        0,
                        crate::window::navigate::Step::OpenPlace("/fixture/loading".into()),
                        window,
                        cx,
                    )
                    .unwrap();
                assert!(shell.panes[0].as_ref().unwrap().pane.busy());
            }
            point
        })
        .unwrap();
    mounted::frame(&mut cx, handle);
    let uri: Arc<[u8]> = format!(
        "{}\r\n",
        flea::ui_logic_drag_drop::uri_for(&format!("{}/b.txt", root.src()))
    )
    .into_bytes()
    .into();
    let types = vec![flea::ui_logic_drag_drop::URILIST.to_owned()];
    handle
        .update(&mut cx, |_, window, cx| {
            let data =
                |mime: &str| (mime == flea::ui_logic_drag_drop::URILIST).then(|| uri.clone());
            let native = super::router::Native::new(
                window.window_handle().window_id().as_u64(),
                (target.x.into(), target.y.into()),
                true,
                &types,
                &data,
                Some(NativeDragAction::Copy),
                false,
                false,
            );
            assert!(super::router::shared(cx).land(&native));
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.run_until_parked();
    if loading && current {
        assert!(mounted::drain(&mut lines)
            .iter()
            .all(|line| !line.contains("collisions") && !line.contains("transfer")));
        return;
    }
    let sent = transfer(handle, &mut vcx, &mut lines);
    assert!(sent.contains(r#""op":"copy""#), "{sent}");
    assert!(sent.contains(&format!(r#""dest":"{dest}""#)), "{sent}");
}

#[test]
fn pb_devices_2_control_pointer_drop_on_a_tab_copies() {
    let root = Root::new("tab-control");
    let dest = format!("{}/alpha", root.src());
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut lines) = mounted::open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, _, _| tabs(shell, &dest))
        .unwrap();
    mounted::frame(&mut cx, handle);
    let file = mounted::centre(&mut cx, handle, 1);
    let target = handle
        .update(&mut cx, |shell, _, _| tab_point(shell))
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    let ctrl = gpui::Modifiers {
        control: true,
        ..Default::default()
    };
    vcx.simulate_mouse_move(file, None, Default::default());
    vcx.simulate_mouse_down(file, gpui::MouseButton::Left, Default::default());
    vcx.simulate_mouse_move(target, Some(gpui::MouseButton::Left), ctrl);
    vcx.simulate_mouse_up(target, gpui::MouseButton::Left, ctrl);
    vcx.run_until_parked();
    let sent = transfer(handle, &mut vcx, &mut lines);
    assert!(sent.contains(r#""op":"copy""#), "{sent}");
    assert!(sent.contains(&format!(r#""dest":"{dest}""#)), "{sent}");
}

#[test]
fn pb_devices_2_new_tab_mark_never_accepts_a_file_drop() {
    let root = Root::new("tab-new");
    let dest = format!("{}/alpha", root.src());
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut lines) = mounted::open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, _, _| tabs(shell, &dest))
        .unwrap();
    mounted::frame(&mut cx, handle);
    let file = mounted::centre(&mut cx, handle, 1);
    let target = handle
        .update(&mut cx, |shell, _, _| {
            let bounds = shell.strip.bounds.get().unwrap();
            let width = shell.strip.snapshot.as_ref().unwrap().frame.tab_width;
            point(
                bounds.origin.x + px(crate::tabs::bar::PAD_X + width * 2. + 12.),
                bounds.origin.y + bounds.size.height / 2.,
            )
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    mounted::drag(&mut vcx, file, target);
    vcx.run_until_parked();
    assert!(mounted::drain(&mut lines)
        .iter()
        .all(|line| !line.contains("collisions") && !line.contains("transfer")));
    handle
        .update(&mut vcx, |shell, _, _| {
            assert_eq!(shell.strip.tabs[0].count(), 2)
        })
        .unwrap();
}
