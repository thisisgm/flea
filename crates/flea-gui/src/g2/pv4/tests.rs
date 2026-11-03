use super::{
    seam::{Host, ModelReader, Wire},
    session::Session,
    view::{Model3dView, Phase},
};
use flea::{
    i18n::I18n,
    ui_logic_pv4::{
        color::Palette,
        control::Surface,
        scene::{Layer, Scene, Segment},
        wire::{self, Data},
    },
};
use gpui::{AppContext, TestAppContext, WindowHandle};
use std::{cell::RefCell, path::Path, rc::Rc, sync::Arc, time::Duration};

pub(super) const DARK: Palette = Palette {
    ground: [0.08, 0.05, 0.04],
    foreground: [0.78, 0.7, 0.58],
    muted: [0.55, 0.48, 0.45],
    frame: [0.44, 0.37, 0.34],
};
const FRAME: Duration = Duration::from_millis(16);

pub(super) fn stl_box(lo: [f32; 3], hi: [f32; 3]) -> Scene {
    let c = |i: usize| {
        [
            if i & 1 == 0 { lo[0] } else { hi[0] },
            if i & 2 == 0 { lo[1] } else { hi[1] },
            if i & 4 == 0 { lo[2] } else { hi[2] },
        ]
    };
    let quads = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [1, 3, 7, 5],
        [3, 2, 6, 7],
        [2, 0, 4, 6],
    ];
    let mut s = Scene::default();
    for q in quads {
        for t in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            let base = s.positions.len() as u32;
            s.positions.extend(t.map(c));
            s.triangles.push([base, base + 1, base + 2]);
        }
    }
    s
}

fn print(layers: u32) -> Scene {
    let mut s = Scene {
        bed: Some([250., 210.]),
        ..Scene::default()
    };
    for l in 0..layers {
        let z = 0.2 * (l + 1) as f32;
        let start = s.segments.len() as u32;
        s.segments.push(Segment {
            a: [100., 100., z],
            b: [140., 100., z],
            extrude: true,
        });
        s.segments.push(Segment {
            a: [140., 100., z],
            b: [140., 120., z],
            extrude: true,
        });
        s.layers.push(Layer {
            segments: start..s.segments.len() as u32,
            z,
        });
    }
    s
}

pub(super) struct Lines(RefCell<Vec<String>>, bool);

impl Wire for Lines {
    fn send(&self, line: String) -> bool {
        self.0.borrow_mut().push(line);
        self.1
    }
}

struct Boxes;

impl ModelReader for Boxes {
    fn read(&self, file: &Path, _: &Data) -> Result<Scene, String> {
        match file.to_str() {
            Some("/c/print.3d") => Ok(print(240)),
            Some("/c/empty.3d") => Ok(Scene::default()),
            Some("/c/bad.3d") => Err("truncated".into()),
            _ => Ok(stl_box([20., 20., 0.], [80., 60., 40.])),
        }
    }
}

fn english() -> Arc<I18n> {
    let none = std::path::PathBuf::from("/nonexistent/flea-pv4");
    let catalogs = vec![crate::test_catalogs::english_only()];
    Arc::new(I18n::load(flea::i18n::Paths {
        omarchy: none.clone(),
        language_file: none,
        catalogs,
    }))
}

fn ready(slot: &str, token: u64, file: &str, format: &str, thumbnail: &str) -> String {
    format!(
        r#"{{"t":"model3d","slot":"{slot}","token":{token},"state":"ready","file":"{file}","thumbnail":"{thumbnail}","data":{{"format":"{format}","facts":{{"dimensions_mm":[60,40,40],"plates":2}},"slicer":{{"print_time_s":4320}},"layers":240,"segments":480,"metadata":{{}},"plates":[{{"facts":{{"dimensions_mm":[60,40,40]}},"slicer":{{}}}},{{"facts":{{"dimensions_mm":[30,30,16]}},"slicer":{{}}}}]}}}}"#
    )
}

pub(super) fn open(
    cx: &mut TestAppContext,
    surface: Surface,
    alive: bool,
) -> (WindowHandle<Model3dView>, Rc<Lines>) {
    let lines = Rc::new(Lines(RefCell::new(Vec::new()), alive));
    let wire: Rc<dyn Wire> = lines.clone();
    let handle =
        cx.add_window(|_, cx| Model3dView::new(surface, wire, Arc::new(Boxes), english(), cx));
    cx.update_window(handle.into(), |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();
    (handle, lines)
}

pub(super) fn with<R>(
    cx: &mut TestAppContext,
    h: WindowHandle<Model3dView>,
    f: impl FnOnce(&mut Model3dView, &mut gpui::Context<Model3dView>) -> R,
) -> R {
    h.update(cx, |view, _, cx| f(view, cx)).unwrap()
}

pub(super) fn load(
    cx: &mut TestAppContext,
    h: WindowHandle<Model3dView>,
    path: &str,
    file: &str,
    format: &str,
) {
    with(cx, h, |v, cx| v.show(path, 0, cx));
    let token = with(cx, h, |v, _| v.session().token());
    let slot = with(cx, h, |v, _| v.session().slot());
    assert!(with(cx, h, |v, cx| v
        .on_line(&ready(slot, token, file, format, ""), cx)));
    cx.run_until_parked();
}

pub(super) fn token(cx: &mut TestAppContext, h: WindowHandle<Model3dView>) -> u64 {
    with(cx, h, |v, _| v.session().token())
}

pub(super) fn frame(cx: &mut TestAppContext, h: WindowHandle<Model3dView>) -> usize {
    cx.executor().advance_clock(FRAME);
    cx.update_window(h.into(), |_, window, cx| window.simulate_next_frame(cx))
        .unwrap()
}

fn requests(cx: &mut TestAppContext, h: WindowHandle<Model3dView>) -> usize {
    with(cx, h, |v, _| v.live.borrow().counters.requests)
}

#[test]
fn each_open_takes_a_new_token_and_only_its_reply_applies_once() {
    let mut s = Session::new("look");
    let line = s.open("/p/a.stl", Some(0));
    let first = s.token();
    assert_eq!(
        line,
        format!(r#"{{"c":"model3d","slot":"look","token":{first},"path":"/p/a.stl","plate":0}}"#)
    );
    s.open("/p/b.stl", None);
    let second = s.token();
    assert!(second > first);
    let stale = wire::parse(&ready("look", first, "/c/a.3d", "stl", "")).unwrap();
    let live = wire::parse(&ready("look", second, "/c/b.3d", "stl", "")).unwrap();
    let other = wire::parse(&ready("column", second, "/c/b.3d", "stl", "")).unwrap();
    assert!(!s.accept(&stale) && !s.accept(&other));
    assert!(s.accept(&live) && !s.accept(&live));
    assert_eq!(s.close(), None);
    s.open("/p/c.stl", None);
    let third = s.token();
    assert_eq!(
        s.close(),
        Some(format!(
            r#"{{"c":"model3d","slot":"look","token":{third},"cancel":true}}"#
        ))
    );
    let mut later = Session::new("look");
    later.open("/p/knob.obj", None);
    assert!(
        later.token() > third,
        "a view mounted later never reuses an earlier view's token"
    );
    assert!(!later.accept(&wire::parse(&ready("look", first, "/c/a.3d", "stl", "")).unwrap()));
}

#[gpui::test]
fn a_ready_reply_builds_off_the_main_thread_and_swaps_in_whole(cx: &mut TestAppContext) {
    let (h, lines) = open(cx, Surface::QuickLook, true);
    load(cx, h, "/p/bracket.stl", "/c/bracket.3d", "stl");
    let t = token(cx, h);
    assert_eq!(
        lines.0.borrow()[0],
        format!(r#"{{"c":"model3d","slot":"look","token":{t},"path":"/p/bracket.stl","plate":0}}"#)
    );
    assert_eq!(with(cx, h, |v, _| v.phase.clone()), Phase::Ready);
    assert!(with(cx, h, |v, _| v.live.borrow().model.is_some()));
    let bar = with(cx, h, |v, _| v.bar()).unwrap();
    assert_eq!(
        (bar.fact.as_deref(), bar.words.clone(), bar.chosen),
        (
            Some("60.0 × 40.0 × 40.0 mm"),
            vec!["Plate 1".to_string(), "Plate 2".into()],
            0
        )
    );
}

#[gpui::test]
fn stale_malformed_and_foreign_lines_change_nothing(cx: &mut TestAppContext) {
    let (h, _lines) = open(cx, Surface::QuickLook, true);
    with(cx, h, |v, cx| v.show("/p/a.stl", 0, cx));
    let first = token(cx, h);
    with(cx, h, |v, cx| v.show("/p/b.stl", 0, cx));
    let second = token(cx, h);
    for line in [
        ready("look", first, "/c/a.3d", "stl", ""),
        ready("column", second, "/c/a.3d", "stl", ""),
        "{\"t\":\"model3d\"".into(),
        "{\"t\":\"listed\"}".into(),
        String::new(),
    ] {
        assert!(!with(cx, h, |v, cx| v.on_line(&line, cx)), "{line}");
    }
    cx.run_until_parked();
    assert_eq!(with(cx, h, |v, _| v.phase.clone()), Phase::Loading);
}

#[gpui::test]
fn a_failed_reply_a_failed_read_or_a_dead_backend_say_the_shared_sentence(cx: &mut TestAppContext) {
    let (h, _) = open(cx, Surface::QuickLook, true);
    with(cx, h, |v, cx| v.show("/p/a.stl", 0, cx));
    let failed = format!(
        r#"{{"t":"model3d","slot":"look","token":{},"state":"failed","reason":"parse error at byte 84"}}"#,
        token(cx, h)
    );
    assert!(with(cx, h, |v, cx| v.on_line(&failed, cx)));
    assert_eq!(
        with(cx, h, |v, _| v.failed()).as_deref(),
        Some("This file cannot be previewed.")
    );
    for file in ["/c/bad.3d", "/c/empty.3d"] {
        with(cx, h, |v, cx| v.hide(cx));
        load(cx, h, "/p/b.stl", file, "stl");
        assert!(
            matches!(with(cx, h, |v, _| v.phase.clone()), Phase::Failed(_)),
            "{file}"
        );
    }
    let (dead, _) = open(cx, Surface::QuickLook, false);
    with(cx, dead, |v, cx| v.show("/p/a.stl", 0, cx));
    assert_eq!(
        with(cx, dead, |v, _| v.failed()).as_deref(),
        Some("This file cannot be previewed.")
    );
}

#[gpui::test]
fn hide_cancels_the_request_and_drops_a_build_in_flight(cx: &mut TestAppContext) {
    let (h, lines) = open(cx, Surface::QuickLook, true);
    with(cx, h, |v, cx| v.show("/p/a.stl", 0, cx));
    let t = token(cx, h);
    assert!(with(cx, h, |v, cx| v
        .on_line(&ready("look", t, "/c/a.3d", "stl", ""), cx)));
    with(cx, h, |v, cx| v.hide(cx));
    cx.run_until_parked();
    assert!(with(cx, h, |v, _| v.live.borrow().model.is_none()));
    assert_eq!(with(cx, h, |v, _| v.phase.clone()), Phase::Idle);
    with(cx, h, |v, cx| v.show("/p/b.stl", 0, cx));
    with(cx, h, |v, cx| v.hide(cx));
    let t = token(cx, h);
    assert_eq!(
        lines.0.borrow().last().cloned(),
        Some(format!(
            r#"{{"c":"model3d","slot":"look","token":{t},"cancel":true}}"#
        ))
    );
}

#[gpui::test]
fn quick_look_turns_and_asks_for_frames_only_while_shown_focused_and_animating(
    cx: &mut TestAppContext,
) {
    let (h, _) = open(cx, Surface::QuickLook, true);
    load(cx, h, "/p/bracket.stl", "/c/bracket.3d", "stl");
    for _ in 0..5 {
        frame(cx, h);
    }
    let turning = requests(cx, h);
    assert!(turning >= 4, "{turning}");
    for host in [
        Host {
            focused: false,
            ..Host::default()
        },
        Host {
            shown: false,
            ..Host::default()
        },
        Host {
            animations: false,
            ..Host::default()
        },
    ] {
        with(cx, h, |v, cx| {
            v.host = host;
            cx.notify();
        });
        frame(cx, h);
        let before = requests(cx, h);
        for _ in 0..10 {
            assert_eq!(frame(cx, h), 0, "{host:?}");
        }
        assert_eq!(requests(cx, h), before, "{host:?}");
        with(cx, h, |v, cx| {
            v.host = Host::default();
            cx.notify();
        });
        frame(cx, h);
    }
}

#[gpui::test]
fn the_column_rests_on_the_embedded_picture_and_never_turns(cx: &mut TestAppContext) {
    let (h, _) = open(cx, Surface::Column, true);
    with(cx, h, |v, cx| v.show("/p/kit.3mf", 0, cx));
    let t = token(cx, h);
    assert!(with(cx, h, |v, cx| v.on_line(
        &ready("column", t, "/c/kit.3d", "3mf", "/c/kit-all.png"),
        cx
    )));
    cx.run_until_parked();
    for _ in 0..10 {
        frame(cx, h);
    }
    assert_eq!(requests(cx, h), 0);
    assert!(with(cx, h, |v, _| v.live.borrow().thumbnail));
    assert_eq!(with(cx, h, |v, _| v.live.borrow().counters.rasters), 0);
    assert!(
        !with(cx, h, |v, cx| v.act("playPause", cx))
            && !with(cx, h, |v, cx| v.act("plateNext", cx))
    );
}

#[gpui::test]
fn plate_keys_load_the_next_plate_and_keep_the_old_frame_until_it_lands(cx: &mut TestAppContext) {
    let (h, lines) = open(cx, Surface::QuickLook, true);
    load(cx, h, "/p/kit.3mf", "/c/kit.3d", "3mf");
    assert!(with(cx, h, |v, cx| v.act("plateNext", cx)));
    let t = token(cx, h);
    assert_eq!(
        lines.0.borrow().last().cloned(),
        Some(format!(
            r#"{{"c":"model3d","slot":"look","token":{t},"path":"/p/kit.3mf","plate":1}}"#
        ))
    );
    assert!(with(cx, h, |v, _| v.live.borrow().model.is_some()));
    assert!(with(cx, h, |v, cx| v
        .on_line(&ready("look", t, "/c/kit.3d", "3mf", ""), cx)));
    cx.run_until_parked();
    assert_eq!(with(cx, h, |v, _| v.bar()).unwrap().chosen, 1);
}

#[gpui::test]
fn a_print_opens_at_its_last_layer_and_the_page_keys_step_it(cx: &mut TestAppContext) {
    let (h, _) = open(cx, Surface::QuickLook, true);
    load(cx, h, "/p/benchy.gcode", "/c/print.3d", "gcode");
    frame(cx, h);
    assert_eq!(
        with(cx, h, |v, _| v
            .live
            .borrow()
            .viewer
            .as_ref()
            .and_then(|v| v.layer)),
        Some(239)
    );
    assert!(with(cx, h, |v, cx| v.act("pageDown", cx)));
    assert_eq!(
        with(cx, h, |v, _| v
            .live
            .borrow()
            .viewer
            .as_ref()
            .and_then(|v| v.layer)),
        Some(238)
    );
    assert!(!with(cx, h, |v, cx| v.act("wireframe", cx)));
    assert_eq!(
        with(cx, h, |v, _| v.bar()).unwrap().fact.as_deref(),
        Some("1 h 12 min")
    );
}
