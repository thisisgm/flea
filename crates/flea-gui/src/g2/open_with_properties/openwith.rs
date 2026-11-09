//! The Open With card: a search, the registered and installed applications under their eyebrows, the default claim and Open.
use super::{
    keys::{edit, Key},
    shell, Effect,
};
use flea::i18n::I18n;
use flea::jsondoc::Json;
use flea::ui_logic_open_with_properties::openwith::{self as logic, Catalogue, Row as Seat};

#[path = "openwith_card.rs"]
mod card;
#[path = "openwith_pointer.rs"]
mod pointer;

/// The board's card width at its 14 body.
pub const WIDTH: f32 = 630.;
/// Seven applications set the list height.
pub const VIEWPORT_ROWS: usize = 7;
/// Filtering keeps the HIG viewport fixed, including both eyebrows, their air and its two hairlines.
pub const VIEWPORT_HEIGHT: f32 = VIEWPORT_ROWS as f32 * shell::APP_ROW
    + 2. * shell::EYEBROW
    + shell::FIRST_EYEBROW_AIR
    + shell::LATER_EYEBROW_AIR
    + 2.;

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
    /// The query the rows were filtered by; a composition shows in the field but filters once it commits.
    filtered: String,
    pub cursor: usize,
    top: usize,
    wheel: pointer::Wheel,
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
            filtered: String::new(),
            cursor: 0,
            top: 0,
            wheel: pointer::Wheel::default(),
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

    pub(crate) fn ready(&self) -> bool {
        self.catalogue.is_some() || self.error.is_some()
    }

    fn refilter(&mut self) {
        self.filtered.clone_from(&self.query);
        self.refresh_rows();
    }

    fn refresh_rows(&mut self) {
        self.rows = self
            .catalogue
            .as_ref()
            .map_or_else(Vec::new, |c| logic::rows(c, &self.filtered));
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
            // OpenWithDialog.qml: Space answers the focused button, and toggles Always from the list, the box or an empty search line.
            Key::Space if self.focus == Stop::Cancel => return self.close(),
            Key::Space if self.focus == Stop::Open => return self.launch(),
            Key::Space if self.focus != Stop::Search || self.query.is_empty() => {
                self.always = !self.always
            }
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

    /// A plain Space on the empty search line is the Always toggle, never a typed character.
    pub(crate) fn space_ticks(&self, event: &gpui::KeyDownEvent) -> bool {
        self.focus == Stop::Search
            && self.query.is_empty()
            && super::keys::key_of(event) == Some(Key::Space)
    }

    /// The search line the window's shared entry edits while Search holds the keys.
    pub(crate) fn ime_value(&self) -> Option<(crate::g2::file_ops::dialog_editor::Slot, &str)> {
        (self.focus == Stop::Search).then_some((
            crate::g2::file_ops::dialog_editor::Slot::OpenWithSearch,
            self.query.as_str(),
        ))
    }

    pub(crate) fn ime_set(&mut self, text: &str, composing: bool) {
        text.clone_into(&mut self.query);
        if !composing && self.filtered != self.query {
            self.refilter();
        }
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
                self.refresh_rows();
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
}
