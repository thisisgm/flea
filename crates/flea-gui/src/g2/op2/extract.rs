//! The encrypted archive's card: its name, one masked field with the eye, and Extract.
use super::{
    cards::{Block, Button, Card, Field, Ink},
    keys::Key,
    Effect,
};
use flea::i18n::I18n;
use flea::ui_logic_op2::{archive, cards as logic, ops};

/// The board's card width at its 14 body.
pub const WIDTH: f32 = 588.;

/// The field opens the walk; a disabled stop is skipped, so Tab never lands on a button that does nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    Password,
    Cancel,
    Extract,
}

/// The card over one archive and destination; the secret lives here only while the card is open, so it has no Debug.
#[derive(Clone)]
pub struct Extract {
    path: String,
    dest: String,
    leaf: String,
    password: String,
    held_eye: bool,
    held_marks: usize,
    retry_key: String,
    retry_len: usize,
    error: Option<String>,
    focus: Stop,
    opened: bool,
}

impl Extract {
    pub fn closed() -> Self {
        Self {
            path: String::new(),
            dest: String::new(),
            leaf: String::new(),
            password: String::new(),
            held_eye: false,
            held_marks: 0,
            retry_key: String::new(),
            retry_len: 0,
            error: None,
            focus: Stop::Password,
            opened: false,
        }
    }

    /// A first ask has no sentence; a wrong password on this card's own submit draws bullets, and a second request keeps the open card with the busy sentence.
    pub fn open(&mut self, path: &str, dest: &str, err: &str, i18n: &I18n) -> Vec<Effect> {
        if self.opened {
            return vec![Effect::Say {
                text: super::busy_sentence(i18n),
                error: true,
            }];
        }
        let again = archive::wrong_password(err)
            && !self.retry_key.is_empty()
            && self.retry_key == retry_key(path, dest);
        self.path = path.into();
        self.dest = dest.into();
        self.leaf = logic::leaf_of(path);
        self.password.clear();
        self.held_marks = if again { self.retry_len } else { 0 };
        self.retry_key.clear();
        self.retry_len = 0;
        self.error = if archive::wrong_password(err) {
            Some(logic::wrong_password_sentence(i18n, &self.leaf))
        } else {
            None
        };
        self.focus = Stop::Password;
        self.opened = true;
        Vec::new()
    }

    pub fn opened(&self) -> bool {
        self.opened
    }

    pub fn leaf(&self) -> &str {
        &self.leaf
    }

    pub fn held_marks(&self) -> usize {
        self.held_marks
    }

    /// Types into the field the way keys would, replacing the bullets and dropping the sentence.
    #[cfg(test)]
    pub fn take_secret_for_test(&mut self, secret: &str) {
        self.password = secret.to_string();
        self.held_marks = 0;
        self.error = None;
    }

    fn stops(&self) -> Vec<Stop> {
        vec![Stop::Password, Stop::Cancel, Stop::Extract]
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

    fn commit(&mut self, i18n: &I18n) -> Vec<Effect> {
        if !self.opened {
            return Vec::new();
        }
        // Extract stays live, as the board draws it; an empty field asks for the password.
        if logic::extract_commit(self.password.is_empty()) != logic::Commit::Accept {
            self.error = Some(i18n.tr("archive.type_password"));
            self.focus = Stop::Password;
            return Vec::new();
        }
        let line = ops::submit_extract_password(Some(&self.path), &self.dest, &self.password, 0);
        // Accepted before close, so the owner's submit still sees it; close clears the field.
        let effects = line
            .map(|line| vec![Effect::Send(line)])
            .unwrap_or_default();
        let typed = self.password.chars().count();
        let key = retry_key(&self.path, &self.dest);
        self.close();
        self.retry_key = key;
        self.retry_len = typed;
        effects
    }

    /// The field clears on submit, cancel, esc and the ground, so a secret never outlives the card.
    pub fn close(&mut self) {
        self.password.clear();
        self.held_eye = false;
        self.held_marks = 0;
        self.retry_key.clear();
        self.retry_len = 0;
        self.path.clear();
        self.dest.clear();
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
            Key::Enter | Key::Space if self.focus == Stop::Cancel => {
                self.close();
                vec![Effect::Close]
            }
            Key::Enter | Key::Space if self.focus == Stop::Extract => self.commit(i18n),
            Key::Enter => self.commit(i18n),
            _ => {
                if self.focus == Stop::Password && super::keys::edit(&mut self.password, &key) {
                    self.held_marks = 0;
                    self.error = None;
                }
                Vec::new()
            }
        }
    }

    /// Cancel and Extract answer; the field answers through the key walk.
    pub fn press(&mut self, i18n: &I18n, button: Stop) -> Vec<Effect> {
        match button {
            Stop::Cancel => {
                self.close();
                vec![Effect::Close]
            }
            Stop::Extract => self.commit(i18n),
            _ => Vec::new(),
        }
    }

    /// A click on the button card() draws at this index presses it, as Enter on it would.
    pub fn press_at(&mut self, i18n: &I18n, at: usize) -> Vec<Effect> {
        match [Stop::Cancel, Stop::Extract].get(at) {
            Some(&button) => self.press(i18n, button),
            None => Vec::new(),
        }
    }

    /// The pointer's entry, so the join can move focus without the key walk.
    pub fn focus(&mut self, stop: Stop) {
        if self.opened {
            self.focus = stop;
        }
    }

    /// Holding the field's eye reveals it only while held, as DialogField's pressed state does.
    pub fn eye_press(&mut self) {
        if self.opened {
            self.held_eye = true;
        }
    }

    pub fn eye_release(&mut self) {
        self.held_eye = false;
    }

    pub fn eye_held(&self) -> bool {
        self.held_eye
    }

    /// The extracting card op1's Jobs card draws from one `job` line of kind extract.
    pub fn job_block(
        i18n: &I18n,
        sample: &flea::ui_logic_op2::wire::ExtractSample,
    ) -> super::cards::JobBlock {
        let block = logic::extracting_block(i18n, sample);
        super::cards::JobBlock {
            id: sample.id,
            head: block.head,
            file: block.file,
            byte: block
                .byte
                .iter()
                .map(|part| (part.text.clone(), part.figure))
                .collect(),
            share: block.share,
            cancel: block.cancel,
        }
    }

    /// The card as the shell draws it: the title, one masked field with the eye, the sentence, the buttons.
    pub fn card(&self, i18n: &I18n) -> Card {
        let error = self.error.clone().unwrap_or_default();
        let mut body = vec![Block::Field(Field {
            input: None,
            label: Some(i18n.tr("archive.password")),
            value: self.password.clone(),
            placeholder: String::new(),
            secret: true,
            held: self.held_marks,
            revealed: self.held_eye,
            eye: true,
            error: !error.is_empty(),
            focused: self.focus == Stop::Password,
        })];
        if !error.is_empty() {
            body.push(Block::Note {
                text: error,
                ink: Ink::Error,
            });
        }
        Card {
            language: i18n.language().into(),
            title: logic::encrypted_title(i18n, &self.leaf),
            esc: i18n.tr("dialogs.file.esc"),
            width: WIDTH,
            body,
            body_layout: super::cards::BodyLayout::Parts,
            buttons: vec![
                Button {
                    label: i18n.tr("archive.cancel"),
                    primary: false,
                    focused: self.focus == Stop::Cancel,
                    enabled: true,
                    available: true,
                },
                Button {
                    label: i18n.tr("archive.extract.action"),
                    primary: true,
                    focused: self.focus == Stop::Extract,
                    enabled: true,
                    // A wrong answer's bullets are text the field holds, so Extract stays available over them.
                    available: !self.password.is_empty() || self.held_marks > 0,
                },
            ],
        }
    }
}

fn retry_key(path: &str, dest: &str) -> String {
    format!("{path}\n{dest}")
}

impl Extract {
    pub(crate) fn ime_revealed(&self) -> bool {
        self.held_eye
    }
    pub(crate) fn ime_value(&self) -> Option<(crate::g2::op1::dialog_editor::Slot, &str)> {
        (self.focus == Stop::Password).then_some((
            crate::g2::op1::dialog_editor::Slot::ExtractPassword,
            &self.password,
        ))
    }
    pub(crate) fn ime_set(&mut self, text: &str) {
        self.password = text.into();
        self.held_marks = 0;
        self.error = None;
    }
}

impl Extract {
    /// The extract half of archiveCardsState in ArchivePasswordDialog.qml's fields; eye is the painted eye centre "x y".
    pub fn reading(&self, eye: &str) -> String {
        let focus = match self.focus {
            Stop::Password => 0,
            Stop::Cancel => 1,
            Stop::Extract => 2,
        };
        let error = self.error.as_deref().unwrap_or("");
        format!(
            "\"extractOpened\":{},\"extractLeaf\":\"{}\",\"extractError\":\"{}\",\"extractFocus\":{},\"extractCapture\":{{\"filled\":{},\"fieldError\":{},\"revealed\":{},\"eye\":\"{}\"}},\"canExtract\":{}",
            self.opened,
            flea::json::escape(&self.leaf),
            flea::json::escape(error),
            focus,
            !self.password.is_empty() || self.held_marks > 0,
            !error.is_empty(),
            self.held_eye,
            flea::json::escape(eye),
            self.opened && !self.password.is_empty(),
        )
    }
}
