//! windows-cut-shiftdrag: a drag held over a folder row lights it and says what a release does, for this window's lift and another window's.
use super::super::super::mounted_tests::{centre, frame, open, Root};
use super::super::{window_key, Drags};
use super::Arrived;
use crate::{list::bench, state::Shell};
use gpui::{
    point, px, ExternalPaths, FileDropEvent, Modifiers, MouseButton, Pixels, Point,
    VisualTestContext, WindowHandle,
};

// The drop cue the List pane draws and the status bar's activity lines.
fn cue(
    vcx: &mut VisualTestContext,
    handle: WindowHandle<Shell>,
) -> (Option<usize>, Option<String>, Vec<String>) {
    handle
        .update(vcx, |shell, window, cx| {
            let states = shell.list_states(0, window, cx);
            let lines = shell.status_messages(cx, |messages, _| {
                messages
                    .activities()
                    .iter()
                    .map(|activity| activity.text.clone())
                    .collect()
            });
            (
                states.drop_target,
                states.drop_label.map(|label| label.to_string()),
                lines,
            )
        })
        .unwrap()
}

// Press on a row and carry it to `to`, still held.
fn hold(vcx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    vcx.simulate_mouse_move(from, None, Modifiers::default());
    vcx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(
        point(from.x + px(3.), from.y - px(14.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    for _ in 0..2 {
        vcx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
        vcx.run_until_parked();
    }
}

#[test]
fn own_drag_over_a_folder_row_lights_it_and_says_the_line() {
    let root = Root::new("rowheld");
    let mut cx = bench::real_cx();
    let (handle, _lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let folder = centre(&mut cx, handle, 0);
    let file = centre(&mut cx, handle, 1);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    hold(&mut vcx, file, folder);
    let (target, label, lines) = cue(&mut vcx, handle);
    assert_eq!(target, Some(0));
    assert_eq!(label.as_deref(), Some("move here"));
    assert!(
        lines.contains(&"Move 1 item to alpha · ctrl copies, shift moves".to_owned()),
        "{lines:?}"
    );
    // Back on its own row, the lift rests on no folder: FileDrag.leaveTarget says the line without a name.
    hold_on(&mut vcx, file);
    let (target, _, lines) = cue(&mut vcx, handle);
    assert_eq!(target, None);
    assert!(
        lines.contains(&"Move 1 item to a folder · ctrl copies, shift moves".to_owned()),
        "{lines:?}"
    );
    vcx.simulate_mouse_up(file, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    frame(&mut cx, handle);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    let (target, _, lines) = cue(&mut vcx, handle);
    assert_eq!(target, None, "the release clears the cue");
    assert!(lines.is_empty(), "{lines:?}");
}

fn hold_on(vcx: &mut VisualTestContext, to: Point<Pixels>) {
    for _ in 0..2 {
        vcx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
        vcx.run_until_parked();
    }
}

#[test]
fn another_windows_shift_drag_over_a_folder_row_says_move() {
    let root = Root::new("rowforeign");
    let other = format!("{}/other", root.src().trim_end_matches("/src"));
    std::fs::create_dir_all(&other).unwrap();
    let carried: Vec<String> = ["c.txt", "d.txt", "e.txt"]
        .iter()
        .map(|name| {
            let path = format!("{other}/{name}");
            std::fs::write(&path, b"x").unwrap();
            path
        })
        .collect();
    let mut cx = bench::real_cx();
    let (handle, _lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let folder = centre(&mut cx, handle, 0);
    let file = centre(&mut cx, handle, 1);
    // The router's file client records the offer at Enter, before the platform reports the drag inside the window.
    handle
        .update(&mut cx, |_, window, cx| {
            let key = window_key(window);
            let hover = cx
                .default_global::<Drags>()
                .hovers
                .entry(key)
                .or_default()
                .clone();
            *hover.borrow_mut() = Some(Arrived {
                position: (0., 0.),
                marker: Some(
                    flea::ui_logic_dg2::Marker {
                        instance: "other-process#1".into(),
                        source: other.clone(),
                        rows: vec![0, 1, 2],
                        listing: 1,
                        dev: 0,
                        deletable: true,
                        copy: false,
                        shift: true,
                    }
                    .encode(),
                ),
                urls: carried
                    .iter()
                    .map(|path| flea::ui_logic_dg2::uri_for(path))
                    .collect(),
                plain: None,
                shelf: None,
                action: None,
                ctrl: false,
                shift: true,
            });
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_event(FileDropEvent::Entered {
        position: file,
        paths: ExternalPaths(carried.iter().map(Into::into).collect()),
    });
    for _ in 0..2 {
        vcx.simulate_event(FileDropEvent::Pending { position: folder });
        vcx.run_until_parked();
    }
    let (target, label, lines) = cue(&mut vcx, handle);
    assert_eq!(target, Some(0));
    assert_eq!(label.as_deref(), Some("move here"));
    assert!(
        lines.contains(&"Move 3 items to alpha · ctrl copies, shift moves".to_owned()),
        "{lines:?}"
    );
    let (source, _lines) = open(&mut cx, root.src());
    source
        .update(&mut cx, |shell, window, cx| window.focus(&shell.focus, cx))
        .unwrap();
    frame(&mut cx, source);
    let mut source_cx = VisualTestContext::from_window(source.into(), &cx);
    source_cx.simulate_event(gpui::ModifiersChangedEvent {
        modifiers: Modifiers {
            control: true,
            ..Modifiers::default()
        },
        capslock: Default::default(),
    });
    source_cx.run_until_parked();
    let (_, label, _) = cue(&mut vcx, handle);
    assert_eq!(
        label.as_deref(),
        Some("copy here"),
        "source keys refresh the other window"
    );
    vcx.simulate_event(FileDropEvent::Pending { position: folder });
    vcx.run_until_parked();
    let (_, label, lines) = cue(&mut vcx, handle);
    assert_eq!(label.as_deref(), Some("copy here"));
    assert!(
        lines.contains(&"Copy 3 items to alpha".to_owned()),
        "released Shift cannot combine with current Ctrl: {lines:?}"
    );
    vcx.simulate_event(FileDropEvent::Pending { position: file });
    vcx.run_until_parked();
    let (target, _, lines) = cue(&mut vcx, handle);
    assert_eq!(target, None, "a plain file row takes no drop");
    assert!(lines.is_empty(), "{lines:?}");
    vcx.simulate_event(FileDropEvent::Pending { position: folder });
    vcx.run_until_parked();
    vcx.simulate_event(FileDropEvent::Exited);
    vcx.run_until_parked();
    frame(&mut cx, handle);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    let (target, _, lines) = cue(&mut vcx, handle);
    assert_eq!(target, None, "the drag leaving the window clears the cue");
    assert!(lines.is_empty(), "{lines:?}");
}

// The lift left for another window: no frame here carries it, so the source keeps the line without a name until the native drag ends.
#[test]
fn a_lift_away_in_another_window_keeps_the_line_until_it_ends() {
    let root = Root::new("rowaway");
    let mut cx = bench::real_cx();
    let (handle, _lines) = open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.follow_file_drags(window, cx)
        })
        .unwrap();
    frame(&mut cx, handle);
    let folder = centre(&mut cx, handle, 0);
    let file = centre(&mut cx, handle, 1);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    hold(&mut vcx, file, file);
    hold_on(&mut vcx, folder);
    // As settle_rail_drag does on a frame with no drag in flight.
    handle
        .update(&mut vcx, |shell, window, cx| {
            shell.clear_rail_aim(window, cx);
            shell.settle_row_drag(window, cx);
        })
        .unwrap();
    let (target, _, lines) = cue(&mut vcx, handle);
    assert_eq!(
        target, None,
        "the folder under the pointer is in the other window now"
    );
    assert_eq!(
        lines,
        vec!["Move 1 item to a folder · ctrl copies, shift moves".to_owned()]
    );
    // The real router and file client must deliver the end notification to the source Shell.
    handle
        .update(&mut vcx, |_, window, cx| {
            let key = window_key(window);
            let serial = cx.global::<Drags>().held[&key].borrow().as_ref().unwrap().1;
            super::super::super::router::shared(cx).source(&gpui::NativeDragEvent {
                source_id: serial,
                action: None,
                phase: gpui::NativeDragPhase::Finished,
            });
        })
        .unwrap();
    vcx.run_until_parked();
    let (_, _, lines) = cue(&mut vcx, handle);
    assert!(lines.is_empty(), "{lines:?}");
}

fn drop_active(vcx: &mut VisualTestContext, handle: WindowHandle<Shell>) -> String {
    handle
        .update(vcx, |shell, window, cx| {
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                crate::ipc::server::Event::Call {
                    request: flea::ipc::wire::Request::Call {
                        target: "flea".into(),
                        function: "listingDropActive".into(),
                        arguments: Vec::new(),
                    },
                    answer,
                },
                window,
                cx,
            );
            receive.try_recv().unwrap().reply.stdout.trim().to_owned()
        })
        .unwrap()
}

// Judge X 2125: listingDropActive is PaneWire.qml's floor, so a held folder row hides it and the listing past the rows shows it.
#[test]
fn fixreaders_listing_drop_active_reads_the_floor_not_a_folder_row() {
    let root = Root::new("rowfloor");
    let mut cx = bench::real_cx();
    let (handle, _lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let folder = centre(&mut cx, handle, 0);
    let file = centre(&mut cx, handle, 1);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    assert_eq!(drop_active(&mut vcx, handle), "false", "no drag");
    hold(&mut vcx, file, folder);
    assert_eq!(cue(&mut vcx, handle).0, Some(0));
    assert_eq!(
        drop_active(&mut vcx, handle),
        "false",
        "a folder row holds it"
    );
    hold_on(&mut vcx, file);
    assert_eq!(cue(&mut vcx, handle).0, None);
    assert_eq!(drop_active(&mut vcx, handle), "true", "the listing's floor");
    vcx.simulate_mouse_up(file, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    frame(&mut cx, handle);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    assert_eq!(
        drop_active(&mut vcx, handle),
        "false",
        "the release ends it"
    );
}
