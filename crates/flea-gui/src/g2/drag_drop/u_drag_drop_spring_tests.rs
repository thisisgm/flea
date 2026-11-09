//! EY-264 after a spring open: the folder now under the still pointer is the lit target, the footer names it and the drop lands there.
use super::mounted_tests::{centre, drain, frame, Root};
use super::vf3_tests::open;
use crate::{list::bench, state::Shell};
use gpui::{point, px, Modifiers, MouseButton, VisualTestContext, WindowHandle};
use std::time::Duration;

fn aimed(
    vcx: &mut VisualTestContext,
    handle: WindowHandle<Shell>,
) -> (Option<(usize, String)>, Vec<String>) {
    handle
        .update(vcx, |shell, window, cx| {
            let states = shell.list_states(0, window, cx);
            let target = states
                .drop_target
                .zip(states.drop_label.map(|label| label.to_string()));
            let lines = shell.status_messages(cx, |messages, _| {
                messages
                    .activities()
                    .iter()
                    .map(|activity| activity.text.to_string())
                    .collect()
            });
            (target, lines)
        })
        .unwrap()
}

#[test]
fn ey264_a_folder_sprung_open_under_a_still_pointer_lights_the_row_now_under_it() {
    let root = Root::new("ey264spring");
    let alpha = format!("{}/alpha", root.src());
    std::fs::create_dir(format!("{alpha}/empty")).unwrap();
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
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(from, None, Modifiers::default());
    vcx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(
        point(from.x + px(20.), from.y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    vcx.simulate_mouse_move(folder, Some(MouseButton::Left), Modifiers::default());
    vcx.run_until_parked();
    vcx.executor().advance_clock(Duration::from_millis(300));
    vcx.run_until_parked();
    let asked = drain(&mut lines);
    assert!(
        asked
            .iter()
            .any(|line| line.contains(r#""c":"list""#) && line.contains(&alpha)),
        "the rest asks for alpha's listing: {asked:?}"
    );
    handle
        .update(&mut vcx, |shell, window, cx| {
            // The spring open's first rows carry the next listing serial after the folder it left.
            let listing = shell.panes[0].as_ref().unwrap().pane.held.listing + 1;
            let rows = vec![crate::window::columns_tests::host::row("empty", true)];
            shell.receive(
                0,
                crate::backend::Event::Reply(flea::protocol::Reply::Listed {
                    path: alpha.clone(),
                    total: rows.len(),
                }),
                window,
                cx,
            );
            shell.receive(
                0,
                crate::backend::Event::Reply(flea::protocol::Reply::Rows {
                    start: 0,
                    listing,
                    rows,
                }),
                window,
                cx,
            );
        })
        .unwrap();
    vcx.executor().advance_clock(Duration::from_millis(200));
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    vcx.run_until_parked();
    let path = handle
        .read_with(&vcx, |shell, _| {
            shell.panes[0].as_ref().unwrap().path.clone()
        })
        .unwrap();
    assert_eq!(path, alpha, "the rest opened alpha");
    let (target, said) = aimed(&mut vcx, handle);
    assert_eq!(target, Some((0, "move here".to_owned())), "{said:?}");
    assert!(
        said.contains(&"Move 1 item to empty · ctrl copies, shift moves".to_owned()),
        "{said:?}"
    );
    vcx.simulate_mouse_up(folder, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    let sent = drain(&mut lines);
    assert!(
        sent.iter()
            .any(|line| line.contains("collisions") && line.contains(&format!("{alpha}/empty"))),
        "the drop lands in the folder under the pointer: {sent:?}"
    );
}
