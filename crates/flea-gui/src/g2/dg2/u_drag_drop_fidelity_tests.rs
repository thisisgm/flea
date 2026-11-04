//! Native-discovered drag hit boundaries that the original mounted fixtures missed.
use super::mounted_tests::{centre, drain, transfer, Root};
use super::u_drag_drop_tests::{crumb, open_listed};
use crate::list::bench;
use crate::path::paint::{strip_pad, Size};
use gpui::{point, px, Modifiers, MouseButton, VisualTestContext};

#[test]
fn ey271_a_parent_crumb_accepts_the_trailing_separator_that_qt_accepts() {
    let root = Root::new("ey271separator");
    let parent = root.src().trim_end_matches("/src").to_owned();
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open_listed(&mut cx, root.src());
    // The slash's own span comes from shaping the crumb with and without it, not from the extent under test.
    let target = handle
        .update(&mut cx, |shell, window, cx| {
            let view = shell.chrome.view.as_ref().unwrap().read(cx).path.read(cx);
            let strip = &view.strips[0];
            let index = strip
                .crumbs()
                .iter()
                .position(|crumb| crumb.path == parent)
                .unwrap();
            let text = strip.crumbs()[index].text.clone();
            let name = text
                .strip_suffix('/')
                .expect("the parent crumb draws its separator");
            let mut paint = view.paint.borrow_mut();
            let shaped = |paint: &mut crate::path::paint::Paint, piece: &str| {
                f32::from(
                    paint
                        .shape(piece, Size::Caption, false, window)
                        .line()
                        .width,
                )
            };
            let (before, after) = (shaped(&mut paint, name), shaped(&mut paint, &text));
            let (left, _) = strip.extent(index).unwrap();
            let bounds = paint.strips[0].unwrap();
            point(
                bounds.left() + px(strip_pad(false, &paint.scale()) + left + (before + after) / 2.),
                bounds.top() + bounds.size.height / 2.,
            )
        })
        .unwrap();
    let file = centre(&mut cx, handle, 1);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    super::rail_drop_tests::hold(&mut vcx, file, target, Modifiers::default());
    drain(&mut lines);
    vcx.simulate_mouse_up(target, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    let sent = transfer(&mut vcx, handle, &mut lines);
    assert!(sent.contains(&format!(r#""dest":"{parent}""#)), "{sent}");
}

#[test]
fn ey271_a_drag_held_over_a_parent_crumb_says_the_move_and_its_drop_moves_there() {
    let root = Root::new("ey271");
    let parent = root.src().trim_end_matches("/src").to_owned();
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open_listed(&mut cx, root.src());
    let file = centre(&mut cx, handle, 1);
    let (target, words) = crumb(&mut cx, handle, &parent);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    super::rail_drop_tests::hold(&mut vcx, file, target, Modifiers::default());
    let held = handle
        .update(&mut vcx, |shell, _, cx| {
            shell.status_messages(cx, |messages, _| {
                messages
                    .activities()
                    .iter()
                    .map(|activity| activity.text.clone())
                    .collect::<Vec<_>>()
            })
        })
        .unwrap();
    assert!(
        held.contains(&format!(
            "Move 1 item to {words} · ctrl copies, shift moves"
        )),
        "{held:?}"
    );
    drain(&mut lines);
    vcx.simulate_mouse_up(target, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    let transfer = transfer(&mut vcx, handle, &mut lines);
    assert!(transfer.contains(r#""op":"move""#), "{transfer}");
    assert!(
        transfer.contains(&format!(r#""dest":"{parent}""#)),
        "{transfer}"
    );
    assert!(
        transfer.contains(&format!("{}/b.txt", root.src())),
        "{transfer}"
    );
}
