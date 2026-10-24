use super::store::Update;
use crate::{
    backend::Backend,
    compositor::{tests::Fixture, Service},
    window::root::Root,
};
use gpui::{px, size, AppContext, TestAppContext};
use std::{io::Write, os::unix::net::UnixListener, sync::mpsc, time::Duration};

#[gpui::test]
fn m9_corner_configreloaded_repaints_settings_controls(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture.answer(r#"{"int":0}"#);
    let socket = fixture.socket();
    let listener = UnixListener::bind(&socket).unwrap();
    let (send, updates) = mpsc::channel();
    let _service = Service::at(Some(socket), fixture.command(0), move |radius| {
        send.send(Update::Corner(radius)).is_ok()
    })
    .unwrap();
    let (mut stream, _) = listener.accept().unwrap();
    let (backend, _requests) = Backend::test_channel();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let shell = cx.new(|cx| {
            let mut shell = crate::state::tests::shell_with(backend, cx);
            shell.launch.settings.data = flea::uischema::defaults();
            shell.open_settings(window, cx);
            shell
                .settings
                .panel
                .as_ref()
                .unwrap()
                .update(cx, |view, _| view.panel.show_section("display"));
            shell
        });
        Root { shell }
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    let shell = root.read_with(cx, |root, _| root.shell.clone());
    for (index, (answer, radius)) in [
        (r#"{"int":0}"#, 0.),
        (r#"{"int":8}"#, 8.),
        (r#"{"int":0}"#, 0.),
        (r#"{"int":8}"#, 8.),
        ("garbage", 0.),
    ]
    .into_iter()
    .enumerate()
    {
        if index > 0 {
            fixture.answer(answer);
            stream
                .write_all(b"activewindow>>caf\xe9\nconfigreloaded>>\n")
                .unwrap();
        }
        let update = updates.recv_timeout(Duration::from_secs(5)).unwrap();
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| shell.receive_settings(update, window, cx))
        });
        cx.run_until_parked();
        let (scale, quads) = cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            (window.scale_factor(), window.painted_quads())
        });
        shell.read_with(cx, |shell, cx| {
            assert_eq!(shell.corner, radius);
            assert_eq!(
                shell
                    .settings
                    .panel
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .panel
                    .facts
                    .corner,
                radius
            );
        });
        assert!(
            quads.iter().any(|quad| {
                quad.bounds.size.width.0 / scale == 18.
                    && quad.bounds.size.height.0 / scale == 18.
                    && quad.corner_radii.top_left.0 / scale == radius / 2.
            }),
            "Settings checkbox must paint half the live radius {radius}"
        );
        assert!(
            quads.iter().any(|quad| {
                quad.bounds.size.height.0 / scale == 24.
                    && quad.corner_radii.top_left.0 / scale == radius
            }),
            "Settings chip must paint the live radius {radius}"
        );
    }
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_sheet(window, cx)));
    fixture.answer(r#"{"int":8}"#);
    stream.write_all(b"configreloaded>>\n").unwrap();
    let update = updates.recv_timeout(Duration::from_secs(5)).unwrap();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive_settings(update, window, cx);
            assert_eq!(shell.settings.sheet.as_ref().unwrap().read(cx).corner, 8.);
        })
    });
    cx.run_until_parked();
    let frame = cx
        .debug_bounds("sheet-card")
        .expect("sheet must draw its card");
    let (scale, quads) = cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        (window.scale_factor(), window.painted_quads())
    });
    let own: Vec<_> = quads
        .iter()
        .filter(|quad| quad.bounds == frame.scale(scale) && quad.border_widths.top.0 > 0.)
        .collect();
    assert!(
        !own.is_empty(),
        "the sheet must paint its own framed bounds"
    );
    for quad in own {
        let radii = quad.corner_radii;
        for radius in [
            radii.top_left,
            radii.top_right,
            radii.bottom_left,
            radii.bottom_right,
        ] {
            assert_eq!(radius.0 / scale, 8.);
        }
    }
}
