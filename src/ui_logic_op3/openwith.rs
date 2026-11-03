// The Open With card's list: the type's registry first, then every installed application, both filtered by one search.
use crate::i18n::I18n;
use crate::jsondoc::Json;

/// One application as the backend's `applications` reply names it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct App {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub default: bool,
}

impl App {
    // Sample input: {"id":"nvim.desktop","label":"Neovim","icon":"nvim","default":true}; a row without an id is dropped.
    pub fn from_json(value: &Json) -> Option<Self> {
        let text = |key| value.get(key).and_then(Json::as_str).unwrap_or("").to_string();
        let id = text("id");
        if id.is_empty() {
            return None;
        }
        Some(Self { id, label: text("label"), icon: text("icon"), default: value.get("default").and_then(Json::as_bool) == Some(true) })
    }
}

/// One drawn row: an eyebrow caption, or an application holding the cursor seat `at`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Eyebrow { group: Group, rule: bool },
    App { app: usize, group: Group, at: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Registered,
    All,
}

/// The parsed `applications` reply: the item, its kind word and both catalogues.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalogue {
    pub path: String,
    pub kind: String,
    pub mime: String,
    pub handlers: Vec<App>,
    pub installed: Vec<App>,
}

impl Catalogue {
    pub fn from_reply(reply: &Json) -> Self {
        let text = |key| reply.get(key).and_then(Json::as_str).unwrap_or("").to_string();
        let apps = |key| {
            reply.get(key).and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(App::from_json).collect()
        };
        Self { path: text("path"), kind: text("kind"), mime: text("mime"), handlers: apps("applications"), installed: apps("installed") }
    }

    pub fn app(&self, group: Group, index: usize) -> Option<&App> {
        match group {
            Group::Registered => self.handlers.get(index),
            Group::All => self.installed.get(index),
        }
    }
}

// Case-insensitive containment on the trimmed query, as the oracle's toLowerCase indexOf.
fn matching(apps: &[App], needle: &str) -> Vec<usize> {
    apps.iter().enumerate().filter(|(_, app)| needle.is_empty() || app.label.to_lowercase().contains(needle)).map(|(at, _)| at).collect()
}

/// A group that matches nothing takes its eyebrow with it; only the second eyebrow carries the separating rule.
pub fn rows(catalogue: &Catalogue, query: &str) -> Vec<Row> {
    let needle = query.trim().to_lowercase();
    let mut out = Vec::new();
    let mut seat = 0;
    let registered = matching(&catalogue.handlers, &needle);
    if !registered.is_empty() {
        out.push(Row::Eyebrow { group: Group::Registered, rule: false });
        for app in &registered {
            out.push(Row::App { app: *app, group: Group::Registered, at: seat });
            seat += 1;
        }
    }
    let every = matching(&catalogue.installed, &needle);
    if !every.is_empty() {
        out.push(Row::Eyebrow { group: Group::All, rule: !registered.is_empty() });
        for app in &every {
            out.push(Row::App { app: *app, group: Group::All, at: seat });
            seat += 1;
        }
    }
    out
}

/// The cursor seats in draw order; an eyebrow is a caption the cursor steps over.
pub fn applications(rows: &[Row]) -> usize {
    rows.iter().filter(|row| matches!(row, Row::App { .. })).count()
}

/// The row index a cursor seat holds, or None past the last application.
pub fn row_of(rows: &[Row], cursor: usize) -> Option<usize> {
    rows.iter().position(|row| matches!(row, Row::App { at, .. } if *at == cursor))
}

/// The application at a cursor seat.
pub fn chosen<'a>(catalogue: &'a Catalogue, rows: &[Row], cursor: usize) -> Option<&'a App> {
    match rows.get(row_of(rows, cursor)?)? {
        Row::App { app, group, .. } => catalogue.app(*group, *app),
        Row::Eyebrow { .. } => None,
    }
}

/// Seven applications and the eyebrows over them set the list height; a short or empty list still fills it.
pub fn viewport_height(rows: &[Row], applications: usize, row_height: f32, eyebrow_height: f32) -> f32 {
    let mut seen = 0;
    let mut total = 0.;
    for row in rows {
        if seen >= applications {
            break;
        }
        match row {
            Row::Eyebrow { .. } => total += eyebrow_height,
            Row::App { .. } => {
                total += row_height;
                seen += 1;
            }
        }
    }
    total + (applications - seen) as f32 * row_height
}

/// A kind word set mid-sentence starts lower case unless it opens with an initialism, so PNG stays PNG.
pub fn mid_sentence(kind: &str) -> String {
    let mut chars = kind.chars();
    match (chars.next(), chars.next()) {
        (Some(first), Some(second)) if first.is_uppercase() && !second.is_uppercase() => {
            first.to_lowercase().chain(kind[first.len_utf8()..].chars()).collect()
        }
        _ => kind.to_string(),
    }
}

pub fn title(i18n: &I18n, name: &str) -> String {
    if name.is_empty() {
        i18n.tr("dialogs.file.openwith.title_plain")
    } else {
        i18n.tr_args("dialogs.file.openwith.title", &[name])
    }
}

pub fn always_label(i18n: &I18n, kind: &str) -> String {
    if kind.is_empty() {
        i18n.tr("dialogs.file.openwith.always_plain")
    } else {
        i18n.tr_args("dialogs.file.openwith.always", &[&mid_sentence(kind)])
    }
}

pub fn eyebrow(i18n: &I18n, group: Group, kind: &str) -> String {
    match group {
        Group::Registered => i18n.tr_args("dialogs.file.openwith.registered", &[kind]),
        Group::All => i18n.tr("dialogs.file.openwith.all"),
    }
}

pub fn no_match(i18n: &I18n, query: &str) -> String {
    i18n.tr_args("dialogs.file.openwith.no_match", &[query.trim()])
}

/// The two requests the card sends, built once so the wire spelling has one owner.
pub fn applications_request(id: u64) -> String {
    format!(r#"{{"c":"menuaction","op":"applications","id":{id},"installed":true}}"#)
}

pub fn open_request(id: u64, application: &str, always: bool) -> String {
    format!(r#"{{"c":"menuaction","op":"openWith","id":{id},"application":"{}","always":{always}}}"#, crate::json::escape(application))
}

pub fn close_request(id: u64) -> String {
    format!(r#"{{"c":"menuaction","op":"close","id":{id}}}"#)
}

#[path = "openwith_icons.rs"]
pub mod icons;
