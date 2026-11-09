// The collision host: one question in flight or open at a time, and every line it sends in answer.
use super::collide::{self, Choice, Fat, Request};
use super::wire::{Collision, Rename};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Host {
    pub ask_id: u64,
    pub pending: Option<Request>,
    pub opened: bool,
    pub fat_opened: bool,
    pub merging: bool,
    pub spends_cut: bool,
    pub outer: Vec<Collision>,
    pub probe: Vec<String>,
}

// What the host wants the window to do after an input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Nothing,
    // Say the one-question refusal on the status bar.
    Refuse,
    Send(String),
    // Send the answer, and the cut on the clipboard is spent with it.
    SendSpent(String),
    Open {
        total: u64,
        names: Vec<Collision>,
        dest: String,
        merge: bool,
    },
    OpenFat {
        renames: Vec<Rename>,
        fstype: String,
        volume: String,
    },
}

impl Host {
    fn probe(&self) -> Vec<&str> {
        self.probe.iter().map(String::as_str).collect()
    }

    // Sample input: ask(request, &[], false, 4) sends {"c":"collisions","id":1,...} and holds the request.
    pub fn ask(&mut self, request: &Request, probe: &[&str], cut: bool, held: u64) -> Step {
        if collide::refused(self.opened || self.fat_opened, self.pending.is_some()) {
            return Step::Refuse;
        }
        self.ask_id += 1;
        self.pending = Some(request.waiting(held));
        self.spends_cut = cut;
        self.merging = false;
        self.outer.clear();
        self.probe = probe.iter().map(|p| (*p).to_owned()).collect();
        Step::Send(collide::question(
            self.pending.as_ref().unwrap_or(request),
            probe,
            self.ask_id,
        ))
    }

    fn live(&self, id: u64) -> bool {
        id == self.ask_id && self.pending.is_some()
    }

    // Nothing collided: the transfer goes out carrying refuse, so a name that appears meanwhile is refused.
    pub fn answered(
        &mut self,
        id: u64,
        total: u64,
        names: Vec<Collision>,
        mergeable: bool,
    ) -> Step {
        if !self.live(id) {
            return Step::Nothing;
        }
        if total == 0 {
            return self.decide(Choice::Refuse);
        }
        let Some(request) = &self.pending else {
            return Step::Nothing;
        };
        let merge = mergeable && !request.is_link();
        let dest = super::wire_text(request.get("dest"));
        self.outer = names.clone();
        self.opened = true;
        Step::Open {
            total,
            names,
            dest,
            merge,
        }
    }

    pub fn answered_fat(
        &mut self,
        id: u64,
        renames: Vec<Rename>,
        fstype: String,
        volume: String,
    ) -> Step {
        if !self.live(id) {
            return Step::Nothing;
        }
        self.fat_opened = true;
        Step::OpenFat {
            renames,
            fstype,
            volume,
        }
    }

    // A FAT choice other than Cancel asks the question again with the rename rule on it.
    pub fn decide_fat(&mut self, choice: Fat) -> Step {
        self.fat_opened = false;
        let Some(request) = &mut self.pending else {
            return self.decide(Choice::Cancel);
        };
        if choice == Fat::Cancel {
            return self.decide(Choice::Cancel);
        }
        request.set("fat", &collide::text(choice.wire()));
        let request = request.clone();
        let probe = self.probe();
        Step::Send(collide::question(&request, &probe, self.ask_id))
    }

    pub fn answered_merge(&mut self, id: u64, total: u64, names: Vec<Collision>) -> Step {
        if !self.live(id) || !self.merging {
            return Step::Nothing;
        }
        if total == 0 {
            return self.decide_merge(Choice::Refuse);
        }
        let dest = collide::merge_dest(
            &super::wire_text(self.pending.as_ref().and_then(|r| r.get("dest"))),
            &self.outer,
        );
        self.opened = true;
        Step::Open {
            total,
            names,
            dest,
            merge: false,
        }
    }

    // Nothing waits and no card is open, the FAT card included, so the next paste is asked.
    fn clear(&mut self) {
        self.pending = None;
        self.spends_cut = false;
        self.merging = false;
        self.opened = false;
        self.fat_opened = false;
        self.outer.clear();
        self.probe.clear();
    }

    // The card's choice: Merge asks the second walk, Cancel sends nothing, any other answers the request.
    pub fn decide(&mut self, choice: Choice) -> Step {
        if choice == Choice::Merge {
            let Some(request) = &self.pending else {
                return Step::Nothing;
            };
            self.merging = true;
            self.opened = false;
            return Step::Send(collide::merge_question(request, &self.probe(), self.ask_id));
        }
        let step = match (&self.pending, choice) {
            (Some(request), c) if c != Choice::Cancel => {
                self.spent(collide::answer(request, c, self.ask_id))
            }
            _ => Step::Nothing,
        };
        self.clear();
        step
    }

    pub fn decide_merge(&mut self, inner: Choice) -> Step {
        let step = match (&self.pending, inner) {
            (Some(request), c) if c != Choice::Cancel => {
                self.spent(collide::merge_transfer(request, c, self.ask_id))
            }
            _ => Step::Nothing,
        };
        self.clear();
        step
    }

    // The card's own chosen signal routes to the walk it is answering.
    pub fn chosen(&mut self, choice: Choice) -> Step {
        if self.merging {
            self.decide_merge(choice)
        } else {
            self.decide(choice)
        }
    }

    fn spent(&self, line: String) -> Step {
        if self.spends_cut {
            Step::SendSpent(line)
        } else {
            Step::Send(line)
        }
    }

    // A backend that is gone, or a refused question, ends the wait and closes both cards.
    pub fn failed(&mut self, place: &str, input: &str) -> bool {
        if place != "backend" && !collide::dropped_pending(place, input) {
            return false;
        }
        self.clear();
        true
    }

    // The ipc reader's word for where the host stands.
    pub fn state(&self) -> &'static str {
        match (
            self.pending.is_some(),
            self.opened || self.fat_opened,
            self.merging,
        ) {
            (false, _, _) => "idle",
            (true, true, true) => "merge",
            (true, true, false) => "open",
            (true, false, true) => "merging",
            (true, false, false) => "asking",
        }
    }
}
