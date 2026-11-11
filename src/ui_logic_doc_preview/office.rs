//! Office documents in the preview: which group a file is, the backend's officepdf and officesheets lines, the
//! converting, ready, failed, locked, unsupported and cancelled states, and the one sentence each failure projects.
use crate::json::{escape, field_bool, field_str, field_str_array, field_usize};

/// The three groups the preview draws differently: documents and presentations as pages, spreadsheets as a table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Document,
    Spreadsheet,
    Presentation,
}

impl Group {
    /// The group by extension, in any case: DOCX, ODT, DOC, RTF; XLSX, ODS, XLS; PPTX, ODP, PPT.
    pub fn of(path: &str) -> Option<Self> {
        // A name that is not UTF-8 is typed by its lossy spelling, where the extension is.
        let shown = crate::bytename::display(path);
        let name = shown.rsplit('/').next().unwrap_or(&shown);
        let (_, ext) = name.rsplit_once('.')?;
        Some(match ext.to_ascii_lowercase().as_str() {
            "docx" | "odt" | "doc" | "rtf" => Group::Document,
            "xlsx" | "ods" | "xls" => Group::Spreadsheet,
            "pptx" | "odp" | "ppt" => Group::Presentation,
            _ => return None,
        })
    }

    /// The bar's leading mark, the row's own kind mark: the lined page, the table or the screen.
    pub fn glyph(self) -> &'static str {
        match self {
            Group::Document => "file-text",
            Group::Spreadsheet => "table",
            Group::Presentation => "presentation",
        }
    }

    /// The catalog key of the facts row that counts the document: Pages, Sheets or Slides.
    pub fn count_key(self) -> &'static str {
        match self {
            Group::Document => "document.office_pages",
            Group::Spreadsheet => "document.office_sheets",
            Group::Presentation => "document.office_slides",
        }
    }
}

/// The backend's office error inventory (src/backend/officepdf.rs); each is diagnostic data, never drawn as written.
pub const NOT_INSTALLED: &str = "LibreOffice is not installed on this box.";
pub const NO_PDF: &str = "the converter wrote no PDF";
pub const NOT_OFFICE: &str = "that file is not an office document";
/// A ready subject that could not draw: its converted PDF did not open, or the code engine refused its sheet.
pub const PDF_UNOPENED: &str = "the converted PDF did not open";
pub const SHEET_REFUSED: &str = "the code engine refused the sheet";
const TIMEOUT_HEAD: &str = "the conversion took longer than ";
const TIMEOUT_TAIL: &str = " seconds";
// A request a newer one or a shutdown ended: a state, never an error.
const ENDED: [&str; 2] = ["cancelled", REPLACED];
// The backend converts one file at a time, so another client's request can replace this one.
const REPLACED: &str = "replaced by a newer request";

/// Why a conversion did not give pages, read from the backend's error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    NotInstalled,
    TimedOut(u64),
    NoPdf,
    NotOffice,
    /// The converter's own failure or a local error, its text kept as the diagnostic.
    Other,
}

impl Reason {
    /// The reason for an error line, None for a request that was cancelled or replaced.
    pub fn of(err: &str) -> Option<Self> {
        if ENDED.contains(&err) {
            return None;
        }
        Some(match err {
            NOT_INSTALLED => Reason::NotInstalled,
            NO_PDF => Reason::NoPdf,
            NOT_OFFICE => Reason::NotOffice,
            _ => match err
                .strip_prefix(TIMEOUT_HEAD)
                .and_then(|rest| rest.strip_suffix(TIMEOUT_TAIL))
            {
                Some(seconds) => seconds.parse().map_or(Reason::Other, Reason::TimedOut),
                None => Reason::Other,
            },
        })
    }

    /// A box with no converter or a file that will never convert keeps today's unsupported preview.
    pub fn unavailable(&self) -> bool {
        matches!(self, Reason::NotInstalled | Reason::NotOffice)
    }
}

/// One office line from the backend.
#[derive(Clone, Debug, PartialEq)]
pub enum Wire {
    Converting,
    /// The cached PDF; first is the cut of a preview that holds only the first pages.
    Pdf {
        path: String,
        first: Option<usize>,
    },
    /// The workbook's sheet names and each sheet's cached CSV, in the workbook's order.
    Sheets {
        names: Vec<String>,
        paths: Vec<String>,
    },
    Failed {
        err: String,
        unavailable: bool,
        locked: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub id: u64,
    pub wire: Wire,
}

/// The request for a document's PDF, or a workbook's sheets.
pub fn request_line(id: u64, path: &str, sheets: bool) -> String {
    let c = if sheets { "officesheets" } else { "officepdf" };
    format!(r#"{{"c":"{}","id":{},"path":"{}"}}"#, c, id, escape(path))
}

/// Sample input: {"t":"officepdf","id":4,"state":"ready","path":"/c/x.pdf"}; officesheets adds names and paths.
pub fn parse(line: &str) -> Option<Line> {
    let kind = field_str(line, "t")?;
    if kind != "officepdf" && kind != "officesheets" {
        return None;
    }
    let id = field_usize(line, "id")? as u64;
    let wire = match field_str(line, "state")?.as_str() {
        "converting" => Wire::Converting,
        "ready" if kind == "officesheets" => Wire::Sheets {
            names: field_str_array(line, "names"),
            paths: field_str_array(line, "paths"),
        },
        "ready" => Wire::Pdf {
            path: field_str(line, "path")?,
            first: field_usize(line, "first"),
        },
        "failed" => Wire::Failed {
            err: field_str(line, "err").unwrap_or_default(),
            unavailable: field_bool(line, "unavailable"),
            locked: field_bool(line, "locked"),
        },
        _ => return None,
    };
    Some(Line { id, wire })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// No document asked.
    Idle,
    /// Asked and not answered: the embedded thumbnail, or the kind mark, and nothing else.
    Converting,
    /// Pages or sheets are here.
    Ready,
    Failed(Reason),
    Locked,
    Unsupported(Reason),
    /// The subject moved on, or the backend ended the request: nothing is drawn and nothing is said.
    Cancelled,
}

/// The failure recipe's mark, its tone and its one sentence's catalog key, with Open With under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Projection {
    pub glyph: &'static str,
    /// The error role for a failure and the lock; the muted frame for unsupported.
    pub error_tone: bool,
    pub key: &'static str,
    pub open_with: bool,
}

/// What a reply asks of the view.
#[derive(Clone, Debug, PartialEq)]
pub enum Applied {
    /// Another request's line, or a line for a document no longer shown.
    Stale,
    Converting,
    Pdf(String),
    Sheet(String),
    /// Failed in a way only the file's header can tell apart from a password.
    Sniff,
    /// Replaced by another client's request: ask once more and keep converting.
    Ask,
    Settled,
}

/// One office subject's state, owned by the preview that shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Office {
    pub group: Option<Group>,
    pub path: String,
    /// The request in flight, 0 when none.
    pub id: u64,
    pub state: State,
    pub pdf: Option<String>,
    /// The preview PDF's cut, None for a whole document (viewer::cut).
    pub first: Option<usize>,
    pub sheet_names: Vec<String>,
    pub sheet_paths: Vec<String>,
    pub sheet: usize,
    /// The backend's own text for the last failure, kept for diagnostics apart from the sentence.
    pub diagnostic: String,
    /// This subject was asked again once after a replace.
    pub reasked: bool,
    /// The failure a converter error gave, held while the header sniff decides between it and a lock.
    pending: Option<Reason>,
}

impl Default for Office {
    fn default() -> Self {
        Office {
            group: None,
            path: String::new(),
            id: 0,
            state: State::Idle,
            pdf: None,
            first: None,
            sheet_names: Vec::new(),
            sheet_paths: Vec::new(),
            sheet: 0,
            diagnostic: String::new(),
            reasked: false,
            pending: None,
        }
    }
}

impl Office {
    /// A new subject asked under `id`; it shows as converting until the backend answers.
    pub fn open(&mut self, path: &str, id: u64) {
        *self = Office {
            group: Group::of(path),
            path: path.to_string(),
            id,
            state: State::Converting,
            ..Office::default()
        };
    }

    /// Whether this subject asks for sheets rather than pages.
    pub fn wants_sheets(&self) -> bool {
        self.group == Some(Group::Spreadsheet)
    }

    /// The subject left before its answer: the request's state is cancelled, and its late line is dropped.
    pub fn leave(&mut self) {
        if self.state == State::Converting {
            self.state = State::Cancelled;
        }
        self.id = 0;
        self.pending = None;
    }

    /// Applies one backend line; only the request in flight takes it.
    pub fn reply(&mut self, line: &Line) -> Applied {
        if self.id == 0 || line.id != self.id {
            return Applied::Stale;
        }
        match &line.wire {
            Wire::Converting => {
                self.state = State::Converting;
                Applied::Converting
            }
            Wire::Pdf { path, first } => {
                self.id = 0;
                self.state = State::Ready;
                self.pdf = Some(path.clone());
                self.first = *first;
                Applied::Pdf(path.clone())
            }
            Wire::Sheets { names, paths } => {
                self.id = 0;
                if paths.is_empty() || names.len() != paths.len() {
                    self.diagnostic = NO_PDF.to_string();
                    self.state = State::Failed(Reason::Other);
                    return Applied::Settled;
                }
                self.state = State::Ready;
                self.sheet_names.clone_from(names);
                self.sheet_paths.clone_from(paths);
                self.sheet = 0;
                Applied::Sheet(paths[0].clone())
            }
            Wire::Failed {
                err,
                unavailable,
                locked,
            } => {
                self.id = 0;
                if err == REPLACED && !*locked && !self.reasked {
                    self.reasked = true;
                    return Applied::Ask;
                }
                self.diagnostic.clone_from(err);
                self.state = match Reason::of(err) {
                    _ if *locked => State::Locked,
                    None => State::Cancelled,
                    Some(reason) if *unavailable || reason.unavailable() => {
                        State::Unsupported(reason)
                    }
                    Some(reason) => {
                        self.pending = Some(reason);
                        return Applied::Sniff;
                    }
                };
                Applied::Settled
            }
        }
    }

    /// The second ask after a replace went out under `id`.
    pub fn asked(&mut self, id: u64) {
        if self.state == State::Converting {
            self.id = id;
        }
    }

    /// A ready subject that could not draw, its PDF unopened or its sheet refused, ends as a failure.
    pub fn undrawable(&mut self, diagnostic: &str) {
        if self.state == State::Ready {
            self.diagnostic = diagnostic.to_string();
            self.state = State::Failed(Reason::Other);
        }
    }

    /// The header sniff's verdict for a failed conversion: locked, or the failure it was asked about.
    pub fn sniffed(&mut self, locked: bool) {
        let Some(reason) = self.pending.take() else {
            return;
        };
        self.state = if locked {
            State::Locked
        } else {
            State::Failed(reason)
        };
    }

    /// The failure recipe a state draws, None for every state that draws pages, a table, a thumbnail or nothing.
    pub fn projection(&self) -> Option<Projection> {
        Some(match self.state {
            State::Failed(_) => Projection {
                glyph: "alert",
                error_tone: true,
                key: "document.office_failed",
                open_with: true,
            },
            State::Locked => Projection {
                glyph: "lock",
                error_tone: true,
                key: "document.office_locked",
                open_with: true,
            },
            State::Unsupported(_) => Projection {
                glyph: "file",
                error_tone: false,
                key: "document.office_unsupported",
                open_with: true,
            },
            _ => return None,
        })
    }
}

#[path = "office_container.rs"]
mod container;
#[cfg(test)]
#[path = "office_cut_tests.rs"]
mod cut_tests;
#[path = "office_sheets.rs"]
mod sheets;
#[cfg(test)]
#[path = "office_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "office_exact_tests.rs"]
mod exact_tests;
