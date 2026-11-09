//! Glass is observed through the real mounted Shell and its ordinary IPC dispatch.
#[path = "glass/material_startup_controls.rs"]
mod material_startup_controls;
#[path = "glass/paint_pass_controls.rs"]
mod paint_pass_controls;
#[path = "glass/producer_controls.rs"]
mod producer_controls;
#[path = "glass/visual_alpha_controls.rs"]
mod visual_alpha_controls;
use super::startup::View;
use crate::{backend::Backend, ipc::server::Event, presentation::fixture::Fixture, state::Shell};
use flea::{
    ipc::wire::{Reply, Request},
    jsondoc::Json,
};
use gpui::{AppContext, TestAppContext, VisualTestContext, Window, WindowHandle};
use std::sync::Arc;

fn launch(cx: &mut TestAppContext) -> WindowHandle<super::root::Root> {
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, cx| {
                let (backend, requests) = Backend::test_channel();
                cx.on_release(move |_, _| drop(requests)).detach();
                Ok(backend)
            },
            monitor: None,
        });
        cx.set_global(crate::state::constructor_fixture::NoSettingsWorker);
        let settings = super::startup::Settings::from_json(
            flea::jsondoc::parse(r#"{"view":"list"}"#).unwrap(),
        );
        let launch = super::startup::Launch::resolve(
            settings,
            super::startup::Arguments {
                path: Some("/files".into()),
                select: None,
            },
            "/home/test",
        );
        let locale = super::locale::Snapshot {
            revision: 1,
            catalog: Arc::new(crate::test_catalogs::english()),
        };
        let (_, events) = async_channel::unbounded();
        super::open(launch, locale, events, cx).expect("the production startup opens")
    })
}

fn call(shell: &mut Shell, window: &mut gpui::Window, cx: &mut gpui::Context<Shell>) -> Reply {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    shell.ipc(
        Event::Call {
            request: Request::Call {
                target: "flea".into(),
                function: "glass".into(),
                arguments: vec![],
            },
            answer,
        },
        window,
        cx,
    );
    receive
        .try_recv()
        .expect("the mounted shell answers glass")
        .reply
}

/// The parsed `glass` reply of the mounted window, read through the ordinary IPC call.
fn glass_value(handle: WindowHandle<super::root::Root>, cx: &mut TestAppContext) -> Json {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.ensure_ipc_reader(window, cx);
                let reply = call(shell, window, cx);
                assert_eq!(
                    reply.code, 0,
                    "glass must read the production window: {reply:?}"
                );
                flea::jsondoc::parse(reply.stdout.trim()).unwrap()
            })
        })
        .unwrap()
}

fn focused(value: &Json) -> Option<bool> {
    value.get("focused").and_then(Json::as_bool)
}

fn stop(handle: WindowHandle<super::root::Root>, cx: &mut TestAppContext) {
    handle
        .update(cx, |root, _, cx| {
            root.shell.update(cx, |shell, _| shell.stop_for_app_quit())
        })
        .unwrap();
}

fn draw(handle: WindowHandle<super::root::Root>, cx: &mut TestAppContext) {
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
}

/// Writes the colour input and each `(index, text)` input, then installs what production resolves from them.
fn install(cx: &mut TestAppContext, name: &str, files: &[(usize, &str)]) -> Fixture {
    let fixture = Fixture::new(name);
    let paths: Vec<_> = (0..7)
        .map(|at| fixture.path().join(format!("input-{at}")))
        .collect();
    fixture.write(&paths[0], "background='#101315'\nforeground='#cacccc'");
    for (at, text) in files {
        fixture.write(&paths[*at], text);
    }
    let snapshot =
        crate::presentation::Snapshot::glass_fixture(&paths, fixture.path().join("font-cache"));
    cx.update(|cx| snapshot.install(cx));
    fixture
}

/// Launches once over the installed inputs and returns the parsed glass reply.
fn read_glass(cx: &mut TestAppContext, name: &str, files: &[(usize, &str)]) -> Json {
    let fixture = install(cx, name, files);
    let handle = launch(cx);
    let value = glass_value(handle, cx);
    stop(handle, cx);
    fixture.cleanup();
    value
}

/// Grounds in the theme background at least half the window tall, at any width, off the left edge, as (opaque, translucent): pane floors and Columns tracks, never the rail or the clear canvas.
fn background_grounds(window: &Window, background: gpui::Hsla) -> (usize, usize) {
    let full_h = f32::from(window.viewport_size().height) * window.scale_factor();
    let want = gpui::Rgba::from(background);
    let same = |color: &gpui::Hsla| {
        let got = gpui::Rgba::from(*color);
        (got.r - want.r).abs() < 1. / 255.
            && (got.g - want.g).abs() < 1. / 255.
            && (got.b - want.b).abs() < 1. / 255.
    };
    let mut counts = (0, 0);
    for quad in window.painted_quads().iter() {
        let tall =
            quad.bounds.size.height.as_f32() >= full_h / 2. && quad.bounds.origin.x.as_f32() > 0.;
        match quad.background.as_solid() {
            Some(color) if tall && same(&color) && color.a == 1. => counts.0 += 1,
            Some(color) if tall && same(&color) && color.a > 0. => counts.1 += 1,
            _ => {}
        }
    }
    counts
}

#[gpui::test]
fn op_glass_startup_chooses_clear_before_mount_and_reads_actual_focus(cx: &mut TestAppContext) {
    let handle = launch(cx);
    let value = glass_value(handle, cx);
    for name in ["clear", "automatic"] {
        assert_eq!(
            value.get(name).and_then(Json::as_bool),
            Some(true),
            "{name}"
        );
    }
    assert_eq!(
        value.get("mode").and_then(Json::as_str),
        Some("transparent")
    );
    assert_eq!(value.get("alpha").and_then(Json::as_f64), Some(1.));
    assert_eq!(value.get("listAlpha").and_then(Json::as_f64), Some(1.));
    // A fresh test window starts inactive, so the reader starts unfocused before the test activates it.
    assert_eq!(focused(&value), Some(false));
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    assert_eq!(focused(&glass_value(handle, cx)), Some(true));
    visual.deactivate_window();
    assert_eq!(focused(&glass_value(handle, cx)), Some(false));
    stop(handle, cx);
}

#[gpui::test]
fn op_glass_unregistered_window_refuses_to_invent_surface_state(cx: &mut TestAppContext) {
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let (backend, _) = Backend::test_channel();
        let shell = cx.new(|cx| crate::state::tests::shell_with(backend, cx));
        shell.update(cx, |shell, cx| {
            let reply = call(shell, window, cx);
            assert_eq!(
                reply,
                Reply::error(3, "FLEA_IPC_UNAVAILABLE flea.glass: window not mounted")
            );
        });
    });
}

#[gpui::test]
fn op_glass_reads_real_source_overrides_and_opaque_launch_latch(cx: &mut TestAppContext) {
    let fixture = crate::presentation::fixture::Fixture::new("op-glass");
    let paths: Vec<_> = (0..7)
        .map(|at| fixture.path().join(format!("input-{at}")))
        .collect();
    fixture.write(&paths[0], "background='#101315'\nforeground='#cacccc'");
    fixture.write(&paths[1], "[flea]\nbackground-alpha=0.34");
    fixture.write(&paths[2], "[flea]\nbackground-alpha=0.999");
    let snapshot =
        crate::presentation::Snapshot::glass_fixture(&paths, fixture.path().join("font-cache"));
    cx.update(|cx| snapshot.install(cx));
    let handle = launch(cx);
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.ensure_ipc_reader(window, cx);
                let reply = call(shell, window, cx);
                assert_eq!(
                    reply.code, 0,
                    "opaque startup must still expose mounted truth: {reply:?}"
                );
                let value = flea::jsondoc::parse(reply.stdout.trim()).unwrap();
                assert_eq!(value.get("clear").and_then(Json::as_bool), Some(false));
                assert_eq!(value.get("early").and_then(Json::as_bool), Some(false));
                assert_eq!(value.get("automatic").and_then(Json::as_bool), Some(false));
                assert_eq!(value.get("alpha").and_then(Json::as_f64), Some(1.));
                shell.stop_for_app_quit();
            });
        })
        .unwrap();
    fixture.cleanup();
}

#[gpui::test]
fn op_glass_explicit_alpha_reaches_the_painted_chrome_without_an_opaque_canvas(
    cx: &mut TestAppContext,
) {
    let fixture = crate::presentation::fixture::Fixture::new("op-glass-ground");
    let paths: Vec<_> = (0..7)
        .map(|at| fixture.path().join(format!("input-{at}")))
        .collect();
    fixture.write(&paths[0], "background='#101315'\nforeground='#cacccc'");
    fixture.write(&paths[2], "[flea]\nbackground-alpha=0.34");
    let snapshot =
        crate::presentation::Snapshot::glass_fixture(&paths, fixture.path().join("font-cache"));
    cx.update(|cx| snapshot.install(cx));
    let handle = launch(cx);
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    handle
        .update(cx, |root, window, cx| {
            let width = f32::from(window.viewport_size().width) * window.scale_factor();
            let height = f32::from(window.viewport_size().height) * window.scale_factor();
            let quads = window.painted_quads();
            assert!(
                !quads.iter().any(|quad| {
                    (quad.bounds.size.width.as_f32() - width).abs() < 1.
                        && (quad.bounds.size.height.as_f32() - height).abs() < 1.
                        && quad
                            .background
                            .as_solid()
                            .is_some_and(|color| color.a == 1.)
                }),
                "the actual glass scene still paints an opaque whole-window canvas"
            );
            assert!(
                quads.iter().any(|quad| {
                    (quad.bounds.size.width.as_f32() - width).abs() < 1.
                        && quad.bounds.origin.y.as_f32() == 0.
                        && quad
                            .background
                            .as_solid()
                            .is_some_and(|color| (color.a - 87. / 255.).abs() < 0.0001)
                }),
                "the actual chrome must paint one stepped ground"
            );
            root.shell.update(cx, |shell, _| shell.stop_for_app_quit());
        })
        .unwrap();
    fixture.cleanup();
}

#[gpui::test]
fn op_glass_automatic_glassy_reads_actual_independent_material_grounds(cx: &mut TestAppContext) {
    let value = read_glass(cx, "op-glass-automatic", &[(6, "")]);
    assert_eq!(value.get("mode").and_then(Json::as_str), Some("glassy"));
    assert_eq!(value.get("automatic").and_then(Json::as_bool), Some(true));
    assert_eq!(value.get("material").and_then(Json::as_bool), Some(true));
    assert!((value.get("alpha").and_then(Json::as_f64).unwrap() - 192. / 255.).abs() < 0.0001);
    assert!(
        value.get("listAlpha").and_then(Json::as_f64).unwrap()
            >= value.get("alpha").and_then(Json::as_f64).unwrap()
    );
}

#[gpui::test]
fn op_glass_toggle_files_are_index_five_opaque_and_index_six_glassy(cx: &mut TestAppContext) {
    let opaque = read_glass(cx, "op-glass-opaque", &[(5, "")]);
    assert_eq!(opaque.get("mode").and_then(Json::as_str), Some("opaque"));
    let glassy = read_glass(cx, "op-glass-glassy", &[(6, "")]);
    assert_eq!(glassy.get("mode").and_then(Json::as_str), Some("glassy"));
}

#[gpui::test]
fn op_glass_list_and_columns_panes_draw_the_user_alpha(cx: &mut TestAppContext) {
    let fixture = install(
        cx,
        "op-glass-columns",
        &[(2, "[flea]\nbackground-alpha=0.34")],
    );
    let handle = launch(cx);
    draw(handle, cx);
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, _| {
                let host = shell.panes[0].as_ref().expect("the launch pane");
                assert_eq!(host.drawn_view, Some(View::List));
                assert!(!host.drawn_list, "the List pane must not be Ready");
            });
            // The control: the explicit alpha is live in this window.
            assert!(
                super::glass::alpha(window, cx) < 1.,
                "explicit alpha must reach the window"
            );
            let background = cx
                .try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default()
                .background;
            // v0.3.12 Pane.qml draws Glass.listBackground, the user's alpha, so the wallpaper shows through the panes too.
            let (opaque, translucent) = background_grounds(window, background);
            assert_eq!(
                opaque, 0,
                "a List pane with no rows must not paint an opaque ground"
            );
            assert!(
                translucent > 0,
                "a List pane with no rows must paint the translucent list ground"
            );
        })
        .unwrap();
    handle
        .update(cx, |root, window, cx| {
            root.shell
                .update(cx, |shell, cx| shell.set_view(View::Columns, window, cx));
        })
        .unwrap();
    draw(handle, cx);
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, _| {
                let host = shell.panes[0].as_ref().expect("the columns pane");
                assert_eq!(host.drawn_view, Some(View::Columns));
                assert!(!host.drawn_list);
            });
            let background = cx
                .try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default()
                .background;
            let (opaque, translucent) = background_grounds(window, background);
            assert_eq!(opaque, 0, "no Columns track may paint an opaque ground");
            assert!(
                translucent > 0,
                "the Columns tracks must paint the translucent list ground"
            );
        })
        .unwrap();
    stop(handle, cx);
    fixture.cleanup();
}

#[path = "glass_grid_tests.rs"]
mod grid;
#[path = "glass_tab_tests.rs"]
mod tab;

#[gpui::test]
fn glass_automatic_alpha_matches_v0312(cx: &mut TestAppContext) {
    let value = read_glass(cx, "sol-glassy-ground-alpha", &[(6, "")]);
    assert_eq!(value.get("mode").and_then(Json::as_str), Some("glassy"));
    let alpha = value.get("alpha").and_then(Json::as_f64).unwrap();
    assert_eq!((alpha * 255.).round() as u8, 192, "{value:?}");
}
