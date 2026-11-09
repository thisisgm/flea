//! U-DRAG-DROP: the drag image, the native crumb drop and the sub-slop drop gate; a lift or a ground press ends the slow click.
use super::mounted_tests::{centre, drain, frame, open, transfer, Root};
use super::u_drag_drop_tests::{crumb, mounted, open_listed, out_and_back, status};
use crate::list::bench;
use crate::state::Shell;
use gpui::{point, px, Modifiers, MouseButton, VisualTestContext, WindowHandle};

#[test]
fn ey376_the_drag_image_draws_the_lift() {
    let root = Root::new("ey376");
    let mut cx = bench::real_cx();
    let handle = mounted(&mut cx, &root);
    let file = centre(&mut cx, handle, 1);
    let folder = centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    super::rail_drop_tests::hold(&mut vcx, file, folder, Modifiers::default());
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    // Board carry.html's front plate: a glyph square 19 px right and 27 px below the pointer.
    let (quads, scale, at) = vcx.update(|window, _| {
        (
            window.painted_quads(),
            window.scale_factor(),
            window.mouse_position(),
        )
    });
    let x = (f32::from(at.x) + 19.) * scale;
    let y = (f32::from(at.y) + 27.) * scale;
    assert!(
        quads.iter().any(|quad| {
            (quad.bounds.origin.x.0 - x).abs() < 0.5
                && (quad.bounds.origin.y.0 - y).abs() < 0.5
                && (quad.bounds.size.width.0 - 19. * scale).abs() < 0.5
        }),
        "no flock plate beside the pointer at {at:?}"
    );
    vcx.simulate_mouse_up(folder, MouseButton::Left, Modifiers::default());
}

#[test]
fn ey271_a_native_file_drop_on_a_parent_crumb_moves_there() {
    use flea::ui_logic_drag_drop::URILIST;
    let root = Root::new("ey271native");
    let parent = root.src().trim_end_matches("/src").to_owned();
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open_listed(&mut cx, root.src());
    let (target, _) = crumb(&mut cx, handle, &parent);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    handle
        .update(&mut vcx, |_, window, cx| {
            let types = vec![URILIST.to_owned()];
            let uris: std::sync::Arc<[u8]> = format!("file://{}/b.txt\r\n", root.src())
                .into_bytes()
                .into();
            let get = |mime: &str| (mime == URILIST).then(|| uris.clone());
            let router = super::router::shared(cx);
            assert!(router.land(&super::router::Native::new(
                window.window_handle().window_id().as_u64(),
                (f32::from(target.x), f32::from(target.y)),
                true,
                &types,
                &get,
                Some(gpui::NativeDragAction::Move),
                false,
                false,
            )));
        })
        .unwrap();
    vcx.run_until_parked();
    let sent = transfer(&mut vcx, handle, &mut lines);
    assert!(sent.contains(r#""op":"move""#), "{sent}");
    assert!(sent.contains(&format!(r#""dest":"{parent}""#)), "{sent}");
    assert!(sent.contains(&format!("{}/b.txt", root.src())), "{sent}");
}

#[test]
fn ey395_a_press_that_never_left_the_slop_still_drops_nothing() {
    let root = Root::new("ey395tap");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.follow_file_drags(window, cx)
        })
        .unwrap();
    frame(&mut cx, handle);
    let folder = centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    let near = point(folder.x + px(4.), folder.y + px(3.));
    out_and_back(&mut vcx, folder, near, folder);
    let before = status(&mut vcx, handle);
    // Mount a live lift even though GPUI itself may ignore this small pointer motion.
    handle
        .update(&mut vcx, |shell, window, cx| {
            assert!(shell.begin_file_drag(0, window, cx));
            shell.land_own_drag(0, window, cx);
        })
        .unwrap();
    vcx.run_until_parked();
    assert_eq!(
        status(&mut vcx, handle),
        before,
        "a sub-slop drop is ignored, never refused"
    );
    vcx.simulate_mouse_up(folder, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    assert!(
        drain(&mut lines)
            .iter()
            .all(|line| !line.contains("transfer") && !line.contains("collisions")),
        "a jitter may open the folder but never transfers it"
    );
}

// A lone tap on the sole marked name, recorded so the next slow tap there would rename it.
fn armed_record(vcx: &mut VisualTestContext, handle: WindowHandle<Shell>) {
    handle
        .update(vcx, |shell, window, cx| {
            let tap = crate::window::rename::slow::Tap {
                row: 0,
                on_name: true,
                modifiers: Modifiers::default(),
                was_sole: true,
            };
            shell.slow_tap(0, tap, window, cx);
            assert!(shell.rename.slow.is_some(), "the tap records itself");
        })
        .unwrap();
}

fn recorded(vcx: &mut VisualTestContext, handle: WindowHandle<Shell>) -> bool {
    handle
        .update(vcx, |shell, _, _| shell.rename.slow.is_some())
        .unwrap()
}

#[test]
fn pb_drag_1_a_lift_or_a_ground_press_cancels_the_pending_slow_click() {
    let root = Root::new("slowcancel");
    let mut cx = bench::real_cx();
    let (handle, _lines) = open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.follow_file_drags(window, cx)
        })
        .unwrap();
    frame(&mut cx, handle);
    let row = centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    armed_record(&mut vcx, handle);
    vcx.simulate_mouse_move(row, None, Modifiers::default());
    vcx.simulate_mouse_down(row, MouseButton::Left, Modifiers::default());
    assert!(recorded(&mut vcx, handle), "a row press keeps the record");
    handle
        .update(&mut vcx, |shell, window, cx| {
            assert!(shell.begin_file_drag(0, window, cx));
            assert!(
                shell.rename.slow.is_none(),
                "FileDrag.liftBegan cancels the slow click"
            );
            shell.escape_file_drag(window, cx);
        })
        .unwrap();
    vcx.simulate_mouse_up(row, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    armed_record(&mut vcx, handle);
    let ground = handle
        .update(&mut vcx, |shell, _, _| {
            let bounds = shell.panes[0].as_ref().unwrap().input.frame.unwrap().bounds;
            point(bounds.origin.x + px(40.), bounds.bottom() - px(12.))
        })
        .unwrap();
    vcx.simulate_mouse_move(ground, None, Modifiers::default());
    vcx.simulate_mouse_down(ground, MouseButton::Left, Modifiers::default());
    assert!(
        !recorded(&mut vcx, handle),
        "SelectionBand's ground press cancels the slow click"
    );
    vcx.simulate_mouse_up(ground, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
}
