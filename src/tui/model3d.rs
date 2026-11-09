use flea::bytename::ExactText as _;
use super::{graphics::{Graphics, Task}, model::Model, pane_state::Unshown, wire::Wire};
use flea::ui_logic_pv4::{self as pv4, color::Palette, scene::Scene, wire::{Data, State as ReplyState}};
use std::{fs::File, io, os::unix::fs::MetadataExt, path::PathBuf, sync::{atomic::{AtomicBool, Ordering}, Arc}};

const SLOT: &str = "tui-model3d";

// Identity is held before asking the parser and checked again around every rendered still.
pub struct Held {
    pub path: PathBuf,
    file: File,
    stamp: [i128; 7],
}
impl Held {
    fn open(path: PathBuf) -> Result<Self, String> {
        let file = super::job::held(&path).map_err(|e| e.to_string())?;
        let stamp = stamp(&file.metadata().map_err(|e| e.to_string())?);
        Ok(Self { path, file, stamp })
    }
    pub fn fresh(&self) -> bool {
        self.file.metadata().is_ok_and(|m| stamp(&m) == self.stamp)
            && std::fs::symlink_metadata(&self.path).is_ok_and(|m| stamp(&m) == self.stamp)
    }
}
fn stamp(m: &std::fs::Metadata) -> [i128; 7] {
    [m.dev() as i128, m.ino() as i128, m.len() as i128, m.mtime() as i128, m.mtime_nsec() as i128, m.ctime() as i128, m.ctime_nsec() as i128]
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Controls {
    pub plate: usize,
    pub orbit: i32,
    pub zoom: i32,
    pub wire: bool,
}

// A render's complete view identity; a new file, camera, terminal, palette or surface invalidates it.
#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    pub path: PathBuf,
    pub generation: usize,
    pub controls: Controls,
    pub pixels: (usize, usize),
    pub cells: (usize, usize),
    pub palette: Palette,
    pub quicklook: bool,
}

#[derive(Default)]
pub struct State {
    source: Option<(PathBuf, usize)>,
    selected: Option<PathBuf>,
    holding: Option<Task<Held>>,
    held: Option<Arc<Held>>,
    token: usize,
    waiting: bool,
    file: Option<PathBuf>,
    pub data: Option<Box<Data>>,
    pub controls: Controls,
    control_generation: usize,
    key: Option<Key>,
    rendering: Option<Task<super::still::ModelStill>>,
    pub ready: Option<super::still::ModelStill>,
    failed: bool,
    pub renders: usize,
    chafa: Option<bool>,
    probing: Option<Task<bool>>,
    protocol: Option<super::graphics::Protocol>,
    #[cfg(test)]
    pub spool: Option<PathBuf>,
}
impl State {
    pub fn stop(&mut self, wire: &mut Wire) -> io::Result<()> {
        if self.waiting {
            send(wire, pv4::wire::cancel(SLOT, self.token as u64))?;
        }
        self.waiting = false;
        if let Some(task) = &self.holding { task.cancel(); }
        if let Some(task) = &self.rendering { task.cancel(); }
        if let Some(task) = &self.probing { task.cancel(); }
        self.source = None;
        self.selected = None;
        self.held = None;
        self.file = None;
        self.data = None;
        self.key = None;
        self.ready = None;
        self.failed = false;
        self.protocol = None;
        self.controls = Controls::default();
        Ok(())
    }
    pub fn active(&self) -> bool {
        self.source.is_some() || self.holding.is_some() || self.rendering.is_some() || self.probing.is_some()
    }
}
fn send(wire: &mut Wire, line: String) -> io::Result<()> {
    let value = crate::jsondoc::parse(&line).map_err(io::Error::other)?;
    let fields = value.as_object().ok_or_else(|| io::Error::other("invalid model request"))?;
    wire.send(fields.iter().map(|(k, v)| (k.as_str(), v.clone())).collect())
}

pub fn receive(m: &mut Model, value: &crate::jsondoc::Json) {
    let Ok(reply) = pv4::wire::parse(&crate::jsondoc::render(value)) else { return };
    let current = m.current_path();
    let content = super::pane_state::content(m).map(|path| (path, m.preview_generation));
    let state = &mut m.model3d;
    if reply.slot != SLOT || reply.token != state.token as u64 || !state.waiting
        || state.source != content || state.selected.as_ref() != Some(&m.preview_path)
        || current.as_ref() != Some(&m.preview_path) || m.pending.is_some()
    { return; }
    state.waiting = false;
    match reply.state {
        ReplyState::Ready { file, data, .. } => {
            state.file = Some(file.into());
            state.data = Some(data);
            m.preview_state.unshown = None;
            m.preview_state.unshown_key = None;
            details(m);
        }
        ReplyState::Failed { .. } | ReplyState::Cancelled => failure(m),
    }
}
fn failure(m: &mut Model) {
    m.model3d.failed = true;
    m.model3d.ready = None;
    m.preview_state.unshown_key = Some("model3d.cannot_preview");
    m.preview_state.unshown = Some(Unshown { sentence: m.preview_state.i18n.tr("model3d.cannot_preview"), open_with: true });
    m.preview_width = 0;
}
fn details(m: &mut Model) {
    super::preview::words::metadata(m);
    m.preview_width = 0;
}
pub fn append_facts(m: &mut Model) {
    if !m.rows.get(&m.cursor).is_some_and(|row| super::pane_state::kind(row) == super::pane_state::Kind::Model3d) { return; }
    let Some(data) = &m.model3d.data else { return };
    let i18n = &m.preview_state.i18n;
    let facts = data.plates.get(m.model3d.controls.plate).filter(|_| data.plates.len() > 1).map_or(&data.facts, |p| &p.facts);
    let mut rows = Vec::new();
    if let Some(value) = dimensions(m) { rows.push(("dimensions", value)); }
    if let Some(n) = facts.triangles.filter(|&n| n > 0) { rows.push(("triangles", i18n.locale().format_count(n.min(i64::MAX as u64) as i64))); }
    if data.format == "3mf" {
        for (key, n) in [("objects", data.facts.objects), ("plates", data.facts.plates)] {
            if let Some(n) = n { rows.push((key, i18n.locale().format_count(n.min(i64::MAX as u64) as i64))); }
        }
    }
    if matches!(data.format.as_str(), "3mf" | "gcode" | "bgcode") {
        for (key, value) in [("printer", &data.slicer.printer), ("filament", &data.slicer.filament)] {
            if let Some(value) = value { rows.push((key, value.clone())); }
        }
        if let Some(mm) = data.slicer.layer_height_mm { rows.push(("layer_height", i18n.tr_args("model3d.value.mm", &[&i18n.locale().format_number(mm, 2)]))); }
        if let Some(seconds) = data.slicer.print_time_s.filter(|s| s.is_finite() && *s >= 0.) {
            let minutes = (seconds / 60.).round() as u64;
            let minute = i18n.locale().localize_number(&format!("{:02}", minutes % 60));
            let duration = if minutes < 60 { i18n.tr_args("model3d.value.minutes", &[&i18n.locale().format_count(minutes as i64)]) }
                else { i18n.tr_args("model3d.value.hours_minutes", &[&i18n.locale().format_count((minutes / 60) as i64), &minute]) };
            rows.push(("print_time", duration));
        }
        if let Some(weight) = data.slicer.weight_g { rows.push(("weight", i18n.tr_args("model3d.value.g", &[&i18n.locale().format_number(weight, 1)]))); }
    }
    for (key, value) in rows {
        let key = match key {
            "dimensions" => "model3d.fact.dimensions", "triangles" => "model3d.fact.triangles",
            "objects" => "model3d.fact.objects", "plates" => "model3d.fact.plates",
            "printer" => "model3d.fact.printer", "filament" => "model3d.fact.filament",
            "layer_height" => "model3d.fact.layer_height", "print_time" => "model3d.fact.print_time",
            _ => "model3d.fact.weight",
        };
        let number = !matches!(key, "model3d.fact.printer" | "model3d.fact.filament");
        super::preview::words::push(m, key, value, number);
    }
    m.preview_width = 0;
}
pub fn dimensions(m: &Model) -> Option<String> {
    let data = m.model3d.data.as_ref()?;
    let facts = data.plates.get(m.model3d.controls.plate).filter(|_| data.plates.len() > 1).map_or(&data.facts, |p| &p.facts);
    let [x, y, z] = facts.dimensions_mm?.map(|v| m.preview_state.i18n.locale().format_number(v, 1));
    Some(m.preview_state.i18n.tr_args("model3d.value.size_mm", &[&x, &y, &z]))
}

// Only Quick Look owns model controls; j and k retain file navigation and Space closes the card.
pub fn key(m: &mut Model, key: &super::input::Key, action: &str, wire: &mut Wire) -> io::Result<bool> {
    if !m.rows.get(&m.cursor).is_some_and(|r| super::pane_state::kind(r) == super::pane_state::Kind::Model3d) { return Ok(false); }
    let plain = matches!(key.mods.as_str(), "" | "shift");
    if plain && (key.name == "Escape" || key.name == "Space") || action == "focusPreview" {
        (m.quicklook, m.preview_focus) = (false, false);
        return Ok(true);
    }
    if matches!(action, "tabNext" | "tabPrevious") { return Ok(false); }
    if !m.quicklook { return Ok(true); }
    if matches!(action, "cursorDown" | "cursorUp") || plain && matches!(key.text.as_str(), "j" | "k") {
        m.move_by(if action == "cursorUp" || key.text == "k" { -1 } else { 1 }, false, wire)?;
        return Ok(true);
    }
    let state = &mut m.model3d;
    if state.control_generation != m.preview_generation {
        state.controls = Controls::default();
        state.control_generation = m.preview_generation;
    }
    let count = state.data.as_ref().map_or(1, |d| d.plates.len().max(1));
    let control = &mut state.controls;
    match action {
        "seekBack" => control.orbit = (control.orbit - 1).rem_euclid(24),
        "seekForward" | "pageForward" => control.orbit = (control.orbit + 1).rem_euclid(24),
        "zoomIn" => control.zoom = (control.zoom + 1).min(32),
        "zoomOut" => control.zoom = (control.zoom - 1).max(-32),
        "zoomFit" | "modelReset" => *control = Controls { plate: control.plate, wire: control.wire, ..Controls::default() },
        "toggleWrap" | "wireframe" => {
            if !state.data.as_ref().is_some_and(|d| matches!(d.format.as_str(), "gcode" | "bgcode")) { control.wire = !control.wire; }
        }
        _ if plain && key.text == "[" => *control = Controls { plate: (control.plate + count - 1) % count, ..Controls::default() },
        _ if plain && key.text == "]" => *control = Controls { plate: (control.plate + 1) % count, ..Controls::default() },
        "tabNext" | "tabPrevious" => return Ok(false),
        _ => {}
    }
    details(m);
    Ok(true)
}

// One step in the real preview path: source hold, backend parse, bounded shared raster and existing terminal transports.
pub fn step(m: &mut Model, graphics: &mut Graphics, wire: &mut Wire, room: (usize, usize), allowed: bool, frame: &mut String) -> io::Result<()> {
    if m.model3d.holding.as_mut().is_some_and(Task::discard_cancelled) { m.model3d.holding = None; }
    if m.model3d.rendering.as_mut().is_some_and(Task::discard_cancelled) { m.model3d.rendering = None; }
    if m.model3d.probing.as_mut().is_some_and(Task::discard_cancelled) { m.model3d.probing = None; }
    if !m.quicklook && m.model3d.controls != Controls::default() {
        m.model3d.controls = Controls::default();
        details(m);
    }
    let source = super::pane_state::content(m).map(|path| (path, m.preview_generation));
    let model_row = m.rows.get(&m.cursor).is_some_and(|r| super::pane_state::kind(r) == super::pane_state::Kind::Model3d);
    if !allowed || source != m.model3d.source || m.model3d.selected.as_ref() != Some(&m.preview_path) {
        let controls = (m.model3d.control_generation == m.preview_generation && m.quicklook).then_some(m.model3d.controls);
        // A model row never keeps the picture of the row before it, even while its preview waits for a manual load.
        let clear = allowed || m.model3d.source.is_some() || model_row;
        m.model3d.stop(wire)?;
        if clear {
            graphics.clear();
            m.preview_state.still.clear();
        }
        if !allowed { return Ok(()); }
        if let Some(controls) = controls { m.model3d.controls = controls; }
        m.model3d.source = source.clone();
        m.model3d.selected = Some(m.preview_path.clone());
    }
    if m.model3d.protocol != Some(graphics.protocol) {
        graphics.clear();
        m.preview_state.still.clear();
        m.model3d.protocol = Some(graphics.protocol);
    }
    if let Some(result) = m.model3d.probing.as_mut().filter(|t| !t.cancelled()).and_then(Task::poll) {
        m.model3d.probing = None;
        m.model3d.chafa = Some(result.unwrap_or(false));
    }
    if graphics.protocol == super::graphics::Protocol::None && m.preview_state.colors.is_some()
        && m.model3d.chafa.is_none() && m.model3d.probing.is_none()
    {
        m.model3d.probing = Some(Task::start(|_| Ok(super::chafa::installed())));
    }
    if m.model3d.failed { return Ok(()); }
    if m.model3d.held.is_none() && m.model3d.holding.is_none() {
        if let Some((path, _)) = source {
            m.model3d.holding = Some(Task::start(move |_| Held::open(path)));
        }
    }
    if let Some(result) = m.model3d.holding.as_mut().filter(|t| !t.cancelled()).and_then(Task::poll) {
        m.model3d.holding = None;
        match result {
            Ok(held) => {
                m.model3d.held = Some(Arc::new(held));
                m.model3d.token = m.model3d.token.wrapping_add(1);
                m.model3d.waiting = true;
                let path = &m.model3d.held.as_ref().unwrap().path;
                send(
                    wire,
                    pv4::wire::request(SLOT, m.model3d.token as u64, &path.exact_text(), None),
                )?;
            }
            Err(_) => failure(m),
        }
    }
    let can_draw = m.preview_state.colors.is_some() && (graphics.protocol != super::graphics::Protocol::None
        || m.model3d.chafa == Some(true));
    if graphics.protocol != super::graphics::Protocol::None && m.preview_state.cell.is_none() { return Ok(()); }
    let Some(pixels) = pixels(room, m.preview_state.cell.unwrap_or(super::graphics::FALLBACK_CELL)).filter(|_| can_draw) else { return Ok(()); };
    let Some((path, generation)) = m.model3d.source.clone() else { return Ok(()); };
    let key = Key { path, generation, controls: m.model3d.controls, pixels, cells: room, palette: m.model3d_palette, quicklook: m.quicklook };
    if m.model3d.key.as_ref() != Some(&key) {
        if let Some(task) = &m.model3d.rendering { task.cancel(); }
        graphics.clear();
        m.preview_state.still.clear();
        m.model3d.key = Some(key.clone());
    }
    if let Some(result) = m.model3d.rendering.as_mut().filter(|t| !t.cancelled()).and_then(Task::poll) {
        m.model3d.rendering = None;
        match result {
            Ok(still) if still.key == key => { m.model3d.ready = Some(still); frame.clear(); }
            Ok(_) => {}
            Err(_) => { failure(m); return Ok(()); }
        }
    }
    if m.model3d.rendering.is_none() && !m.model3d.ready.as_ref().is_some_and(|r| r.key == key) {
        if let (Some(held), Some(file)) = (m.model3d.held.clone(), m.model3d.file.clone()) {
            let old = m.model3d.ready.take();
            let wanted = key.clone();
            #[cfg(test)]
            let spool = m.model3d.spool.clone();
            #[cfg(not(test))]
            let spool = None;
            m.model3d.renders += 1;
            m.model3d.rendering = Some(Task::start(move |cancel| super::still::render_model(held, file, wanted, old, spool, cancel)));
        }
    }
    if let Some(still) = m.model3d.ready.as_ref().filter(|r| r.key == key) {
        let path = still.image.0.clone();
        if graphics.protocol == super::graphics::Protocol::None {
            if let Some(colors) = m.preview_state.colors {
                let key = super::still::Key { path: key.path.clone(), generation, page: (m.model3d.renders, 0, 0), cells: room, colors };
                // chafa refuses the render's PPM as an unknown format, so the half-block path hands it the same pixels as QOI.
                let (width, height) = still.key.pixels;
                m.preview_state.still.want(key, || Some(super::still::Source::Bytes(super::still::qoi(&still.rgba, width, height))));
                if m.preview_state.still.poll() { frame.clear(); }
                if !m.preview_state.still.error.is_empty() { failure(m); }
            }
        } else {
            let cell = m.preview_state.cell.unwrap_or(super::graphics::FALLBACK_CELL);
            graphics.request(path, pixels.0.div_ceil(cell.0), pixels.1.div_ceil(cell.1), pixels);
            if graphics.accept() { frame.clear(); }
            if !graphics.error.is_empty() { graphics.clear(); failure(m); }
        }
    }
    Ok(())
}

// Fit a bounded render into the terminal's pixel box without distorting its aspect ratio.
pub fn pixels(room: (usize, usize), cell: (usize, usize)) -> Option<(usize, usize)> {
    let (w, h) = (room.0.checked_mul(cell.0)?, room.1.checked_mul(cell.1)?);
    if w == 0 || h == 0 { return None; }
    if w <= 1024 && h <= 768 { return Some((w, h)); }
    if w as u128 * 768 <= h as u128 * 1024 { Some((((w as u128 * 768 / h as u128) as usize).max(1), 768)) }
    else { Some((1024, ((h as u128 * 1024 / w as u128) as usize).max(1))) }
}

// The cache is decoded by the same shared reader the GUI preview uses, never by a terminal format parser.
pub fn scene(path: &std::path::Path, cancel: &AtomicBool) -> Result<Scene, String> {
    if cancel.load(Ordering::Relaxed) { return Err("model render cancelled".into()); }
    let mut scene = flea::preview_model::read_cancellable(path, cancel)?;
    scene.recentre();
    Ok(scene)
}

#[cfg(test)]
#[path = "model3d_tests.rs"]
pub(super) mod tests;
#[cfg(test)]
#[path = "model3d_transport_tests.rs"]
mod transport_tests;
#[cfg(test)]
#[path = "model3d_fixtui3d_tests.rs"]
mod fixtui3d_tests;
