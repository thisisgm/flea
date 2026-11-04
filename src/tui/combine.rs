//! Combine adapts the shared composition and safe publication service to the terminal card.
mod card;
use super::{render::cards::words::Held, words::Text};
mod input;
mod preview;
mod replies;
use super::{
    model::Model,
    wire::{count, flag, number, text, word, Wire},
};
use crate::jsondoc::Json;
pub use card::{draw, geometry};
use flea::ui_logic_op4::{
    composition::{self, Background, Layout, Settings, Spacing},
    output,
};
pub use input::key;
pub use replies::receive;
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct State {
    pub card: Option<Form>,
    pub run: Option<Run>,
    preview: preview::State,
    serial: usize,
    mime: std::sync::OnceLock<crate::backend::mime::Db>,
    aliases: std::sync::OnceLock<crate::backend::aliases::Aliases>,
}
pub struct Form {
    pub paths: Vec<String>,
    pub sizes: Vec<(u32, u32)>,
    pub settings: Settings,
    pub format: String,
    pub name: String,
    pub columns: String,
    pub focus: usize,
    pub image: usize,
    pub error: String,
    error_words: Option<Held>,
    revision: usize,
    custom_name: bool,
    fresh: bool,
    drag: Option<(usize, usize, usize)>,
}
pub struct Run {
    request: usize,
    menu: usize,
    source: String,
    folder: PathBuf,
    operation: Option<usize>,
    cancelled: bool,
}
impl Form {
    fn new() -> Self {
        Self {
            paths: Vec::new(),
            sizes: Vec::new(),
            settings: Settings {
                layout: Layout::Row,
                spacing: Spacing::Small,
                background: Background::Transparent,
                columns: 2,
            },
            format: "png".into(),
            name: String::new(),
            columns: "2".into(),
            focus: 8,
            image: 0,
            error: String::new(),
            error_words: None,
            revision: 1,
            custom_name: false,
            fresh: true,
            drag: None,
        }
    }
    pub fn background_enabled(&self) -> bool {
        composition::background_enabled(
            self.settings.layout,
            self.settings.spacing,
            self.paths.len(),
            self.settings.columns,
        )
    }
    pub fn composition(&self) -> Option<flea::ui_logic_op4::geometry::Composition> {
        flea::ui_logic_op4::geometry::Composition::new(&self.sizes, &self.settings).ok()
    }
    fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.error.clear();
    }
    fn rename_default(&mut self) {
        if !self.custom_name {
            if let Some(first) = self.paths.first() {
                self.name = output::combined_name(Path::new(first), &self.format);
            }
        }
    }
    fn reorder(&mut self, from: usize, to: usize) {
        if from >= self.paths.len() || to >= self.paths.len() || from == to {
            return;
        }
        let path = self.paths.remove(from);
        self.paths.insert(to, path);
        if self.sizes.len() == self.paths.len() {
            let size = self.sizes.remove(from);
            self.sizes.insert(to, size);
        }
        self.image = to;
        self.focus = 0;
        self.rename_default();
        self.changed();
    }
}
pub fn offered(m: &Model) -> bool {
    if !(2..=output::MAX_ITEMS).contains(&m.selected.len()) {
        return false;
    }
    m.selected.iter().all(|i| {
        m.selected_rows
            .get(i)
            .or_else(|| m.rows.get(i))
            .is_some_and(|r| super::print::kind(m, r) == output::Kind::Image)
    })
}
fn menu(w: &mut Wire, id: usize, op: &str) -> io::Result<()> {
    w.send(vec![
        ("c", word("menuaction")),
        ("op", word(op)),
        ("id", number(id)),
        ("action", word("combine")),
    ])
}
pub fn open(m: &mut Model, w: &mut Wire, from_menu: bool) -> io::Result<()> {
    if m.combining.run.is_some() || m.converting.run.is_some() {
        return Ok(());
    }
    if super::contents::guarded(m) || m.searching || m.pending_clipboard || !offered(m) {
        m.fail_text(Text::Key("dialogs.output.error.image_count"));
        return Ok(());
    }
    m.dismiss_error();
    m.quicklook = false;
    m.preview_focus = false;
    m.menu = false;
    m.combining.card = Some(Form::new());
    super::render::cards::probe(&m.rounded);
    if from_menu {
        m.menu_action = "combineValidate".into();
        menu(w, m.action_id, "validate")
    } else {
        m.action_id = m.action_id.wrapping_add(1).max(1);
        m.menu_count = m.selected.len();
        m.menu_action = "combineSnapshot".into();
        w.send(vec![
            ("c", word("menuaction")),
            ("op", word("snapshot")),
            ("id", number(m.action_id)),
            ("rows", m.indices()),
        ])
    }
}
pub fn cancel(m: &mut Model, w: &mut Wire) -> io::Result<bool> {
    if let Some(run) = &mut m.combining.run {
        run.cancelled = true;
        if let Some(id) = run.operation {
            w.send(vec![("c", word("transfercancel")), ("id", number(id))])?;
        }
        return Ok(true);
    }
    if m.combining.card.take().is_none() {
        return Ok(false);
    }
    m.combining.preview.cancel();
    m.menu_action.clear();
    menu(w, m.action_id, "close")?;
    Ok(true)
}
pub fn submit(m: &mut Model, w: &mut Wire) -> io::Result<()> {
    if m.combining.run.is_some() || m.menu_action != "combineCard" {
        return Ok(());
    }
    let f = m.combining.card.as_mut().unwrap();
    let invalid = if f.settings.layout == Layout::Grid
        && !f
            .columns
            .parse::<usize>()
            .ok()
            .is_some_and(|n| (1..=output::MAX_ITEMS).contains(&n))
    {
        Some("dialogs.output.error.columns")
    } else if !output::bare_name(&f.name) || output::extension(Path::new(&f.name)) != f.format {
        Some("dialogs.output.error.name")
    } else if f.format == "jpg" && f.settings.background == Background::Transparent {
        Some("dialogs.output.error.jpeg_alpha")
    } else {
        None
    };
    if let Some(key) = invalid {
        Held::set(&mut f.error, &mut f.error_words, Text::Key(key), &m.i18n);
        return Ok(());
    }
    if f.paths.len() < 2 {
        return Ok(());
    }
    m.combining.serial = m.combining.serial.wrapping_add(1).max(1);
    let request = m.combining.serial;
    let fields = vec![
        ("c", word("combine")),
        ("requestId", number(request)),
        ("menuId", number(m.action_id)),
        (
            "paths",
            Json::Arr(f.paths.iter().map(|p| word(p)).collect()),
        ),
        ("format", word(&f.format)),
        ("name", word(&f.name)),
        ("layout", word(layout_name(f.settings.layout))),
        ("spacing", word(spacing_name(f.settings.spacing))),
        ("background", word(background_name(f.settings.background))),
        ("columns", number(f.settings.columns)),
    ];
    m.combining.run = Some(Run {
        request,
        menu: m.action_id,
        source: f.paths[0].clone(),
        folder: Path::new(&f.paths[0])
            .parent()
            .unwrap_or(Path::new("/"))
            .into(),
        operation: None,
        cancelled: false,
    });
    m.combining.card = None;
    m.combining.preview.cancel();
    m.menu_action = "combineSubmitting".into();
    m.transfer_text(Text::Key("tui.combine.running"));
    w.send(fields)
}
fn layout_name(value: Layout) -> &'static str {
    match value {
        Layout::Row => "Row",
        Layout::Column => "Column",
        Layout::Grid => "Grid",
    }
}
fn spacing_name(value: Spacing) -> &'static str {
    match value {
        Spacing::None => "None",
        Spacing::Small => "Small",
        Spacing::Large => "Large",
    }
}
fn background_name(value: Background) -> &'static str {
    match value {
        Background::Transparent => "Transparent",
        Background::White => "White",
        Background::Black => "Black",
    }
}
pub fn step(
    m: &mut Model,
    graphics: &mut super::graphics::Graphics,
    frame: &mut String,
) -> (usize, usize) {
    preview::step(m, graphics, frame)
}

pub fn poll(m: &mut Model) {
    if m.combining.card.is_none() {
        m.combining.preview.poll_cancelled();
    }
}

#[cfg(test)]
pub fn preview_pending(m: &Model) -> bool {
    m.combining.preview.pending()
}

// Geometry and painting read the same frame locale after this synchronous refresh.
pub fn refresh_locale(m: &mut Model) {
    if let Some(f) = &mut m.combining.card {
        Held::refresh(&mut f.error, &mut f.error_words, &m.i18n);
    }
}
