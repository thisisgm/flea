//! The shelf pile's pinned entries, read for Settings, and the `flea shelf` calls that change them, run in order.
use crate::jsondoc::{self, Json};
use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    pub path: String,
    pub name: String,
    pub folder: bool,
    pub missing: bool,
}

fn pin_leaf(path: &str) -> String {
    super::leaf(path.trim_end_matches('/')).to_owned()
}

// Sample input: {"items":[{"path":"/home/gm/Invoices","name":"Invoices","folder":true,"pinned":true}]}; anything unreadable pins nothing.
pub fn pinned(text: &str) -> Vec<Pin> {
    let Ok(doc) = jsondoc::parse(text) else {
        return Vec::new();
    };
    let Some(items) = doc.get("items").and_then(Json::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter(|item| item.get("pinned").and_then(Json::as_bool) == Some(true))
        .filter_map(|item| {
            let path = item.get("path").and_then(Json::as_str)?;
            Some(Pin {
                path: path.to_owned(),
                name: pin_leaf(path),
                folder: item.get("folder").and_then(Json::as_bool) == Some(true),
                missing: item.get("missing").and_then(Json::as_bool) == Some(true),
            })
        })
        .collect()
}

// The pile file the bar plugin writes: $XDG_STATE_HOME/omarchy/flea-shelf/shelf.json.
pub fn pile_file(state_home: &str, home: &str) -> String {
    let root = if state_home.is_empty() {
        format!("{home}/.local/state")
    } else {
        state_home.to_owned()
    };
    format!("{root}/omarchy/flea-shelf/shelf.json")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    Pin(String),
    Unpin(String),
    Order(String, usize),
}

impl Call {
    // The argv after the flea binary.
    pub fn args(&self) -> Vec<String> {
        match self {
            Self::Pin(path) => vec!["shelf".into(), "pin".into(), path.clone()],
            Self::Unpin(path) => vec!["shelf".into(), "unpin".into(), path.clone()],
            Self::Order(path, to) => {
                vec!["shelf".into(), "order".into(), path.clone(), to.to_string()]
            }
        }
    }
}

// One call runs at a time and a call arriving behind it waits its turn, so a pin and its unpin never land reversed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pins {
    pub records: Vec<Pin>,
    pub running: Option<Call>,
    pub waiting: VecDeque<Call>,
    pub last_error: Option<&'static str>,
}

impl Pins {
    pub fn at(&self, path: &str) -> Option<usize> {
        self.records.iter().position(|pin| pin.path == path)
    }

    // The call to start now, or None when it queued behind one already running or waiting.
    pub fn run(&mut self, call: Call) -> Option<Call> {
        if self.running.is_some() || !self.waiting.is_empty() {
            self.waiting.push_back(call);
            return None;
        }
        self.last_error = None;
        self.running = Some(call.clone());
        Some(call)
    }

    // Rule 4: the pins are a list the operator orders; a move off either end does nothing.
    pub fn move_to(&mut self, path: &str, to: isize) -> Option<Call> {
        let _ = self.at(path)?;
        if to < 0 || to as usize >= self.records.len() {
            return None;
        }
        self.run(Call::Order(path.to_owned(), to as usize))
    }

    pub fn step(&mut self, path: &str, direction: isize) -> Option<Call> {
        let at = self.at(path)? as isize;
        self.move_to(path, at + direction)
    }

    // The running call exited: record its refusal and hand back the next waiting call to start.
    pub fn exited(&mut self, code: i32) -> Option<Call> {
        let ran = self.running.take();
        self.last_error = (code != 0).then_some(match ran {
            Some(Call::Unpin(_)) => "places.shelf.unpin_failed",
            _ => "places.shelf.pin_failed",
        });
        let next = self.waiting.pop_front()?;
        self.running = Some(next.clone());
        Some(next)
    }
}
