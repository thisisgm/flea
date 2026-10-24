use crate::{
    backend::Backend,
    window::{root::Root, settings::store::Update},
};
use gpui::{AppContext, TestAppContext};

#[gpui::test]
fn m9_corner_visual_scene_stays_sticky_after_service_updates(cx: &mut TestAppContext) {
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
    let shell = root.read_with(cx, |root, _| root.shell.clone());
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive_settings(Update::Corner(8.), window, cx);
            assert_eq!(shell.corner, 8., "an ordinary window follows the service");
        })
    });
    for radius in [0., 8., 0.] {
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                let spec = super::contract::Spec::parse(
                    &std::process::id().to_string(),
                    &format!("g1-01-list-r{radius}"),
                    r#"{"path":"/fixture"}"#,
                )
                .unwrap();
                let (answer, _receive) = std::sync::mpsc::sync_channel(1);
                shell.apply_scene(spec, answer, window, cx);
                // Completion or expiry of the receipt must not release the window's explicit corner.
                shell.visual.pending = None;
                shell.visual.deadline = None;
                for live in [8. - radius, radius, 8. - radius] {
                    shell.receive_settings(Update::Corner(live), window, cx);
                    assert_eq!(shell.corner, radius);
                    assert_eq!(
                        crate::compositor::corner(cx),
                        live,
                        "the app global must keep the compositor value, never this window's scene corner"
                    );
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
                }
            })
        });
        cx.run_until_parked();
        let (scale, quads) = cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            (window.scale_factor(), window.painted_quads())
        });
        assert!(
            quads
                .iter()
                .any(|quad| quad.bounds.size.width.0 / scale == 18.
                    && quad.bounds.size.height.0 / scale == 18.
                    && quad.corner_radii.top_left.0 / scale == radius / 2.),
            "scene checkbox must keep radius {radius}"
        );
    }
}
