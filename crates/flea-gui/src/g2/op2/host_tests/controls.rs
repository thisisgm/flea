//! Flash control readers through a mounted Root, with clicks at the returned painted positions.
use super::*;
use gpui::{point, px, Modifiers, MouseButton};

fn centre(cx: &mut TestAppContext, handle: Handle, name: &str) -> String {
    edit(cx, handle, |shell, window, cx| {
        let (answer, receive) = std::sync::mpsc::sync_channel(1);
        shell.ipc(
            Event::Call {
                request: Request::Call {
                    target: "flea".into(),
                    function: "flashControlCentre".into(),
                    arguments: vec![name.into()],
                },
                answer,
            },
            window,
            cx,
        );
        let reply = receive.try_recv().unwrap().reply;
        assert_eq!(reply.code, 0, "{}", reply.stderr);
        reply.stdout.trim_end().to_owned()
    })
}

fn painted(cx: &mut TestAppContext, handle: Handle, selector: &'static str) -> String {
    let bounds = VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds(selector)
        .expect("the control was painted");
    let x = f32::from(bounds.center().x);
    let y = f32::from(bounds.center().y);
    format!("{} {}", (x + 0.5).floor() as i64, (y + 0.5).floor() as i64)
}

fn click(cx: &mut TestAppContext, handle: Handle, location: &str) {
    let coordinates: Vec<f32> = location.split_whitespace().map(|n| n.parse().unwrap()).collect();
    assert_eq!(coordinates.len(), 2);
    let point = point(px(coordinates[0]), px(coordinates[1]));
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_mouse_down(point, MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_up(point, MouseButton::Left, Modifiers::none());
    frame(cx, handle);
}

#[test]
fn pb_devices_ipc_flash_controls_closed_and_unknown_are_empty() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, _, _) = start(&mut cx);
    for name in ["flash", "cancel", "end", "unknown"] {
        assert_eq!(centre(&mut cx, handle, name), "");
    }
    choose(&mut cx, handle, 44, "flashDrive", "", &["/files/disk.img"]);
    assert_eq!(centre(&mut cx, handle, "unknown"), "");
    assert_eq!(centre(&mut cx, handle, "cancel"), "");
    assert_eq!(centre(&mut cx, handle, "end"), "");
    let expected = painted(&mut cx, handle, "op2-flash-action");
    assert_eq!(centre(&mut cx, handle, "flash"), expected);
}

#[test]
fn pb_devices_ipc_flash_controls_follow_flash_cancel_end_and_hide() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _) = start(&mut cx);
    choose(&mut cx, handle, 44, "flashDrive", "", &["/files/disk.img"]);
    receive(&mut cx, handle, 0, r#"{"t":"flashimage","path":"/files/disk.img","kind":"img","bytes":100,"unpacked":100,"known":true,"member":""}"#);
    receive(&mut cx, handle, 0, r#"{"t":"flashtargets","drives":[{"dev":"sdz","seq":3,"serial":"S1","label":"STICK","model":"Fixture","bytes":4096,"volumes":[],"refused":""}]}"#);
    drain(&mut requests);
    let flash = centre(&mut cx, handle, "flash");
    assert_eq!(flash, painted(&mut cx, handle, "op2-flash-action"));
    click(&mut cx, handle, &flash);
    assert!(drain(&mut requests).iter().any(|line| line.starts_with(r#"{"c":"flash","id":1,"#)));
    receive(&mut cx, handle, 0, r#"{"t":"flashstarted","id":31,"total":100,"known":true}"#);
    let cancel = centre(&mut cx, handle, "cancel");
    assert_eq!(cancel, painted(&mut cx, handle, "op2-flash-cancel"));
    assert_eq!(centre(&mut cx, handle, "flash"), "");
    click(&mut cx, handle, &cancel);
    assert!(drain(&mut requests).iter().any(|line| line.contains(r#""c":"flashcancel""#)));
    receive(&mut cx, handle, 0, r#"{"t":"flashdone","id":31,"ok":false,"phase":"write","at":0,"err":"cancelled","blank":false}"#);
    assert_eq!(centre(&mut cx, handle, "end"), painted(&mut cx, handle, "op2-flash-end"));
    assert_eq!(centre(&mut cx, handle, "cancel"), "");
    keys(&mut cx, handle, "escape");
    for name in ["flash", "cancel", "end"] {
        assert_eq!(centre(&mut cx, handle, name), "");
    }
}
