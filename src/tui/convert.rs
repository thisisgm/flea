//! Convert is an adapter to the shared batch service and its held menu identities.
mod card;
use super::{render::cards::words::Held, words::Text};
mod input;
mod replies;
use super::{
    model::Model,
    wire::{count, flag, number, text, word, Wire},
};
use crate::jsondoc::Json;
pub use card::draw;
use flea::ui_logic_print_convert::{
    output::{self, Kind},
    pages::Pages,
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
    serial: usize,
    mime: std::sync::OnceLock<crate::backend::mime::Db>,
    aliases: std::sync::OnceLock<crate::backend::aliases::Aliases>,
}
pub struct Form {
    pub paths: Vec<String>,
    pub kind: Kind,
    pub choice: usize,
    pub strip: bool,
    pub combine: bool,
    pub pages: String,
    pub count: Option<u32>,
    pub focus: usize,
    pub scroll: usize,
    pub error: String,
    error_words: Option<Held>,
    info: Option<super::job::Job>,
    edit_fresh: bool,
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
    pub fn format(&self) -> &'static str {
        self.kind.formats()[self.choice]
    }
    pub fn combine_option(&self) -> bool {
        self.kind == Kind::Image && self.paths.len() > 1 && self.format() == "pdf"
    }
    pub fn option_enabled(&self) -> bool {
        self.kind == Kind::Image && (self.format() != "pdf" || self.combine_option())
    }
}
pub fn offered(m: &Model) -> bool {
    let indices: Vec<_> = if m.selected.is_empty() {
        vec![m.cursor]
    } else {
        m.selected.iter().copied().collect()
    };
    if indices.is_empty() || indices.len() > output::MAX_ITEMS {
        return false;
    }
    let kinds = indices
        .iter()
        .map(|i| {
            m.selected_rows
                .get(i)
                .or_else(|| m.rows.get(i))
                .map(|r| super::print::kind(m, r))
        })
        .collect::<Option<Vec<_>>>();
    kinds.is_some_and(|kinds| output::can_convert(&kinds))
}
fn menu(w: &mut Wire, id: usize, op: &str) -> io::Result<()> {
    w.send(vec![
        ("c", word("menuaction")),
        ("op", word(op)),
        ("id", number(id)),
        ("action", word("convert")),
    ])
}
pub fn open(m: &mut Model, w: &mut Wire, from_menu: bool) -> io::Result<()> {
    if m.converting.run.is_some() {
        return Ok(());
    }
    if super::contents::guarded(m) || m.searching || m.pending_clipboard || !offered(m) {
        m.fail_text(Text::Key("dialogs.output.error.convert_kind"));
        return Ok(());
    }
    m.dismiss_error();
    if !from_menu {
        m.menu_path = m.current_path().unwrap_or_default();
    }
    m.quicklook = false;
    m.preview_focus = false;
    m.menu = false;
    m.converting.card = Some(Form {
        paths: Vec::new(),
        kind: Kind::Image,
        choice: 0,
        strip: false,
        combine: true,
        pages: String::new(),
        count: None,
        focus: 0,
        scroll: 0,
        error: String::new(),
        error_words: None,
        info: None,
        edit_fresh: true,
    });
    super::render::cards::probe(&m.rounded);
    if from_menu {
        m.menu_action = "convertValidate".into();
        menu(w, m.action_id, "validate")
    } else {
        m.action_id = m.action_id.wrapping_add(1).max(1);
        m.menu_count = m.selected.len().max(1);
        m.menu_action = "convertSnapshot".into();
        w.send(vec![
            ("c", word("menuaction")),
            ("op", word("snapshot")),
            ("id", number(m.action_id)),
            ("rows", m.indices()),
        ])
    }
}
pub fn cancel(m: &mut Model, w: &mut Wire) -> io::Result<bool> {
    if let Some(run) = &mut m.converting.run {
        run.cancelled = true;
        if let Some(id) = run.operation {
            w.send(vec![("c", word("transfercancel")), ("id", number(id))])?;
        }
        return Ok(true);
    }
    if m.converting.card.take().is_none() {
        return Ok(false);
    }
    m.menu_action.clear();
    menu(w, m.action_id, "close")?;
    Ok(true)
}
pub fn submit(m: &mut Model, w: &mut Wire) -> io::Result<()> {
    if m.converting.run.is_some() || m.menu_action != "convertCard" {
        return Ok(());
    }
    let f = m.converting.card.as_mut().unwrap();
    let pages = Pages::parse(&f.pages);
    if pages.as_ref().is_err() {
        Held::set(
            &mut f.error,
            &mut f.error_words,
            Text::Key("dialogs.output.error.pages"),
            &m.i18n,
        );
        return Ok(());
    }
    if f.kind == Kind::Pdf {
        let Some(n) = f.count else {
            return Ok(());
        };
        if pages.as_ref().unwrap().selected(n).is_err() {
            Held::set(
                &mut f.error,
                &mut f.error_words,
                Text::Plural("dialogs.output.error.document_pages", n as usize, vec![]),
                &m.i18n,
            );
            return Ok(());
        }
    }
    if f.paths.is_empty() {
        return Ok(());
    }
    let range = pages.unwrap().cups().unwrap_or_else(|| "All".into());
    m.converting.serial = m.converting.serial.wrapping_add(1).max(1);
    let request = m.converting.serial;
    let fields = vec![
        ("c", word("convertbatch")),
        ("requestId", number(request)),
        ("menuId", number(m.action_id)),
        (
            "paths",
            Json::Arr(f.paths.iter().map(|p| word(p)).collect()),
        ),
        ("format", word(f.format())),
        ("strip", Json::Bool(f.strip && f.format() != "pdf")),
        ("combine", Json::Bool(f.combine)),
        ("pages", word(&range)),
    ];
    m.converting.run = Some(Run {
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
    m.converting.card = None;
    m.menu_action = "convertSubmitting".into();
    m.transfer_text(Text::Key("dialogs.output.converting"));
    w.send(fields)
}
pub fn poll(m: &mut Model) {
    let Some(f) = &mut m.converting.card else {
        return;
    };
    let Some(result) = f.info.as_ref().and_then(|j| match j.result.try_recv() {
        Ok(result) => Some(result),
        Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(String::new())),
        Err(_) => None,
    }) else {
        return;
    };
    f.info = None;
    f.count = result
        .ok()
        .and_then(|bytes| {
            String::from_utf8_lossy(&bytes).lines().find_map(|l| {
                l.strip_prefix("Pages:")
                    .and_then(|s| s.trim().parse::<u32>().ok())
            })
        })
        .filter(|n| (1..=flea::ui_logic_print_convert::pages::MAX_PAGES).contains(n));
    if f.count.is_none() {
        Held::set(
            &mut f.error,
            &mut f.error_words,
            Text::Key("dialogs.output.error.page_count"),
            &m.i18n,
        );
    }
}

// Geometry and painting read the same frame locale after this synchronous refresh.
pub fn refresh_locale(m: &mut Model) {
    if let Some(f) = &mut m.converting.card {
        Held::refresh(&mut f.error, &mut f.error_words, &m.i18n);
    }
}

// The range's last file goes by its name alone, under the first path, as the convert-pdf board prints it.
fn last_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map_or_else(
        || path.to_owned(),
        |name| flea::bytename::display(&name.to_string_lossy()).into_owned(),
    )
}
