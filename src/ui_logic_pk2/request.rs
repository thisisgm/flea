//! Portal request labels and hints (Picker.js port) via picker.browser keys.
use crate::i18n::I18n;
use crate::jsondoc::{self, Json};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FilterDef {
    pub label: String,
    pub globs: Vec<String>,
    pub mimes: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Request {
    pub mode: String,
    pub title: String,
    pub app: String,
    pub accept: String,
    pub multiple: bool,
    pub directory: bool,
    pub hidden: Option<bool>,
    pub folder: String,
    pub file: String,
    pub name: String,
    pub files: Vec<String>,
    pub filters: Vec<FilterDef>,
    pub current: String,
    pub current_index: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Chip {
    pub label: String,
    pub index: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TabStop {
    pub name: String,
    pub visible: bool,
    pub enabled: bool,
    pub tab: bool,
}

// The portal request: every field defaulted, so short arrivals open.
pub fn parse_request(text: &str) -> Request {
    let open = Request {
        mode: "open".to_owned(),
        current_index: -1,
        ..Request::default()
    };
    let Ok(doc) = jsondoc::parse(text.trim()) else { return open };
    if !matches!(doc, Json::Obj(_)) {
        return open;
    }
    let text_of = |key: &str| doc.get(key).and_then(Json::as_str).unwrap_or("").to_owned();
    let mode_raw = text_of("mode");
    Request {
        mode: if mode_raw == "save" || mode_raw == "savefiles" {
            mode_raw
        } else {
            "open".to_owned()
        },
        title: text_of("title"),
        app: text_of("app"),
        accept: text_of("accept"),
        multiple: doc.get("multiple").and_then(Json::as_bool).unwrap_or(false),
        directory: doc.get("directory").and_then(Json::as_bool).unwrap_or(false),
        hidden: doc.get("hidden").and_then(Json::as_bool),
        folder: text_of("folder"),
        file: text_of("file"),
        name: text_of("name"),
        files: strings(doc.get("files")),
        filters: filters(doc.get("filters")),
        current: text_of("current"),
        current_index: match doc.get("currentIndex") {
            Some(Json::Num(literal)) => literal.parse().unwrap_or(-1),
            _ => -1,
        },
    }
}

fn strings(value: Option<&Json>) -> Vec<String> {
    value
        .and_then(Json::as_array)
        .map_or_else(Vec::new, |items| items.iter().filter_map(|item| item.as_str().map(str::to_owned)).collect())
}

// Each filter keeps its own fields; malformed entries never shift.
fn filters(value: Option<&Json>) -> Vec<FilterDef> {
    let Some(items) = value.and_then(Json::as_array) else { return Vec::new() };
    items
        .iter()
        .map(|item| FilterDef {
            label: item.get("label").and_then(Json::as_str).unwrap_or("").to_owned(),
            globs: strings(item.get("globs")),
            mimes: strings(item.get("mimes")),
        })
        .collect()
}

// The caller title wins; the fallback says the purpose.
pub fn title_of(i18n: &I18n, req: &Request) -> String {
    if !req.title.is_empty() {
        return req.title.clone();
    }
    let key = if req.mode == "save" || req.mode == "savefiles" {
        "picker.browser.title_save"
    } else if req.directory {
        "picker.browser.title_folder"
    } else {
        "picker.browser.title_file"
    };
    i18n.tr(key)
}

// Only a portal identity is ever shown; a host app has none.
pub fn subtitle_app(i18n: &I18n, req: &Request) -> Option<String> {
    if req.app.is_empty() {
        None
    } else {
        Some(i18n.tr_args("picker.browser.requested_by", &[&req.app]))
    }
}

pub fn default_accept(i18n: &I18n, req: &Request) -> String {
    let key = if req.mode == "save" {
        "picker.browser.accept_save"
    } else if req.mode == "savefiles" || req.directory {
        "picker.browser.accept_folder"
    } else {
        "picker.browser.accept_open"
    };
    i18n.tr(key)
}

// The primary button; the count rides on it in the multiple case.
pub fn accept_label(i18n: &I18n, req: &Request, count: usize) -> String {
    let base = if req.accept.is_empty() {
        default_accept(i18n, req)
    } else {
        req.accept.clone()
    };
    if req.multiple && count > 1 {
        i18n.tr_args("picker.browser.accept_count", &[&base, &i18n.locale().format_count(count as i64)])
    } else {
        base
    }
}

// The footer's left half: what is checked, and what it weighs.
pub fn status_line(i18n: &I18n, count: usize, bytes: u64) -> String {
    if count == 0 {
        i18n.tr("picker.browser.selected_none")
    } else {
        i18n.tr_args(
            "picker.browser.selected_some",
            &[&i18n.locale().format_count(count as i64), &i18n.locale().format_size(bytes)],
        )
    }
}

// Footer right half: only answered keys, spelled keys.toml-style.
pub fn hints(i18n: &I18n, req: &Request) -> String {
    let key = if req.mode == "save" {
        "picker.browser.hints_save"
    } else if req.multiple {
        if req.directory {
            "picker.browser.hints_open_dir"
        } else {
            "picker.browser.hints_open_many"
        }
    } else {
        "picker.browser.hints_open_one"
    };
    i18n.tr(key)
}

// The filter menu's rows: the caller's filters, then All files, always explicit.
pub fn chips(i18n: &I18n, req: &Request) -> Vec<Chip> {
    if req.filters.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<Chip> = req
        .filters
        .iter()
        .enumerate()
        .map(|(i, f)| Chip {
            label: f.label.clone(),
            index: i as i64,
        })
        .collect();
    out.push(Chip {
        label: i18n.tr("picker.browser.all_files"),
        index: -1,
    });
    out
}

// The rail Recent row is a location token.
pub fn recent_label(i18n: &I18n) -> String {
    i18n.tr("picker.browser.recent")
}

// While a query lives, Esc clears the filter before cancelling.
pub fn hints_filtered(i18n: &I18n) -> String {
    i18n.tr("picker.browser.hints_filtered")
}

// Which filter starts current: the caller's index, then its label, else first.
pub fn current_chip(req: &Request) -> i64 {
    if req.filters.is_empty() {
        return -1;
    }
    if req.current_index >= 0 && (req.current_index as usize) < req.filters.len() {
        return req.current_index;
    }
    for (i, f) in req.filters.iter().enumerate() {
        if f.label == req.current {
            return i as i64;
        }
    }
    0
}

// Tab ring as PickerWindow.qml:837 passes stops; filter has own shortcut.
pub fn tab_order(chrome: &[TabStop], rail: &TabStop, view: &TabStop, save: &[TabStop], answers: &[TabStop]) -> Vec<String> {
    let mut out = Vec::new();
    for stop in chrome
        .iter()
        .chain(std::iter::once(rail))
        .chain(std::iter::once(view))
        .chain(save.iter())
        .chain(answers.iter())
    {
        if stop.visible && stop.enabled && stop.tab {
            out.push(stop.name.clone())
        }
    }
    out
}
