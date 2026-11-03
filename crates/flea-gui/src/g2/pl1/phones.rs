//! The phone rows read off the shared gio listing; a phone only lists and unmounts here, never with -f.
use super::process::{Job, Outcome};
use flea::ui_logic_pl1::mounts::{row_by_key, same_entries};
use flea::ui_logic_pl1::phones::parse_phones;
use flea::ui_logic_pl1::{Kind, Note, RailEntry};

#[derive(Debug)]
pub enum Effect {
    Start(Job),
    Message(Note, bool),
    Released,
}

#[derive(Debug, Default)]
pub struct Phones {
    pub entries: Vec<RailEntry>,
    pub revision: u64,
    pub answered: bool,
    pub running: bool,
    unmount_label: String,
}

impl Phones {
    /// An unchanged listing assigns nothing, so a row being edited keeps what was typed.
    pub fn rebuild(&mut self, listing: &str) {
        self.answered = true;
        let rows = parse_phones(listing);
        if !same_entries(&self.entries, &rows) {
            self.entries = rows;
            self.revision += 1;
        }
    }

    /// The rail hands back a key, resolved fresh because a poll may have rebuilt the rows.
    pub fn release(&mut self, key: &str) -> Vec<Effect> {
        match row_by_key(&self.entries, key) {
            Some(row) => self.unmount(row),
            None => Vec::new(),
        }
    }

    pub fn unmount(&mut self, index: usize) -> Vec<Effect> {
        let Some(e) = self
            .entries
            .get(index)
            .filter(|e| e.kind == Kind::Phone && e.mounted && !self.running)
        else {
            return Vec::new();
        };
        self.unmount_label = e.label.clone();
        self.running = true;
        vec![Effect::Start(Job::plain(&[
            "gio",
            "mount",
            "-u",
            &e.uri.clone(),
        ]))]
    }

    pub fn exited(&mut self, outcome: &Outcome) -> Vec<Effect> {
        self.running = false;
        let mut out = Vec::new();
        if outcome.code != 0 {
            out.push(Effect::Message(
                Note::new("device.transfer_using", &[&self.unmount_label]),
                true,
            ));
        }
        out.push(Effect::Released);
        out
    }
}
