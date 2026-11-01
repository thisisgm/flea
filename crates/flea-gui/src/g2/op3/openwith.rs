//! The Open With card: a search, the registered and installed applications under their eyebrows, the default claim and Open.
use super::{
    keys::{edit, Key},
    shell::{self, AppRow, Block, Button, Card, Check, Cursor, Glyph, Ink, Tri},
    Effect,
};
use flea::i18n::I18n;
use flea::jsondoc::Json;
use flea::ui_logic_op3::openwith::{self as logic, Catalogue, Row as Seat};

/// The board's card width at its 14 body.
pub const WIDTH: f32 = 630.;
/// Seven applications set the list height.
pub const VIEWPORT_ROWS: usize = 7;

/// Search opens holding the keys, the cursor row grey behind it; the list, the checkbox and the two buttons follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    Search,
    List,
    Always,
    Cancel,
    Open,
}

const STOPS: [Stop; 5] = [
    Stop::Search,
    Stop::List,
    Stop::Always,
    Stop::Cancel,
    Stop::Open,
];

#[derive(Clone, Debug)]
pub struct OpenWith {
    id: u64,
    name: String,
    catalogue: Option<Catalogue>,
    rows: Vec<Seat>,
    pub query: String,
    pub cursor: usize,
    top: usize,
    pub always: bool,
    pub focus: Stop,
    opening: bool,
    /// Set once the catalogue request is out, so a repeated snapshot answer cannot ask twice.
    asked: bool,
    error: Option<String>,
}

impl OpenWith {
    /// The card opens on the list with the search live and asks both catalogues once its snapshot is held, since the backend's one menu slot refuses a second queued request.
    pub fn open(id: u64, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            catalogue: None,
            rows: Vec::new(),
            query: String::new(),
            cursor: 0,
            top: 0,
            always: false,
            focus: Stop::Search,
            opening: false,
            asked: false,
            error: None,
        }
    }

    fn applications(&self) -> usize {
        logic::applications(&self.rows)
    }

    fn refilter(&mut self) {
        self.rows = self
            .catalogue
            .as_ref()
            .map_or_else(Vec::new, |c| logic::rows(c, &self.query));
        self.cursor = self.cursor.min(self.applications().saturating_sub(1));
        self.top = 0;
        self.follow();
    }

    /// The viewport's first row moves only as far as the cursor needs, eyebrow included.
    fn follow(&mut self) {
        let Some(at) = logic::row_of(&self.rows, self.cursor) else {
            return;
        };
        let seat = if at > 0 && matches!(self.rows[at - 1], Seat::Eyebrow { .. }) {
            at - 1
        } else {
            at
        };
        if seat < self.top {
            self.top = seat;
        }
        while self.apps_between(self.top, at + 1) > VIEWPORT_ROWS && self.top < at {
            self.top += 1;
        }
    }

    fn apps_between(&self, from: usize, to: usize) -> usize {
        logic::applications(&self.rows[from..to])
    }

    fn close(&self) -> Vec<Effect> {
        vec![Effect::Send(logic::close_request(self.id)), Effect::Close]
    }

    fn launch(&mut self) -> Vec<Effect> {
        if self.opening {
            return Vec::new();
        }
        let Some(app) = self
            .catalogue
            .as_ref()
            .and_then(|c| logic::chosen(c, &self.rows, self.cursor))
        else {
            return Vec::new();
        };
        let line = logic::open_request(self.id, &app.id, self.always);
        self.opening = true;
        self.error = None;
        vec![Effect::Send(line)]
    }

    /// A click on the drawn button at an index: Cancel closes as Escape does, Open launches the cursor application.
    pub fn press_at(&mut self, at: usize) -> Vec<Effect> {
        match at {
            0 => {
                self.focus = Stop::Cancel;
                self.close()
            }
            1 => {
                self.focus = Stop::Open;
                self.launch()
            }
            _ => Vec::new(),
        }
    }

    pub fn key(&mut self, key: Key) -> Vec<Effect> {
        let at = STOPS.iter().position(|s| *s == self.focus).unwrap_or(0);
        match key {
            Key::Escape => return self.close(),
            Key::Tab => self.focus = STOPS[(at + 1) % STOPS.len()],
            Key::Backtab => self.focus = STOPS[(at + STOPS.len() - 1) % STOPS.len()],
            Key::Enter => {
                return match self.focus {
                    Stop::Cancel => self.close(),
                    Stop::Always => {
                        self.always = !self.always;
                        Vec::new()
                    }
                    Stop::Search | Stop::List | Stop::Open => self.launch(),
                }
            }
            Key::Space if self.focus == Stop::Always => self.always = !self.always,
            Key::Up if matches!(self.focus, Stop::Search | Stop::List) => {
                self.cursor = self.cursor.saturating_sub(1);
                self.follow();
            }
            Key::Down if matches!(self.focus, Stop::Search | Stop::List) => {
                self.cursor = (self.cursor + 1).min(self.applications().saturating_sub(1));
                self.follow();
            }
            Key::Left | Key::Right if matches!(self.focus, Stop::Cancel | Stop::Open) => {
                self.focus = if self.focus == Stop::Cancel {
                    Stop::Open
                } else {
                    Stop::Cancel
                };
            }
            ref typed if self.focus == Stop::Search && edit(&mut self.query, typed) => {
                self.refilter()
            }
            _ => {}
        }
        Vec::new()
    }

    /// Sample input: the applications reply fills the list; an openWith reply closes the card or says why not. Another request's reply is not this card's.
    pub fn reply(&mut self, i18n: &I18n, reply: &Json) -> Vec<Effect> {
        if reply.get("id").and_then(Json::as_f64) != Some(self.id as f64) {
            return Vec::new();
        }
        let ok = reply.get("ok").and_then(Json::as_bool) == Some(true);
        match reply.get("op").and_then(Json::as_str) {
            Some("snapshot") if self.asked => {}
            Some("snapshot") if ok => {
                self.asked = true;
                return vec![Effect::Send(logic::applications_request(self.id))];
            }
            Some("snapshot") => self.error = Some(i18n.tr("dialogs.file.openwith.list_failed")),
            Some("applications") if ok => {
                let catalogue = Catalogue::from_reply(reply);
                self.cursor = catalogue
                    .handlers
                    .iter()
                    .position(|a| a.default)
                    .unwrap_or(0);
                self.catalogue = Some(catalogue);
                self.refilter();
            }
            Some("applications") => self.error = Some(i18n.tr("dialogs.file.openwith.list_failed")),
            Some("openWith") if ok => return vec![Effect::Close],
            Some("openWith") => {
                self.opening = false;
                if reply.get("cancelled").and_then(Json::as_bool) != Some(true) {
                    self.error = Some(i18n.tr("dialogs.file.openwith.open_failed"));
                }
            }
            _ => {}
        }
        Vec::new()
    }

    pub fn card(&self, i18n: &I18n) -> Card {
        let kind = self.catalogue.as_ref().map_or("", |c| c.kind.as_str());
        let search = shell::Field {
            input: None,
            label: None,
            value: self.query.clone(),
            placeholder: i18n.tr("dialogs.file.openwith.search"),
            glyph: Some(Glyph::Search),
            focused: self.focus == Stop::Search,
        };
        let mut body = vec![Block::Field(search)];
        if self.catalogue.is_some() && self.rows.is_empty() {
            body.push(Block::Note {
                text: logic::no_match(i18n, &self.query),
                inset: 0.,
                ink: Ink::Muted,
                lift: 0.,
            });
        } else {
            let (rows, shown) = self.shown(i18n, kind);
            body.push(Block::Apps(
                rows,
                VIEWPORT_ROWS.saturating_sub(shown) as f32 * shell::APP_ROW,
            ));
        }
        let always = Check {
            value: if self.always { Tri::On } else { Tri::Off },
            focused: self.focus == Stop::Always,
        };
        body.push(Block::CheckLine {
            check: always,
            label: logic::always_label(i18n, kind),
        });
        body.push(Block::Note {
            text: i18n.tr("dialogs.file.openwith.undo_note"),
            inset: shell::CHECK + shell::ROW_GAP,
            ink: Ink::Muted,
            lift: 3.,
        });
        if let Some(error) = &self.error {
            body.push(Block::Note {
                text: error.clone(),
                inset: 0.,
                ink: Ink::Error,
                lift: 0.,
            });
        } else if self.opening {
            body.push(Block::Note {
                text: i18n.tr("dialogs.file.openwith.opening"),
                inset: 0.,
                ink: Ink::Muted,
                lift: 0.,
            });
        }
        Card {
            language: i18n.language().to_owned(),
            title: logic::title(i18n, &self.name),
            esc: i18n.tr("dialogs.file.esc"),
            width: WIDTH,
            bleed: true,
            body,
            buttons: vec![
                Button {
                    label: i18n.tr("dialogs.file.cancel"),
                    primary: false,
                    focused: self.focus == Stop::Cancel,
                    enabled: true,
                },
                Button {
                    label: i18n.tr("dialogs.file.openwith.open"),
                    primary: true,
                    focused: self.focus == Stop::Open,
                    enabled: self.can_open(),
                },
            ],
        }
    }

    /// Open submits only with an application under the cursor and no launch already out.
    pub fn can_open(&self) -> bool {
        let chosen = self
            .catalogue
            .as_ref()
            .and_then(|c| logic::chosen(c, &self.rows, self.cursor));
        chosen.is_some() && !self.opening
    }

    /// Whether a launch is out and its answer not yet in.
    pub fn opening(&self) -> bool {
        self.opening
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn catalogue(&self) -> Option<&Catalogue> {
        self.catalogue.as_ref()
    }

    /// The filtered rows, eyebrows included, as ui/OpenWithDialog.qml's rows model holds them.
    pub fn rows(&self) -> &[Seat] {
        &self.rows
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Only the rows the viewport shows are built, seven applications from the first row the cursor keeps in view.
    fn shown(&self, i18n: &I18n, kind: &str) -> (Vec<AppRow>, usize) {
        let Some(catalogue) = &self.catalogue else {
            return (Vec::new(), 0);
        };
        let mut out = Vec::new();
        let mut apps = 0;
        let start = self.top.min(self.rows.len());
        for (at, row) in self.rows[start..].iter().enumerate() {
            if apps == VIEWPORT_ROWS {
                break;
            }
            out.push(match row {
                Seat::Eyebrow { group, .. } => AppRow::Eyebrow {
                    text: logic::eyebrow(i18n, *group, kind),
                    first: at == 0,
                },
                Seat::App {
                    app,
                    group,
                    at: seat,
                } => {
                    apps += 1;
                    let app = catalogue.app(*group, *app).cloned().unwrap_or_default();
                    let tail = if app.default {
                        i18n.tr("dialogs.file.openwith.default")
                    } else {
                        String::new()
                    };
                    let cursor = match (*seat == self.cursor, self.focus) {
                        (false, _) => Cursor::None,
                        (true, Stop::List) => Cursor::Focused,
                        (true, _) => Cursor::Waiting,
                    };
                    let mark = app.icon.is_empty().then_some(Glyph::AppWindow);
                    AppRow::App {
                        label: app.label,
                        tail,
                        cursor,
                        mark,
                        icon: app.icon,
                    }
                }
            });
        }
        (out, apps)
    }
}
