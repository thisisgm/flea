//! Spring loading through the mounted pointer drag, including the unchanged default.
use super::mounted_tests::{centre, frame, open as open_rows, Root};
use crate::list::bench;
use gpui::{point, px, Modifiers, MouseButton, TestAppContext, VisualTestContext, WindowHandle};
use std::time::Duration;

pub(super) fn open(
    cx: &mut TestAppContext,
    folder: String,
) -> (
    WindowHandle<crate::state::Shell>,
    impl FnMut() -> Option<String>,
) {
    let (handle, _old_lines) = open_rows(cx, folder.clone());
    let (backend, lines) = crate::backend::Backend::test_lines();
    handle
        .update(cx, |shell, _, cx| {
            let rows = shell.panes[0].as_ref().unwrap().pane.held.rows.clone();
            let mut host =
                crate::window::pane::PaneHost::new(folder.clone(), None, 646., &shell.locale);
            host.backend = Some(backend);
            host.pane.use_executor(cx.background_executor().clone());
            host.pane
                .receive(flea::protocol::Reply::Listed {
                    path: folder.clone(),
                    total: rows.len(),
                })
                .unwrap();
            host.pane
                .receive(flea::protocol::Reply::Rows {
                    start: 0,
                    listing: 2,
                    rows,
                })
                .unwrap();
            host.changed(&shell.locale);
            host.nav
                .after_reply(&mut host.pane, &folder, &mut host.selected);
            host.list.borrow_mut().typeface = bench::bench_typeface();
            shell.panes[0] = Some(host);
            shell.path_labels[0] = folder.clone().into();
            shell.launch.paths[0] = folder;
        })
        .unwrap();
    (handle, lines)
}

#[test]
fn vf3_drag_dwell_opens_at_300ms_and_preserves_the_lift() {
    let root = Root::new("spring");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, _, _| {
            shell.launch.settings.data =
                flea::jsondoc::parse(r#"{"openFoldersWhileDragging":true}"#).unwrap();
        })
        .unwrap();
    frame(&mut cx, handle);
    let from = centre(&mut cx, handle, 1);
    let folder = centre(&mut cx, handle, 0);
    let mut cx = VisualTestContext::from_window(handle.into(), &cx);
    cx.simulate_mouse_move(from, None, Modifiers::default());
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        point(from.x + px(20.), from.y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_move(folder, Some(MouseButton::Left), Modifiers::default());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(299));
    cx.run_until_parked();
    handle
        .read_with(&cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.path, root.src());
            assert!(
                host.nav.asked(&host.pane).is_none(),
                "no navigation before the dwell"
            );
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    handle
        .read_with(&cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                host.nav.asked(&host.pane),
                Some(format!("{}/alpha", root.src()))
            )
        })
        .unwrap();
    let sent: Vec<_> = std::iter::from_fn(&mut lines).collect();
    assert!(
        sent.iter()
            .any(|line| line.contains("/alpha") && line.contains("list")),
        "spring loading must use the pane backend: {sent:?}"
    );
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.receive(
                0,
                crate::backend::Event::Reply(flea::protocol::Reply::Listed {
                    path: format!("{}/alpha", root.src()),
                    total: 0,
                }),
                window,
                cx,
            );
            shell.receive(
                0,
                crate::backend::Event::Reply(flea::protocol::Reply::Rows {
                    start: 0,
                    listing: 3,
                    rows: Vec::new(),
                }),
                window,
                cx,
            );
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    handle
        .read_with(&cx, |shell, _| {
            assert_eq!(
                shell.panes[0].as_ref().unwrap().path,
                format!("{}/alpha", root.src())
            )
        })
        .unwrap();
    handle
        .update(&mut cx, |shell, window, cx| {
            let gpui::ExternalDragPayload::Mime(payload) =
                shell.file_drag_payload(window, cx).unwrap()
            else {
                panic!("file payload")
            };
            let urls = payload.data.get(flea::ui_logic_drag_drop::URILIST).unwrap();
            assert!(
                String::from_utf8_lossy(&urls).contains(&flea::ui_logic_drag_drop::uri_for(
                    &format!("{}/b.txt", root.src())
                ))
            );
            shell.escape_file_drag(window, cx);
        })
        .unwrap();
}

#[test]
fn vf3_ancestor_crumb_drop_uses_its_path() {
    let root = Root::new("crumb");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let from = centre(&mut cx, handle, 1);
    let target = crumb_point(&mut cx, handle, &root, false);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    super::mounted_tests::drag(&mut vcx, from, target);
    vcx.run_until_parked();
    let transfer = super::mounted_tests::transfer(&mut vcx, handle, &mut lines);
    let dest = root.src().trim_end_matches("/src").to_owned();
    assert!(
        transfer.contains(&format!("\"dest\":\"{dest}\"")),
        "ancestor must receive the original files: {transfer}"
    );
    assert!(
        transfer.contains("\"op\":\"move\""),
        "same-device ancestor drop must move: {transfer}"
    );
    assert!(
        transfer.contains(&format!("{}/b.txt", root.src())),
        "original path must survive chrome routing: {transfer}"
    );
}

#[test]
fn vf3_held_listing_folder_paints_caption_and_clears_it_on_leave() {
    let root = Root::new("listing-cue");
    let mut cx = bench::real_cx();
    let (handle, _lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let from = centre(&mut cx, handle, 1);
    let folder = centre(&mut cx, handle, 0);
    let mut cx = VisualTestContext::from_window(handle.into(), &cx);
    cx.simulate_mouse_move(from, None, Modifiers::default());
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        from + point(px(20.), px(0.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_move(folder, Some(MouseButton::Left), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    handle
        .update(&mut cx, |shell, window, cx| {
            let states = shell.list_states(0, window, cx);
            assert_eq!(
                states.drop_target,
                Some(0),
                "only the hovered folder is a destination"
            );
            assert_eq!(states.drop_label.as_deref(), Some("move here"));
            let host = shell.panes[0].as_ref().unwrap();
            assert!(
                host.list
                    .borrow()
                    .visible()
                    .iter()
                    .any(|row| row.index == 0 && row.state.drop_target),
                "the real row painter must consume the cue"
            );
            let footer = shell.footer(cx);
            let line = footer.read(cx).message_line();
            let primary = &line.read(cx).drawn().primary;
            assert!(
                primary.contains("alpha") && primary.to_lowercase().contains("move"),
                "held status must name the actual destination: {primary}"
            );
        })
        .unwrap();
    cx.simulate_mouse_move(from, Some(MouseButton::Left), Modifiers::default());
    cx.run_until_parked();
    handle
        .update(&mut cx, |shell, window, cx| {
            assert!(shell.list_states(0, window, cx).drop_target.is_none())
        })
        .unwrap();
    cx.simulate_keystrokes("escape");
}

pub(super) fn crumb_point(
    cx: &mut TestAppContext,
    handle: WindowHandle<crate::state::Shell>,
    root: &Root,
    separator: bool,
) -> gpui::Point<gpui::Pixels> {
    handle
        .read_with(cx, |shell, cx| {
            let view = shell.chrome.view.as_ref().unwrap().read(cx).path.read(cx);
            let paint = view.paint.borrow();
            let strip = &view.strips[0];
            let index = strip
                .crumbs()
                .iter()
                .position(|c| c.path == root.src().trim_end_matches("/src"))
                .unwrap();
            let (left, width) = strip.extent(index).unwrap();
            let bounds = paint.strips[0].unwrap();
            point(
                bounds.left() + px(left + if separator { width - 1. } else { 2. }),
                bounds.top() + bounds.size.height / 2.,
            )
        })
        .unwrap()
}

#[test]
fn vf3_crumb_refuses_a_drop_while_its_pane_is_loading() {
    let root = Root::new("crumb-loading");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let from = centre(&mut cx, handle, 1);
    let crumb = crumb_point(&mut cx, handle, &root, false);
    let mut cx = VisualTestContext::from_window(handle.into(), &cx);
    cx.simulate_mouse_move(from, None, Modifiers::default());
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        from + point(px(20.), px(0.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_move(crumb, Some(MouseButton::Left), Modifiers::default());
    handle
        .update(&mut cx, |shell, window, cx| {
            shell
                .step(0, crate::window::navigate::Step::Refresh(None), window, cx)
                .unwrap();
            assert!(shell.panes[0].as_ref().unwrap().pane.busy());
        })
        .unwrap();
    cx.simulate_mouse_up(crumb, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    let sent = super::mounted_tests::drain(&mut lines);
    assert!(
        !sent
            .iter()
            .any(|line| line.contains("collisions") || line.contains("transfer")),
        "loading refusal must send no operation: {sent:?}"
    );
    handle
        .read_with(&cx, |shell, _| {
            assert_eq!(
                shell.panes[0].as_ref().unwrap().status.as_ref(),
                shell.locale.catalog.tr("pane.busy")
            )
        })
        .unwrap();
}
