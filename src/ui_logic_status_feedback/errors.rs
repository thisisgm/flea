//! Backend failures as one translated sentence each, chosen from the wire's `where` and its fixed English before any catalog lookup.
use crate::i18n::I18n;

#[path = "drive_names.rs"]
mod drive_names;
pub use drive_names::drive_refusal;
#[path = "unheld.rs"]
mod unheld;

/// A sentence ready to translate: a catalog key and its arguments, never prose matched after translation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sentence {
    pub key: &'static str,
    pub args: Vec<String>,
    /// The argument that is a backend cause, said through `clause` so a fixed one reads in the catalog's language.
    pub cause: Option<usize>,
}

impl Sentence {
    fn key(key: &'static str) -> Self {
        Self {
            key,
            args: Vec::new(),
            cause: None,
        }
    }

    fn with(key: &'static str, arg: &str) -> Self {
        Self {
            key,
            args: vec![arg.to_owned()],
            cause: None,
        }
    }

    fn args(key: &'static str, args: &[&str]) -> Self {
        Self {
            key,
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            cause: None,
        }
    }

    /// The sentence in the catalog's current language.
    pub fn render(&self, catalog: &I18n) -> String {
        let args: Vec<String> = self
            .args
            .iter()
            .enumerate()
            .map(|(at, arg)| {
                if self.cause == Some(at) {
                    clause(arg, catalog)
                } else {
                    arg.clone()
                }
            })
            .collect();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        catalog.tr_args(self.key, &args)
    }
}

/// The pane state a failed listing reaches: a denial is Locked, everything else is Error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListingState {
    Locked,
    Error,
}

/// The fallback when nothing better is known, which never leaks the backend's own words or a path.
pub const GENERIC: &str = "feedback.error.generic";

/// src/error.rs's fixed causes, each a whole sentence of its own; matched whole, so a longer message stays a diagnostic.
const OS_REASONS: [(&str, &str); 35] = [
    ("item not found", "feedback.os.not_found"),
    ("permission denied", "feedback.os.permission_denied"),
    ("already exists", "feedback.os.already_exists"),
    ("connection refused", "feedback.os.connection_refused"),
    ("connection reset", "feedback.os.connection_reset"),
    ("connection ended", "feedback.os.connection_ended"),
    ("not connected", "feedback.os.not_connected"),
    ("address is already in use", "feedback.os.address_in_use"),
    ("address is unavailable", "feedback.os.address_unavailable"),
    ("connection closed", "feedback.os.connection_closed"),
    ("the item is busy; try again", "feedback.os.would_block"),
    ("invalid input", "feedback.os.invalid_input"),
    ("invalid data", "feedback.os.invalid_data"),
    ("operation timed out", "feedback.os.timed_out"),
    ("could not write all data", "feedback.os.write_zero"),
    ("operation was interrupted", "feedback.os.interrupted"),
    ("operation is not supported", "feedback.os.unsupported"),
    ("file ended unexpectedly", "feedback.os.unexpected_eof"),
    ("not enough memory", "feedback.os.out_of_memory"),
    (
        "the drive could not read or write the data",
        "feedback.os.io",
    ),
    ("file or device is busy", "feedback.os.busy"),
    (
        "the source and destination are on different drives",
        "feedback.os.cross_device",
    ),
    ("the drive is unavailable", "feedback.os.no_device"),
    (
        "part of the path is not a folder",
        "feedback.os.not_a_folder",
    ),
    ("item is a folder", "feedback.os.is_a_folder"),
    ("too many files are open", "feedback.os.too_many_open"),
    ("file is too large", "feedback.os.too_large"),
    ("the drive is full", "feedback.os.full"),
    ("the drive is read-only", "feedback.os.read_only"),
    ("that name is too long", "feedback.os.name_too_long"),
    ("folder is not empty", "feedback.os.not_empty"),
    ("the path has too many links", "feedback.os.too_many_links"),
    ("file is no longer available", "feedback.os.stale"),
    ("storage quota exceeded", "feedback.os.quota"),
    (
        "the operation could not finish",
        "feedback.os.could_not_finish",
    ),
];

/// The catalog key for one of src/error.rs's fixed causes, compared whole and case folded.
pub fn os_reason(message: &str) -> Option<&'static str> {
    let text = message.trim().trim_end_matches('.').to_lowercase();
    OS_REASONS
        .iter()
        .find(|(words, _)| *words == text)
        .map(|(_, key)| *key)
}

/// A backend sentence passed through: a known cause gets its key, anything else is a diagnostic inside a localized template.
pub fn passthrough(message: &str) -> Sentence {
    if message.is_empty() {
        return Sentence::key(GENERIC);
    }
    if let Some(key) = os_reason(message) {
        return Sentence::key(key);
    }
    if let Some(sentence) = extract_refusal(message) {
        return sentence;
    }
    if let Some(sentence) = drive_refusal(message) {
        return sentence;
    }
    if let Some(key) = unheld::reason(message) {
        return Sentence::key(key);
    }
    if unheld::recognizes(message) {
        return Sentence {
            cause: Some(0),
            ..Sentence::with("feedback.error.backend", message)
        };
    }
    Sentence::with("feedback.error.backend", &capitalised(message))
}

/// A cause set inside another sentence: a fixed one in the catalog's language without its closing stop where that language has other words, anything else as the backend said it.
pub fn clause(message: &str, catalog: &I18n) -> String {
    let said = if let Some(key) = os_reason(message) {
        catalog.tr(key)
    } else if let Some(refusal) = drive_refusal(message) {
        refusal.render(catalog)
    } else if let Some(said) = unheld::translated(message, catalog) {
        said
    } else {
        return message.to_owned();
    };
    let said = said.trim_end_matches(['.', '。', '।', '۔']);
    if said.to_lowercase() == message.trim().trim_end_matches('.').to_lowercase() {
        message.to_owned()
    } else {
        said.to_owned()
    }
}

// src/backend/extractnames.rs's two refusals; the refused name's reason is fsname's own words, said as drive_refusal reads them.
const TWINS_TAIL: &str = "' would take one name on this drive, so nothing was extracted";
const REFUSED_AT: &str = "' cannot be extracted onto this drive: ";

// Sample input: "'a.txt' and 'A.txt' would take one name on this drive, so nothing was extracted" reads back as archive.done.twins with both names.
/// An extract refused from the archive's index, read back into its key and names; twins split where their lower cases meet.
pub fn extract_refusal(message: &str) -> Option<Sentence> {
    let text = message.trim().strip_prefix('\'')?;
    if let Some(pair) = text.strip_suffix(TWINS_TAIL) {
        let splits: Vec<(&str, &str)> = pair
            .match_indices("' and '")
            .map(|(at, and)| (&pair[..at], &pair[at + and.len()..]))
            .collect();
        let (one, other) = splits
            .iter()
            .find(|(one, other)| one.to_lowercase() == other.to_lowercase())
            .or(splits.first())?;
        return Some(Sentence::args("archive.done.twins", &[*one, *other]));
    }
    let at = text.rfind(REFUSED_AT)?;
    Some(Sentence {
        cause: Some(1),
        ..Sentence::args(
            "archive.done.name_refused",
            &[&text[..at], &text[at + REFUSED_AT.len()..]],
        )
    })
}

/// The backend's own words read as a sentence: one full stop at the end, the first letter raised only on a plain first word, so a leading name with a dot, slash, colon or digit keeps its case; a name that reads as a plain word is still raised, since the text marks no name.
pub fn capitalised(message: &str) -> String {
    let mut chars = message.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let word = message.split_whitespace().next().unwrap_or_default();
    let plain = word
        .chars()
        .all(|c| c.is_alphabetic() || matches!(c, '\'' | '\u{2019}' | '-'));
    let mut out: String = if plain {
        first.to_uppercase().collect()
    } else {
        first.into()
    };
    out.push_str(chars.as_str());
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}

fn lower(message: &str) -> String {
    message.to_lowercase()
}

/// rename gets the errno's "File exists" and mkdir's own collision says "already exists".
pub fn exists(message: &str) -> bool {
    let text = lower(message);
    text.contains("file exists") || text.contains("already exists")
}

pub fn read_only(message: &str) -> bool {
    lower(message).contains("read-only")
}

/// A per-filesystem name refusal from src/backend/fsname.rs carries its own sentence, which drive_refusal reads back.
pub fn name_refusal(message: &str) -> bool {
    [
        "is not allowed in a name",
        "is reserved on this",
        "cannot end in a dot",
        "cannot end in a space",
        "cannot hold control character",
    ]
    .iter()
    .any(|words| message.contains(words))
}

/// The one place the errno wording of a denial is read, so the sentence and the pane state agree.
pub fn denied(place: &str, message: &str) -> bool {
    place == "scan" && lower(message).contains("permission denied")
}

fn share_stale(message: &str) -> bool {
    let text = lower(message);
    text.contains("stale") || text.contains("no longer available")
}

fn share_gone(message: &str) -> bool {
    lower(message).contains("not connected")
}

fn share_silent(message: &str) -> bool {
    let text = lower(message);
    text.contains("timed out") || text.contains("timeout")
}

/// The sentence for a failed request; `named` is the failing directory's leaf, empty when the breadcrumb already names it.
pub fn sentence(place: &str, message: &str, named: &str) -> Sentence {
    let pick = |named_key: &'static str, bare: &'static str| {
        if named.is_empty() {
            Sentence::key(bare)
        } else {
            Sentence::with(named_key, named)
        }
    };
    let keep_or = |fallback: &'static str| {
        if name_refusal(message) || read_only(message) {
            passthrough(message)
        } else if exists(message) {
            Sentence::key("feedback.error.taken")
        } else {
            Sentence::key(fallback)
        }
    };
    match place {
        "scan" if denied(place, message) => pick("feedback.error.denied_named", "pane.locked"),
        "scan" if share_stale(message) => Sentence::key("feedback.error.share_stale"),
        "scan" if share_gone(message) => Sentence::key("feedback.error.share_gone"),
        "scan" if share_silent(message) => Sentence::key("feedback.error.share_silent"),
        "scan" => pick("feedback.error.unread_named", "pane.unread"),
        "sort" => Sentence::key("feedback.error.sort"),
        "read" => Sentence::key("feedback.error.stopped"),
        "undo" | "redo" | "bulkrename" | "window" => passthrough(message),
        "rename" => keep_or("feedback.error.rename"),
        "rename-kept" => Sentence::key("feedback.error.rename_kept"),
        "mkdir" => keep_or("feedback.error.mkdir"),
        "state" => Sentence::key("feedback.error.state"),
        "statefile" => Sentence::key("feedback.error.statefile"),
        "duplicate" if read_only(message) => passthrough(message),
        "duplicate" => Sentence::key("feedback.error.duplicate"),
        "trash" if read_only(message) => passthrough(message),
        "trash" => Sentence::key("feedback.error.trash"),
        "stale" => Sentence::key("feedback.error.stale"),
        "archive" if message == "the backend is not running" => pick(
            "feedback.error.archive_stopped_named",
            "feedback.error.archive_stopped",
        ),
        // The backend names an empty clipboard by its emptyclipboard code and this one message; the paste keys say the empty-clipboard sentence for it, as ui/js/Messages.js routes the code.
        "pasteimage"
            if message
                .trim()
                .trim_end_matches('.')
                .eq_ignore_ascii_case("the clipboard holds no image") =>
        {
            Sentence::key("menu.refusal.empty_clipboard")
        }
        "transfer" | "archive" | "convert" | "link" | "linktarget" | "rename-stranded"
        | "pasteimage" | "searchcontents" => passthrough(message),
        _ => Sentence::key(GENERIC),
    }
}

pub fn listing_state(place: &str, message: &str) -> ListingState {
    if denied(place, message) {
        ListingState::Locked
    } else {
        ListingState::Error
    }
}

/// A mode as ls draws it without the type letter, set-id and sticky bits included.
pub fn permissions(mode: u32) -> String {
    let mut out: Vec<char> = "rwxrwxrwx"
        .chars()
        .enumerate()
        .map(|(at, letter)| {
            if mode & (0o400 >> at) != 0 {
                letter
            } else {
                '-'
            }
        })
        .collect();
    for (bit, execute, at, set, unset) in [
        (0o4000, 0o100, 2, 's', 'S'),
        (0o2000, 0o010, 5, 's', 'S'),
        (0o1000, 0o001, 8, 't', 'T'),
    ] {
        if mode & bit != 0 {
            out[at] = if mode & execute != 0 { set } else { unset };
        }
    }
    out.into_iter().collect()
}

/// Entering needs execute and listing needs read, so an owner holding both would not have been denied.
pub const OWNER_CAN_LIST: u32 = 0o500;

/// The Locked tile's detail line; a zero mode means the stat failed too and earns no line.
pub fn locked_line(mode: u32, catalog: &I18n) -> String {
    let drawn = permissions(mode);
    match mode {
        0 => String::new(),
        _ if mode & OWNER_CAN_LIST == OWNER_CAN_LIST => {
            catalog.tr_args("pane.permissions_other_owner", &[&drawn])
        }
        _ => drawn,
    }
}

/// The credentialed mount helper's exit status: 124 timed out, 126 and 127 could not run, 77 refused the password.
pub fn connect_failure(exit_code: i32) -> &'static str {
    match exit_code {
        124 => "feedback.connect.no_answer",
        126 | 127 => "feedback.connect.no_prompt",
        77 => "feedback.connect.no_password",
        _ => "feedback.connect.unopened",
    }
}
