use super::*;
use crate::{backend::model3d, jsondoc::{self, Json}, tui::{actions, input::Key as Input, keymap::Map, model::Row, stage, wire}};
use std::{path::Path, time::{Duration, Instant}};

const OBJ: &[u8] = b"v 0 0 0\nv 10 0 0\nv 0 10 0\nv 0 0 10\nf 1 3 2\nf 1 2 4\nf 1 4 3\nf 2 3 4\n";

pub(crate) struct Fixture(PathBuf);
impl Fixture {
    pub(crate) fn new() -> Self {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/m9-tui-04");
        std::fs::create_dir_all(&parent).unwrap();
        let output = std::process::Command::new("mktemp").args(["-d"]).arg(parent.join("test.XXXXXXXX")).output().unwrap();
        assert!(output.status.success());
        let path = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(path.is_absolute() && path.parent() == Some(parent.as_path()));
        std::fs::write(path.join(".flea-test-sandbox"), b"owned m9 TUI model tests\n").unwrap();
        Self(path)
    }
    pub(crate) fn path(&self, name: &str) -> PathBuf { self.0.join(name) }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(self.0.is_absolute() && !self.0.as_os_str().is_empty() && self.0.join(".flea-test-sandbox").is_file());
        assert!(self.0.starts_with(Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/m9-tui-04")));
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

pub(super) fn model(root: &Fixture) -> Model {
    std::fs::write(root.path("part.obj"), OBJ).unwrap();
    let mut m = Model::new(root.0.clone(), &Json::Null);
    m.height = 28;
    m.columns = 130;
    m.total = 1;
    m.rows.insert(0, Row { name: "part.obj".into(), directory: false, size: OBJ.len(), mode: 0o100644, link: String::new(), kind: "OBJ 3D model".into(), thumbnail: false, modified: 0, birth: None, icon: "application-octet-stream".into() });
    m.preview_state.cell = Some((8, 16));
    m.preview_state.colors = Some(crate::tui::chafa::ColorMode::Truecolor);
    m.model3d.spool = Some(root.0.clone());
    crate::tui::preview::load(&mut m, true);
    m
}
pub(super) fn step(m: &mut Model, graphics: &mut Graphics, shown: &mut stage::Shown, wire: &mut Wire, overlay: bool) {
    stage::step(m, graphics, shown, wire, (130, 31), overlay, &mut String::new()).unwrap();
}
pub(super) fn settle(mut predicate: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(20);
    while !predicate() {
        assert!(Instant::now() < until, "model preview did not settle");
        std::thread::sleep(Duration::from_millis(5));
    }
}
pub(super) fn reply(m: &mut Model, root: &Fixture, format: &str, bytes: &[u8]) {
    let parsed = model3d::parse(format, bytes).unwrap();
    let file = root.path("cache.3d");
    model3d::wire::write(&parsed, File::create(&file).unwrap()).unwrap();
    let value = jsondoc::parse(&format!(r#"{{"t":"model3d","slot":"{SLOT}","token":{},"state":"ready","file":"{}","data":{}}}"#, m.model3d.token, file.display(), model3d::wire::summary(&parsed))).unwrap();
    receive(m, &value);
}
pub(super) fn prime(m: &mut Model, root: &Fixture, graphics: &mut Graphics, shown: &mut stage::Shown, wire: &mut Wire) {
    settle(|| { step(m, graphics, shown, wire, false); m.model3d.waiting });
    let ask = wire.events.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    assert_eq!(wire::text(&ask, "c"), "model3d");
    assert_eq!(wire::text(&ask, "path"), root.path("part.obj").to_string_lossy());
    reply(m, root, "obj", OBJ);
}
pub(super) fn input(text: &str) -> Input {
    if matches!(text, "Space" | "Escape" | "Left" | "Right") { Input::named(text, "") }
    else { Input { name: text.into(), text: text.into(), mods: String::new(), pointer: None } }
}

#[test]
fn tui_model3d_fix1_symlink_reply_keeps_selected_and_content_identity() {
    let root = Fixture::new();
    let mut m = model(&root);
    std::os::unix::fs::symlink("part.obj", root.path("link.obj")).unwrap();
    let row = m.rows.get_mut(&0).unwrap();
    row.name = "link.obj".into();
    row.link = "part.obj".into();
    row.mode = 0o120777;
    crate::tui::preview::load(&mut m, true);
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    graphics.protocol = crate::tui::graphics::Protocol::Kitty;
    m.capabilities.graphics = graphics.protocol;
    settle(|| { step(&mut m, &mut graphics, &mut shown, &mut wire, false); m.model3d.waiting });
    let ask = wire.events.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    assert_eq!(wire::text(&ask, "path"), root.path("part.obj").to_string_lossy());
    let parsed = model3d::parse("obj", OBJ).unwrap();
    let file = root.path("cache.3d");
    model3d::wire::write(&parsed, File::create(&file).unwrap()).unwrap();
    let value = jsondoc::parse(&format!(r#"{{"t":"model3d","slot":"{SLOT}","token":{},"state":"ready","file":"{}","data":{}}}"#, m.model3d.token, file.display(), model3d::wire::summary(&parsed))).unwrap();
    m.receive(value.clone(), &mut wire).unwrap();
    assert!(!m.model3d.waiting, "a valid symlink reply was rejected");
    assert!(m.model3d.data.is_some());
    settle(|| { step(&mut m, &mut graphics, &mut shown, &mut wire, false); m.model3d.ready.is_some() });
    assert_eq!(m.preview_path, root.path("link.obj"));
    assert_eq!(m.model3d.held.as_ref().unwrap().path, root.path("part.obj"));
    m.model3d.waiting = true;
    m.model3d.data = None;
    m.rows.get_mut(&0).unwrap().link = "other.obj".into();
    m.receive(value.clone(), &mut wire).unwrap();
    assert!(m.model3d.waiting && m.model3d.data.is_none(), "a retargeted link accepted an old reply");
    m.rows.get_mut(&0).unwrap().link = "part.obj".into();
    m.preview_generation += 1;
    m.receive(value, &mut wire).unwrap();
    assert!(m.model3d.waiting && m.model3d.data.is_none(), "a new generation accepted an old reply");
    m.model3d.stop(&mut wire).unwrap();
    drop(graphics);
    wire::finish(wire, reader);
}

#[test]
fn tui_model3d_fix1_chafa_probe_completes_on_a_later_dispatch() {
    let root = Fixture::new();
    let mut m = model(&root);
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    step(&mut m, &mut graphics, &mut shown, &mut wire, false);
    assert!(m.model3d.chafa.is_none(), "stage synchronously discovered chafa on PATH");
    settle(|| { step(&mut m, &mut graphics, &mut shown, &mut wire, false); m.model3d.chafa.is_some() });
    assert_eq!(m.model3d.chafa, Some(crate::tui::chafa::installed()));
    m.model3d.stop(&mut wire).unwrap();
    wire::finish(wire, reader);
}

#[test]
fn tui_model3d_steps_orbit_only_in_quicklook() {
    let root = Fixture::new();
    let mut m = model(&root);
    let (mut wire, reader) = wire::echo_wire();
    let map = Map::load();
    for preset in ["default", "mac", "vim", "yazi"] {
        m.preset = preset.into();
        m.preview_focus = true;
        m.quicklook = false;
        actions::key(&mut m, &input("Right"), &map, &mut wire).unwrap();
        assert_eq!(m.model3d.controls.orbit, 0);
        m.quicklook = true;
        for _ in 0..2 { actions::key(&mut m, &input("Right"), &map, &mut wire).unwrap(); }
        assert_eq!(m.model3d.controls.orbit, 2, "{preset}");
        actions::key(&mut m, &input("h"), &map, &mut wire).unwrap();
        assert_eq!(m.model3d.controls.orbit, 1);
        actions::key(&mut m, &input("l"), &map, &mut wire).unwrap();
        assert_eq!(m.model3d.controls.orbit, 2);
        actions::key(&mut m, &input("Space"), &map, &mut wire).unwrap();
        assert!(!m.quicklook);
        m.model3d.controls = Controls::default();
    }
    wire::finish(wire, reader);
}

#[test]
fn tui_model3d_plate_zoom_reframe_wire_are_bounded() {
    let root = Fixture::new();
    let mut m = model(&root);
    let (mut wire, reader) = wire::echo_wire();
    m.quicklook = true;
    m.model3d.data = Some(Box::new(Data { plates: vec![Default::default(), Default::default()], ..Data::default() }));
    let map = Map::load();
    for text in ["]", "]", "["] { actions::key(&mut m, &input(text), &map, &mut wire).unwrap(); }
    assert_eq!(m.model3d.controls.plate, 1);
    for _ in 0..100 { actions::key(&mut m, &input("+"), &map, &mut wire).unwrap(); }
    assert_eq!(m.model3d.controls.zoom, 32);
    for _ in 0..100 { actions::key(&mut m, &input("-"), &map, &mut wire).unwrap(); }
    assert_eq!(m.model3d.controls.zoom, -32);
    actions::key(&mut m, &input("w"), &map, &mut wire).unwrap();
    assert!(m.model3d.controls.wire);
    actions::key(&mut m, &input("0"), &map, &mut wire).unwrap();
    assert_eq!(m.model3d.controls, Controls { plate: 1, wire: true, ..Controls::default() });
    assert!(pixels((usize::MAX, 3), (8, 16)).is_none());
    assert_eq!(pixels((10_000, 10_000), (8, 16)), Some((384, 768)));
    wire::finish(wire, reader);
}

#[test]
fn tui_model3d_rejects_stale_source_and_view() {
    let root = Fixture::new();
    let mut m = model(&root);
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    prime(&mut m, &root, &mut graphics, &mut shown, &mut wire);
    let data = m.model3d.data.take();
    m.model3d.waiting = true;
    let value = |token| jsondoc::parse(&format!(r#"{{"t":"model3d","slot":"{SLOT}","token":{token},"state":"failed"}}"#)).unwrap();
    receive(&mut m, &value(0));
    assert!(m.model3d.waiting && m.preview_state.unshown.is_none());
    m.preview_generation += 1;
    let token = m.model3d.token;
    receive(&mut m, &value(token));
    assert!(m.preview_state.unshown.is_none());
    m.model3d.data = data;
    let held = Held::open(root.path("part.obj")).unwrap();
    std::fs::rename(root.path("part.obj"), root.path("old.obj")).unwrap();
    std::fs::write(root.path("part.obj"), OBJ).unwrap();
    assert!(!held.fresh(), "a replacement with identical bytes is a different held source");
    let wanted = Key { path: root.path("part.obj"), generation: m.preview_generation, controls: Controls::default(), pixels: (80, 60), cells: (10, 4), palette: m.model3d_palette, quicklook: false };
    assert!(crate::tui::still::render_model(Arc::new(held), root.path("cache.3d"), wanted, None, Some(root.0.clone()), &AtomicBool::new(false)).is_err());
    m.model3d.stop(&mut wire).unwrap();
    wire::finish(wire, reader);
}

#[test]
fn tui_model3d_rest_has_no_extra_render() {
    let root = Fixture::new();
    let mut m = model(&root);
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    graphics.protocol = crate::tui::graphics::Protocol::Kitty;
    m.capabilities.graphics = graphics.protocol;
    prime(&mut m, &root, &mut graphics, &mut shown, &mut wire);
    settle(|| { step(&mut m, &mut graphics, &mut shown, &mut wire, false); m.model3d.ready.is_some() });
    assert_eq!(m.model3d.renders, 1);
    for _ in 0..50 { step(&mut m, &mut graphics, &mut shown, &mut wire, false); }
    assert_eq!(m.model3d.renders, 1);
    assert!(wire.events.try_recv().is_err());
    m.quicklook = true;
    actions::key(&mut m, &input("Right"), &Map::load(), &mut wire).unwrap();
    step(&mut m, &mut graphics, &mut shown, &mut wire, false);
    assert!(graphics.bytes.is_empty() && m.preview_state.still.key().is_none());
    settle(|| { step(&mut m, &mut graphics, &mut shown, &mut wire, false); m.model3d.ready.as_ref().is_some_and(|r| r.key.controls.orbit == 1) });
    assert_eq!(m.model3d.renders, 2);
    for _ in 0..50 { step(&mut m, &mut graphics, &mut shown, &mut wire, false); }
    assert_eq!(m.model3d.renders, 2);
    actions::key(&mut m, &input("Space"), &Map::load(), &mut wire).unwrap();
    settle(|| { step(&mut m, &mut graphics, &mut shown, &mut wire, false); m.model3d.ready.as_ref().is_some_and(|r| !r.key.quicklook && r.key.controls == Controls::default()) });
    assert_eq!(m.model3d.renders, 3, "the resting column regained its own isometric view");
    m.model3d.stop(&mut wire).unwrap();
    drop(graphics);
    wire::finish(wire, reader);
}

#[test]
fn tui_model3d_classifies_decodes_and_renders_every_parser_format() {
    let root = Fixture::new();
    let gcode = b"G90\nM83\nG1 Z0.2 F1200\nG1 X10 E1\nG1 Y10 E1\n";
    let mut bgcode = b"GCDE\x01\0\0\0\0\0\x01\0\0\0".to_vec();
    bgcode.extend_from_slice(&(gcode.len() as u32).to_le_bytes());
    bgcode.extend_from_slice(&[0, 0]);
    bgcode.extend_from_slice(gcode);
    let cases: [(&str, &[u8]); 7] = [
        ("obj", OBJ),
        ("stl", b"solid triangle\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 10 0 0\nvertex 0 10 0\nendloop\nendfacet\nendsolid triangle\n"),
        ("3mf", include_bytes!("../backend/model3d/fixtures/deflated.3mf")),
        ("ply", b"ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0\n10 0 0\n0 10 0\n3 0 1 2\n"),
        ("amf", b"<amf><object id='1'><mesh><vertices><vertex><coordinates><x>0</x><y>0</y><z>0</z></coordinates></vertex><vertex><coordinates><x>10</x><y>0</y><z>0</z></coordinates></vertex><vertex><coordinates><x>0</x><y>10</y><z>0</z></coordinates></vertex></vertices><volume><triangle><v1>0</v1><v2>1</v2><v3>2</v3></triangle></volume></mesh></object></amf>"),
        ("gcode", gcode), ("bgcode", &bgcode),
    ];
    for (format, bytes) in cases {
        let mut m = model(&root);
        m.rows.get_mut(&0).unwrap().name = format!("part.{format}");
        assert_eq!(crate::tui::pane_state::kind(&m.rows[&0]), crate::tui::pane_state::Kind::Model3d);
        let parsed = model3d::parse(format, bytes).unwrap();
        model3d::wire::write(&parsed, File::create(root.path("cache.3d")).unwrap()).unwrap();
        let key = Key { path: root.path("part.obj"), generation: 1, controls: Controls::default(), pixels: (160, 120), cells: (20, 8), palette: m.model3d_palette, quicklook: false };
        let started = Instant::now();
        let still = crate::tui::still::render_model(Arc::new(Held::open(root.path("part.obj")).unwrap()), root.path("cache.3d"), key, None, Some(root.0.clone()), &AtomicBool::new(false)).unwrap();
        let ppm = std::fs::read(&still.image.0).unwrap();
        assert!(ppm.starts_with(b"P6\n160 120\n255\n"));
        let header = b"P6\n160 120\n255\n".len();
        assert!(ppm[header..].as_chunks::<3>().0.iter().any(|pixel| pixel != &ppm[header..header + 3]), "{format} rendered only its ground");
        eprintln!("MEASURE tui_model3d {format}: {:?}, 160x120, one still", started.elapsed());
    }
}

#[test]
fn tui_model3d_parser_and_renderer_failures_use_cannot_show() {
    for parser_failure in [true, false] {
        let root = Fixture::new();
        let mut m = model(&root);
        let (mut wire, reader) = wire::echo_wire();
        let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
        graphics.protocol = crate::tui::graphics::Protocol::Kitty;
        m.capabilities.graphics = graphics.protocol;
        prime(&mut m, &root, &mut graphics, &mut shown, &mut wire);
        if parser_failure {
            m.model3d.waiting = true;
            let value = jsondoc::parse(&format!(r#"{{"t":"model3d","slot":"{SLOT}","token":{},"state":"failed","reason":"private parser diagnostic"}}"#, m.model3d.token)).unwrap();
            m.receive(value, &mut wire).unwrap();
        } else {
            std::fs::write(root.path("cache.3d"), b"invalid shared model cache").unwrap();
            settle(|| { step(&mut m, &mut graphics, &mut shown, &mut wire, false); m.preview_state.unshown.is_some() });
        }
        let sentence = m.preview_state.i18n.tr("model3d.cannot_preview");
        let refusal = m.preview_state.unshown.as_ref().unwrap();
        assert_eq!(refusal.sentence, sentence);
        assert!(refusal.open_with && graphics.bytes.is_empty());
        let area = crate::tui::pane::Area { columns: 60, padding: 2, height: 20, card: false };
        let theme = crate::tui::theme::Theme::from_text("");
        let body: String = (0..20).map(|i| crate::tui::pane::body(&m, i, &area, &theme)).collect();
        assert!(body.contains(&sentence) && !body.contains("private parser diagnostic"));
        drop(graphics);
        m.model3d.stop(&mut wire).unwrap();
        wire::finish(wire, reader);
    }
}
