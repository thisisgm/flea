//! U-DRAG-DROP cues: a drag on no target says where a folder would take it, and a held parent crumb washes its whole span.
use super::mounted_tests::{centre, Root};
use super::rail_drop_tests::{hold, hold_keys};
use super::u_drag_drop_tests::{crumb, mounted, open_listed};
use crate::path::{paint::strip_pad, strip::Hit};
use crate::{list::bench, state::Shell};
use gpui::{point, px, Modifiers, MouseButton, VisualTestContext, WindowHandle};

fn lines(vcx: &mut VisualTestContext, handle: WindowHandle<Shell>) -> Vec<String> {
    handle
        .update(vcx, |shell, _, cx| {
            shell.status_messages(cx, |messages, _| {
                messages
                    .activities()
                    .iter()
                    .map(|activity| activity.text.to_string())
                    .collect()
            })
        })
        .unwrap()
}

#[test]
fn ey264_a_drag_held_on_no_target_says_where_a_folder_would_take_it() {
    let root = Root::new("ey264");
    let mut cx = bench::real_cx();
    let handle = mounted(&mut cx, &root);
    let file = centre(&mut cx, handle, 1);
    let blank = point(file.x, file.y + px(160.));
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    hold(&mut vcx, file, blank, Modifiers::default());
    let move_line = "Move 1 item to a folder · ctrl copies, shift moves".to_owned();
    assert!(
        lines(&mut vcx, handle).contains(&move_line),
        "plain keys move"
    );
    let ground = |vcx: &mut VisualTestContext| -> Vec<String> {
        lines(vcx, handle)
            .into_iter()
            .filter(|line| line.contains("to a folder"))
            .collect()
    };
    hold_keys(&mut vcx, blank, Modifiers::control());
    assert_eq!(ground(&mut vcx), ["Copy 1 item to a folder"]);
    hold_keys(&mut vcx, blank, Modifiers::control_shift());
    assert_eq!(ground(&mut vcx), ["Link 1 item to a folder"]);
    vcx.simulate_mouse_up(blank, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    let after = lines(&mut vcx, handle);
    assert!(
        !after.iter().any(|line| line.contains("to a folder")),
        "the release takes the line away: {after:?}"
    );
}

// Green only: the frame lives on PathView.drop_target, which the base tree does not have.
#[test]
fn ey271_the_held_crumb_takes_the_drop_frame_and_the_release_clears_it() {
    let root = Root::new("ey271frame");
    let parent = root.src().trim_end_matches("/src").to_owned();
    let mut cx = bench::real_cx();
    let (handle, _lines) = open_listed(&mut cx, root.src());
    let file = centre(&mut cx, handle, 1);
    let (target, _) = crumb(&mut cx, handle, &parent);
    let framed = |vcx: &mut VisualTestContext| {
        handle
            .update(vcx, |shell, _, cx| {
                let chrome = shell.chrome.view.as_ref().unwrap().read(cx);
                chrome.path.read(cx).drop_target
            })
            .unwrap()
    };
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    hold(&mut vcx, file, target, Modifiers::default());
    let held = framed(&mut vcx);
    let parent_index = handle
        .read_with(&vcx, |shell, cx| {
            let view = shell.chrome.view.as_ref().unwrap().read(cx).path.read(cx);
            view.strips[0]
                .crumbs()
                .iter()
                .position(|crumb| crumb.path == parent)
        })
        .unwrap();
    assert_eq!(
        held,
        parent_index.map(|index| (0, index)),
        "the parent crumb under the pointer takes the frame"
    );
    // v0.3.12 washes the whole crumb, separator included: the painted wash spans every x hit gives the crumb.
    let (slot, index) = held.unwrap();
    let (quads, scale) = vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        (window.painted_quads(), window.scale_factor())
    });
    let (want, text) = handle
        .update(&mut vcx, |shell, _, cx| {
            let chrome = shell.chrome.view.as_ref().unwrap().read(cx);
            let view = chrome.path.read(cx);
            let paint = view.paint.borrow();
            let bounds = paint.strips[slot].unwrap();
            let strip = &view.strips[slot];
            let (left, width) = strip.extent(index).unwrap();
            assert_eq!(strip.hit(left + width - 0.5), Hit::Crumb(index));
            assert_ne!(strip.hit(left + width + 0.5), Hit::Crumb(index));
            let x = f32::from(bounds.left()) + strip_pad(slot != 0, &paint.scale()) + left;
            let y = f32::from(bounds.top());
            (
                [x, y, width].map(|v| v * scale),
                strip.crumbs()[index].text.clone(),
            )
        })
        .unwrap();
    assert!(
        text.ends_with('/'),
        "the held crumb draws its separator: {text}"
    );
    let washes: Vec<_> = quads
        .iter()
        .map(|q| [q.bounds.origin.x, q.bounds.origin.y, q.bounds.size.width].map(|v| v.as_f32()))
        .filter(|got| {
            got.iter()
                .zip(want)
                .all(|(got, want)| (got - want).abs() <= 1.)
        })
        .collect();
    assert!(
        !washes.is_empty(),
        "no wash spans the whole crumb {text}, want x y width {want:?}"
    );
    vcx.simulate_mouse_move(file, Some(MouseButton::Left), Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(framed(&mut vcx), None, "leaving the crumb clears the frame");
    vcx.simulate_mouse_move(target, Some(MouseButton::Left), Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(framed(&mut vcx), held, "coming back frames the crumb again");
    vcx.simulate_mouse_up(target, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(framed(&mut vcx), None, "the release clears the frame");
}
