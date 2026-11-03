use super::{
    live::{Live, Model},
    seam::Host,
    session::Session,
    tests::{frame, load, open, stl_box, with, DARK},
    view::Phase,
};
use flea::ui_logic_pv4::{
    camera,
    color::Palette,
    control::{Action, Surface, Viewer},
    motion::{EASE_IN, TURN_SECONDS},
    raster::{Prepared, Raster, Style},
    render_thumbnail,
    scene::Scene,
    wire,
};
use gpui::TestAppContext;

fn reply(slot: &str, token: u64, state: &str, format: &str, picture: &str) -> String {
    format!(
        r#"{{"t":"model3d","slot":"{slot}","token":{token},"state":"{state}","file":"/c/part.3d","thumbnail":"{picture}","data":{{"format":"{format}","plates":[{{}},{{}}]}}}}"#
    )
}

#[test]
fn m9_g2rows_16_shared_scene_raster_thumbnail_and_wire_contract() {
    let scene = stl_box([0.; 3], [60., 40., 40.]);
    let thumb = render_thumbnail(&scene, 0, 64, 64, DARK).unwrap();
    let prepared = Prepared::new(&scene, DARK);
    let camera = camera::thumbnail(&scene, 0, 64., 64.).unwrap();
    let mut raster = Raster::default();
    raster.resize(64, 64, 2);
    raster.render(
        &scene,
        &prepared,
        0,
        &camera,
        &Style {
            palette: DARK,
            wire: None,
            layer: None,
            plate: None,
            ground: None,
        },
    );
    let mut rgba = Vec::new();
    raster.rgba_into(&mut rgba);
    assert_eq!(thumb, rgba, "TUI and Grid consume the same RGBA raster");
    assert_eq!(rgba.len(), 64 * 64 * 4);
    assert_eq!(rgba[3], 0);
    assert!(rgba.as_chunks::<4>().0.iter().any(|p| p[3] == 255));
    let light = Palette {
        foreground: [0.2, 0.2, 0.25],
        ..DARK
    };
    assert_ne!(thumb, render_thumbnail(&scene, 0, 64, 64, light).unwrap());
    let mut authored = scene.clone();
    authored.colors = vec![Some([240, 90, 10]); authored.triangles.len()];
    assert_eq!(
        render_thumbnail(&authored, 0, 64, 64, DARK),
        render_thumbnail(&authored, 0, 64, 64, light)
    );
    let mut invalid = scene.clone();
    invalid.triangles[0][0] = u32::MAX;
    assert!(render_thumbnail(&invalid, 0, 64, 64, DARK).is_none());
    assert!(render_thumbnail(&Scene::default(), 0, 64, 64, DARK).is_none());
    let mut terminal = Viewer::new(Surface::Terminal, &scene, 0, 64., 64.).unwrap();
    terminal.opened(0., true);
    assert!(!terminal.spinning());
    assert!(terminal.act(Action::OrbitLeft, 0., true));
    terminal.advance(1.);
    assert_ne!(terminal.view, terminal.home);
    assert_eq!((TURN_SECONDS, EASE_IN), (24., 0.6));
    let line = wire::request("terminal", 17, "/p/a\"b.stl", Some(1));
    let doc = flea::jsondoc::parse(&line).unwrap();
    assert_eq!(
        doc.get("path").and_then(|v| v.as_str()),
        Some("/p/a\"b.stl")
    );
    let mut session = Session::new("terminal");
    session.open("/p/part.stl", None);
    let token = session.token();
    assert!(!session.accept(&wire::parse(&reply("look", token, "ready", "stl", "")).unwrap()));
    assert_eq!(session.close(), Some(wire::cancel("terminal", token)));
    assert!(!session.accept(&wire::parse(&reply("terminal", token, "ready", "stl", "")).unwrap()));
}

#[gpui::test]
fn m9_g2rows_16_all_formats_host_and_cancel_by_token_slot(cx: &mut TestAppContext) {
    for format in ["stl", "3mf", "obj", "ply", "amf", "gcode", "bgcode"] {
        for surface in [Surface::QuickLook, Surface::Column] {
            let (host, _) = open(cx, surface, true);
            let path = format!("/p/part.{format}");
            let file = if matches!(format, "gcode" | "bgcode") {
                "/c/print.3d"
            } else {
                "/c/part.3d"
            };
            load(cx, host, &path, file, format);
            assert_eq!(
                with(cx, host, |view, _| view.phase.clone()),
                Phase::Ready,
                "{format} {surface:?}"
            );
            frame(cx, host);
            if matches!(format, "gcode" | "bgcode") {
                assert!(with(cx, host, |view, cx| view.act("pageDown", cx)));
                assert_eq!(
                    with(cx, host, |view, _| view
                        .live
                        .borrow()
                        .viewer
                        .as_ref()
                        .unwrap()
                        .layer),
                    Some(238)
                );
            }
            with(cx, host, |view, cx| view.show("/p/next.3mf", 0, cx));
            let old = with(cx, host, |view, _| view.session().token());
            with(cx, host, |view, cx| view.show("/p/latest.3mf", 0, cx));
            let token = with(cx, host, |view, _| view.session().token());
            let slot = with(cx, host, |view, _| view.session().slot());
            let foreign = if slot == "look" { "column" } else { "look" };
            assert!(!with(cx, host, |view, cx| view
                .on_line(&reply(slot, old, "ready", format, ""), cx)));
            assert!(!with(cx, host, |view, cx| view
                .on_line(&reply(foreign, token, "failed", format, ""), cx)));
            assert!(with(cx, host, |view, cx| view
                .on_line(&reply(slot, token, "cancelled", format, ""), cx)));
            assert_eq!(with(cx, host, |view, _| view.phase.clone()), Phase::Idle);
            assert!(!with(cx, host, |view, cx| view
                .on_line(&reply(slot, token, "ready", format, ""), cx)));
            with(cx, host, |view, cx| view.show("/p/failing.stl", 0, cx));
            let token = with(cx, host, |view, _| view.session().token());
            assert!(with(cx, host, |view, cx| view
                .on_line(&reply(slot, token, "failed", format, ""), cx)));
            assert_eq!(
                with(cx, host, |view, _| view.failed()).as_deref(),
                Some("This file cannot be previewed.")
            );
            with(cx, host, |view, cx| view.hide(cx));
        }
    }
    let (look, _) = open(cx, Surface::QuickLook, true);
    load(cx, look, "/p/kit.3mf", "/c/kit.3d", "3mf");
    frame(cx, look);
    let old = with(cx, look, |view, _| view.presented.borrow().image.clone()).unwrap();
    assert!(with(cx, look, |view, cx| view.act("plateNext", cx)));
    assert!(with(cx, look, |view, _| std::sync::Arc::ptr_eq(
        view.presented.borrow().image.as_ref().unwrap(),
        &old
    )));
    let token = with(cx, look, |view, _| view.session().token());
    assert!(with(cx, look, |view, cx| view
        .on_line(&reply("look", token, "ready", "3mf", ""), cx)));
    cx.run_until_parked();
    assert_eq!(with(cx, look, |view, _| view.bar().unwrap().chosen), 1);
    with(cx, look, |view, cx| view.hide(cx));
}

#[gpui::test]
fn m9_g2rows_16_hidden_column_and_quicklook_motion_rules(cx: &mut TestAppContext) {
    let (look, _) = open(cx, Surface::QuickLook, true);
    load(cx, look, "/p/bracket.stl", "/c/bracket.3d", "stl");
    frame(cx, look);
    assert!(with(cx, look, |view, _| view
        .live
        .borrow()
        .viewer
        .as_ref()
        .unwrap()
        .spinning()));
    for host in [
        Host {
            shown: false,
            ..Host::default()
        },
        Host {
            focused: false,
            ..Host::default()
        },
    ] {
        with(cx, look, |view, cx| {
            view.host = host;
            cx.notify();
        });
        frame(cx, look);
        let before = with(cx, look, |view, _| {
            view.live.borrow().viewer.as_ref().unwrap().view
        });
        for action in [
            "seekBack",
            "zoomIn",
            "modelReset",
            "wireframe",
            "plateNext",
            "playPause",
        ] {
            assert!(
                !with(cx, look, |view, cx| view.act(action, cx)),
                "inactive preview accepted {action}: {host:?}"
            );
        }
        assert_eq!(
            with(cx, look, |view, _| view
                .live
                .borrow()
                .viewer
                .as_ref()
                .unwrap()
                .view),
            before
        );
        let requests = with(cx, look, |view, _| view.live.borrow().counters.requests);
        for _ in 0..10 {
            assert_eq!(frame(cx, look), 0);
        }
        assert_eq!(
            with(cx, look, |view, _| view.live.borrow().counters.requests),
            requests
        );
    }
    with(cx, look, |view, cx| {
        view.host = Host {
            animations: false,
            ..Host::default()
        };
        cx.notify();
    });
    frame(cx, look);
    assert!(!with(cx, look, |view, _| view
        .live
        .borrow()
        .viewer
        .as_ref()
        .unwrap()
        .spinning()));
    for _ in 0..10 {
        assert_eq!(frame(cx, look), 0);
    }
    with(cx, look, |view, cx| view.hide(cx));
    let (column, _) = open(cx, Surface::Column, true);
    with(cx, column, |view, cx| view.show("/p/kit.3mf", 0, cx));
    let token = with(cx, column, |view, _| view.session().token());
    assert!(with(cx, column, |view, cx| view.on_line(
        &reply("column", token, "ready", "3mf", "/c/authored.png"),
        cx
    )));
    cx.run_until_parked();
    for _ in 0..10 {
        assert_eq!(frame(cx, column), 0);
    }
    assert!(with(cx, column, |view, _| view.live.borrow().thumbnail));
    assert_eq!(
        with(cx, column, |view, _| view.live.borrow().counters.rasters),
        0
    );
    with(cx, column, |view, cx| view.hide(cx));
    let mut generated = Live::new(Surface::Column);
    generated.set(
        Some(Model::build(stl_box([0.; 3], [10.; 3]), 0, DARK).unwrap()),
        false,
    );
    let first = generated.frame(0., &Host::default(), (64., 64.), 1., DARK);
    assert!(!first.moving);
    assert!(generated.finish(first.job.unwrap().draw()));
    assert!(generated
        .frame(1., &Host::default(), (64., 64.), 1., DARK)
        .job
        .is_none());
}
