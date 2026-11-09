//! The With-password compress card: format, name, the password pair and the header switch.
use super::{
    cards::{Block, Button, Card, CheckLine, Chips, Field, Hit, Ink},
    keys::Key,
    Effect,
};
use flea::i18n::I18n;
use flea::ui_logic_archive_flash::{archive, cards as logic, ops};

/// The board's card width at its 14 body.
pub const WIDTH: f32 = 590.;

/// Format opens the walk; a disabled stop is skipped, so Tab never lands on a button that does nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    Format,
    Name,
    Password,
    Confirm,
    Encrypt,
    Cancel,
    Compress,
}

/// The card over resolved paths; the secret lives here only while the card is open, so it has no Debug.
#[derive(Clone)]
pub struct Compress {
    paths: Vec<String>,
    base: String,
    menu_id: u64,
    formats: Vec<String>,
    format: String,
    stem: String,
    name: String,
    password: String,
    confirm: String,
    held_password: bool,
    held_confirm: bool,
    encrypt_names: bool,
    error: Option<String>,
    focus: Stop,
    opened: bool,
}

impl Compress {
    /// Opens over the resolved paths, 7z first, keys on Password; the archive lands in the pane's search base.
    pub fn open(
        paths: Vec<String>,
        pane_base: &str,
        parent_leaf: &str,
        menu_id: u64,
        reported: &[&str],
    ) -> (Self, Vec<Effect>) {
        let base = pane_base.to_string();
        let leaves: Vec<&str> = paths.iter().map(String::as_str).collect();
        let leaves = flea::ui_logic_archive_flash::cards::path_leaves(&leaves);
        let stem = archive::archive_stem(
            &leaves.iter().map(String::as_str).collect::<Vec<_>>(),
            parent_leaf,
        );
        let formats = archive::board_formats(reported);
        let format = if formats.iter().any(|f| f == "7z") {
            "7z".into()
        } else {
            formats[0].clone()
        };
        let name = format!("{stem}.{format}");
        let card = Self {
            paths,
            base,
            menu_id,
            formats,
            format,
            stem,
            name,
            password: String::new(),
            confirm: String::new(),
            held_password: false,
            held_confirm: false,
            encrypt_names: true,
            error: None,
            focus: Stop::Password,
            opened: true,
        };
        (card, Vec::new())
    }

    pub fn opened(&self) -> bool {
        self.opened
    }

    pub fn password_len(&self) -> usize {
        self.password.chars().count()
    }

    fn matched(&self) -> bool {
        self.password == self.confirm
    }

    fn can_compress(&self) -> bool {
        self.opened && self.matched() && !self.password.is_empty() && !self.name.is_empty()
    }

    fn stops(&self) -> Vec<Stop> {
        let mut stops = vec![Stop::Format, Stop::Name, Stop::Password, Stop::Confirm];
        if self.format == "7z" {
            stops.push(Stop::Encrypt);
        }
        stops.extend([Stop::Cancel, Stop::Compress]);
        stops
    }

    fn edit_focused(&mut self, key: &Key) -> bool {
        let field = match self.focus {
            Stop::Name => &mut self.name,
            Stop::Password => &mut self.password,
            Stop::Confirm => &mut self.confirm,
            _ => return false,
        };
        let edited = super::keys::edit(field, key);
        if edited {
            self.error = None;
        }
        edited
    }

    fn step_tab(&mut self, back: bool) {
        let stops = self.stops();
        let at = stops.iter().position(|stop| *stop == self.focus);
        let next = match at {
            None => {
                if back {
                    stops.len() - 1
                } else {
                    0
                }
            }
            Some(at) => (at + if back { stops.len() - 1 } else { 1 }) % stops.len(),
        };
        self.focus = stops[next];
    }

    fn choose_format(&mut self, index: usize) {
        let next = self.formats[index.min(self.formats.len() - 1)].clone();
        if next == self.format {
            return;
        }
        self.format = next.clone();
        self.name = format!("{}.{next}", self.stem);
    }

    fn commit(&mut self, i18n: &I18n) -> Vec<Effect> {
        if !self.opened {
            return Vec::new();
        }
        if !self.can_compress() {
            let refusal = logic::compress_commit(
                self.matched(),
                self.password.is_empty(),
                self.name.is_empty(),
            );
            match &refusal {
                logic::Commit::EmptyName => {
                    self.error = None;
                    self.focus = Stop::Name;
                }
                _ => self.error = Some(logic::compress_refusal(i18n, &refusal)),
            }
            return Vec::new();
        }
        let paths: Vec<&str> = self.paths.iter().map(String::as_str).collect();
        let dest = join(&self.base, &self.name);
        let line = ops::submit_compress_password(
            i18n,
            &ops::CompressRequest {
                paths: &paths,
                dest: &dest,
                format: &self.format,
                password: &self.password,
                encrypt_names: self.encrypt_names && self.format == "7z",
                menu_id: self.menu_id,
            },
            false,
        );
        // Accepted before close, so the owner's submit still sees the source; close clears the pair.
        let effects = line
            .map(|submit| {
                let mut out = vec![Effect::Send(submit.line)];
                if let Some(sticky) = submit.sticky {
                    out.push(Effect::Say {
                        text: sticky,
                        error: false,
                    });
                }
                out
            })
            .unwrap_or_default();
        self.close();
        effects
    }

    /// The fields clear on close, so a secret never outlives the card that took it.
    pub fn close(&mut self) {
        self.password.clear();
        self.confirm.clear();
        self.held_password = false;
        self.held_confirm = false;
        self.stem.clear();
        self.paths.clear();
        self.opened = false;
    }

    pub fn key(&mut self, i18n: &I18n, key: Key) -> Vec<Effect> {
        match key {
            Key::Escape => {
                self.close();
                vec![Effect::Close]
            }
            Key::Tab => {
                self.step_tab(false);
                Vec::new()
            }
            Key::Backtab => {
                self.step_tab(true);
                Vec::new()
            }
            Key::Left if self.focus == Stop::Format => {
                let at = self
                    .formats
                    .iter()
                    .position(|f| *f == self.format)
                    .unwrap_or(0);
                self.choose_format(at.saturating_sub(1));
                Vec::new()
            }
            Key::Right if self.focus == Stop::Format => {
                let at = self
                    .formats
                    .iter()
                    .position(|f| *f == self.format)
                    .unwrap_or(0);
                self.choose_format(at + 1);
                Vec::new()
            }
            Key::Space if self.focus == Stop::Encrypt => {
                self.encrypt_names = !self.encrypt_names;
                Vec::new()
            }
            Key::Enter | Key::Space if self.focus == Stop::Cancel => {
                self.close();
                vec![Effect::Close]
            }
            Key::Enter if self.focus == Stop::Format || self.focus == Stop::Encrypt => {
                self.commit(i18n)
            }
            Key::Space if self.focus == Stop::Compress => self.commit(i18n),
            Key::Enter => self.commit(i18n),
            _ => {
                self.edit_focused(&key);
                Vec::new()
            }
        }
    }

    /// Cancel and Compress answer; every other stop answers through the key walk.
    pub fn press(&mut self, i18n: &I18n, button: Stop) -> Vec<Effect> {
        match button {
            Stop::Cancel => {
                self.close();
                vec![Effect::Close]
            }
            Stop::Compress => self.commit(i18n),
            _ => Vec::new(),
        }
    }

    /// A click on the button card() draws at this index focuses and presses it, as Enter on it would.
    pub fn press_at(&mut self, i18n: &I18n, at: usize) -> Vec<Effect> {
        match [Stop::Cancel, Stop::Compress].get(at) {
            Some(&button) => {
                self.focus(button);
                self.press(i18n, button)
            }
            None => Vec::new(),
        }
    }

    /// A click focuses its control and answers as that control's key: a chip picks, the checkbox toggles.
    pub fn click(&mut self, i18n: &I18n, hit: Hit) -> Vec<Effect> {
        match hit {
            Hit::Button(at) => return self.press_at(i18n, at),
            Hit::Chip(at) => {
                self.focus(Stop::Format);
                self.pick_format(at);
            }
            Hit::Field(at) => {
                if let Some(&field) = [Stop::Name, Stop::Password, Stop::Confirm].get(at) {
                    self.focus(field);
                }
            }
            Hit::Check => {
                self.focus(Stop::Encrypt);
                self.toggle_encrypt();
            }
        }
        Vec::new()
    }

    /// The pointer's entries, so the join can wire taps without the key walk.
    pub fn pick_format(&mut self, index: usize) {
        if self.opened && index < self.formats.len() {
            self.choose_format(index);
        }
    }

    pub fn toggle_encrypt(&mut self) {
        if self.opened && self.format == "7z" {
            self.encrypt_names = !self.encrypt_names;
        }
    }

    fn held_eye(&mut self, field: Stop) -> Option<&mut bool> {
        match field {
            Stop::Password => Some(&mut self.held_password),
            Stop::Confirm => Some(&mut self.held_confirm),
            _ => None,
        }
    }

    /// Holding a field's eye reveals that field alone, as DialogField's pressed state does.
    pub fn eye_press(&mut self, field: Stop) {
        if let Some(held) = self.held_eye(field) {
            *held = true;
        }
    }

    pub fn eye_release(&mut self, field: Stop) {
        if let Some(held) = self.held_eye(field) {
            *held = false;
        }
    }

    pub fn eye_held(&self) -> bool {
        self.held_password || self.held_confirm
    }

    pub fn focus(&mut self, stop: Stop) {
        if self.opened && self.stops().contains(&stop) {
            self.focus = stop;
        }
    }
}

fn join(base: &str, name: &str) -> String {
    if base == "/" {
        return format!("/{name}");
    }
    format!("{base}/{name}")
}

mod ime;
mod reader;

#[path = "compress_view.rs"]
mod view;
