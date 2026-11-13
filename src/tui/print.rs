//! Print uses the backend's captured selection, capability query and per-file CUPS acceptance.
mod card;
use super::{render::cards::words::Held, words::Text};
mod input;
mod replies;
use super::{
    model::{Model, Row},
    wire::{count, flag, number, text, word, Wire},
};
use crate::{
    backend::{aliases::Aliases, mime::Db},
    jsondoc::Json,
};
pub use card::draw;
use flea::ui_logic_print_convert::{
    output::{Kind, MAX_ITEMS},
    pages::Pages,
    printing::{self, Capabilities, Destination},
};
pub use input::key;
pub use replies::receive;
use std::{collections::BTreeSet, io, sync::OnceLock};

const MAX_PAGES_BYTES: usize = 256;
const FIELD_COLUMN: usize = 22;
const MIN_CARD_WIDTH: usize = 44;

#[derive(Default)]
pub struct State {
    pub card: Option<Form>,
    pub run: Option<Run>,
    serial: usize,
    pub(super) types: OnceLock<(Db, Aliases)>,
}
pub struct Form {
    pub paths: Vec<String>,
    folder: std::path::PathBuf,
    pub paged: bool,
    pub destinations: Vec<Destination>,
    pub printer: String,
    // The printer's printer-info from the backend, or its queue ID when it gives none.
    pub printer_name: String,
    pub caps: Capabilities,
    pub copies: String,
    pub pages: String,
    pub two_sided: bool,
    pub grayscale: bool,
    pub query: Option<usize>,
    pub pending_printer: String,
    pub queried: bool,
    pub setup: Vec<String>,
    pub error: String,
    error_words: Option<Held>,
    pub focus: usize,
    pub dropdown: bool,
    pub choice: usize,
    pub scroll: usize,
    pub edit_fresh: bool,
}
pub struct Run {
    request: usize,
    menu: usize,
    operation: Option<usize>,
    paths: Vec<String>,
    printer: String,
    outcomes: BTreeSet<String>,
    accepted: BTreeSet<String>,
    cancelled: bool,
}
fn next(m: &mut Model) -> usize {
    m.printing.serial = m.printing.serial.wrapping_add(1).max(1);
    m.printing.serial
}
fn menu(w: &mut Wire, id: usize, op: &str) -> io::Result<()> {
    w.send(vec![
        ("c", word("menuaction")),
        ("op", word(op)),
        ("id", number(id)),
        ("action", word("print")),
    ])
}
pub fn kind(m: &Model, row: &Row) -> Kind {
    if row.directory || !row.link.is_empty() {
        return Kind::Other;
    }
    classify(m, &row.shown())
}
fn classify(m: &Model, path: &str) -> Kind {
    let (mime, aliases) = m
        .printing
        .types
        .get_or_init(|| (Db::load(), Aliases::load()));
    crate::backend::printconvert::classify(path, mime, aliases)
}
pub fn offered(m: &Model) -> bool {
    let indices: Vec<_> = if m.selected.is_empty() {
        vec![m.cursor]
    } else {
        m.selected.iter().copied().collect()
    };
    !indices.is_empty()
        && indices.len() <= MAX_ITEMS
        && indices.iter().all(|index| {
            m.selected_rows
                .get(index)
                .or_else(|| m.rows.get(index))
                .is_none_or(|row| kind(m, row).printable())
        })
}
pub fn open(m: &mut Model, w: &mut Wire, from_menu: bool) -> io::Result<()> {
    m.menu = false;
    let refusal = if m.printing.run.is_some() {
        Some("dialogs.output.error.print_busy")
    } else if super::contents::holding(m) {
        Some("tui.operation.search_busy")
    } else if super::contents::guarded(m) {
        Some("search.contents.identity")
    } else if m.searching {
        Some("tui.operation.search_busy")
    } else if m.pending_clipboard {
        Some("jobs.target.busy")
    } else if !offered(m) {
        Some("dialogs.output.error.print_kind")
    } else {
        None
    };
    if let Some(reason) = refusal {
        m.fail_text(Text::Key(reason));
        return Ok(());
    }
    m.dismiss_error();
    if !from_menu {
        m.menu_path = m.current_path().unwrap_or_default();
    }
    m.quicklook = false;
    m.preview_focus = false;
    m.menu = false;
    m.printing.card = Some(Form {
        paths: Vec::new(),
        folder: m.path.clone(),
        paged: false,
        destinations: Vec::new(),
        printer: String::new(),
        printer_name: String::new(),
        caps: Capabilities::default(),
        copies: "1".into(),
        pages: String::new(),
        two_sided: false,
        grayscale: false,
        query: None,
        pending_printer: String::new(),
        queried: false,
        setup: Vec::new(),
        error: String::new(),
        error_words: None,
        focus: 6,
        dropdown: false,
        choice: 0,
        scroll: 0,
        edit_fresh: true,
    });
    super::render::cards::probe(&m.rounded);
    if from_menu {
        m.menu_action = "printValidate".into();
        menu(w, m.action_id, "validate")
    } else {
        m.action_id = m.action_id.wrapping_add(1).max(1);
        m.menu_count = m.selected.len().max(1);
        m.menu_action = "printSnapshot".into();
        w.send(vec![
            ("c", word("menuaction")),
            ("op", word("snapshot")),
            ("id", number(m.action_id)),
            ("rows", m.indices()),
        ])
    }
}
pub fn query(m: &mut Model, w: &mut Wire, printer: &str) -> io::Result<()> {
    let id = next(m);
    let Some(form) = &mut m.printing.card else {
        return Ok(());
    };
    form.query = Some(id);
    form.pending_printer = printer.into();
    form.error.clear();
    w.send(vec![
        ("c", word("printoptions")),
        ("id", number(id)),
        ("printer", word(printer)),
    ])
}
pub fn cancel(m: &mut Model, w: &mut Wire) -> io::Result<bool> {
    if let Some(run) = &mut m.printing.run {
        run.cancelled = true;
        m.printing.card = None;
        if let Some(id) = run.operation {
            w.send(vec![("c", word("transfercancel")), ("id", number(id))])?;
        }
        return Ok(true);
    }
    if m.printing.card.take().is_none() {
        return Ok(false);
    }
    m.menu_action.clear();
    menu(w, m.action_id, "close")?;
    Ok(true)
}
pub fn submit(m: &mut Model, w: &mut Wire) -> io::Result<()> {
    let Some(form) = &m.printing.card else {
        return Ok(());
    };
    if !form.queried || form.query.is_some() || !form.error.is_empty() || m.printing.run.is_some() {
        return Ok(());
    }
    if form.destinations.is_empty() {
        let argv = form.setup.clone();
        if argv != printing::setup_argv() {
            return Ok(());
        }
        let result = std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        match result {
            Ok(child) => {
                m.launches.push((m.i18n.tr("dialogs.output.setup"), child));
                cancel(m, w)?;
            }
            Err(error) => {
                m.fail_text(Text::Args(
                    "dialogs.output.error.operation",
                    vec![crate::error::io_message(&error).into()],
                ));
            }
        }
        return Ok(());
    }
    let copies = form
        .copies
        .parse::<usize>()
        .ok()
        .filter(|n| (printing::MIN_COPIES..=printing::MAX_COPIES).contains(n));
    let error = if copies.is_none() {
        Some("dialogs.output.error.copies")
    } else if Pages::parse(&form.pages).is_err() {
        Some("dialogs.output.error.pages")
    } else {
        None
    };
    if let Some(error) = error {
        let f = m.printing.card.as_mut().unwrap();
        Held::set(&mut f.error, &mut f.error_words, Text::Key(error), &m.i18n);
        return Ok(());
    }
    if !form.destinations.iter().any(|d| d.name == form.printer) || form.paths.is_empty() {
        return Ok(());
    }
    let request = next(m);
    let form = m.printing.card.as_ref().unwrap();
    let fields = vec![
        ("c", word("print")),
        ("requestId", number(request)),
        ("menuId", number(m.action_id)),
        (
            "paths",
            Json::Arr(form.paths.iter().map(|path| word(path)).collect()),
        ),
        ("printer", word(&form.printer)),
        ("printerName", word(&form.printer_name)),
        ("copies", number(copies.unwrap())),
        ("pages", word(&form.pages)),
        ("twoSided", Json::Bool(form.two_sided)),
        ("grayscale", Json::Bool(form.grayscale)),
    ];
    m.printing.run = Some(Run {
        request,
        menu: m.action_id,
        operation: None,
        paths: form.paths.clone(),
        printer: form.printer_name.clone(),
        outcomes: BTreeSet::new(),
        accepted: BTreeSet::new(),
        cancelled: false,
    });
    m.menu_action = "printSubmitting".into();
    w.send(fields)
}

// Geometry and painting read the same frame locale after this synchronous refresh.
pub fn refresh_locale(m: &mut Model) {
    if let Some(f) = &mut m.printing.card {
        Held::refresh(&mut f.error, &mut f.error_words, &m.i18n);
    }
}
