//! Connection completion, cancellation and the deadline each consume the flight once.
use super::mount::{completion, Effect, Mounts};
use flea::ui_logic_pl1::Note;

impl Mounts {
    pub(super) fn fail(&mut self, key: &'static str) -> Vec<Effect> {
        let Some(f) = self.flight.take() else {
            return Vec::new();
        };
        let mut effects = completion(&f.request, Some(key));
        if f.request.id == 0 {
            effects.push(Effect::Retry {
                uri: f.request.uri,
                label: f.request.label,
                origin: f.request.origin,
                error: key,
                args: Vec::new(),
                connect_failed: key != "network.password_needed",
                password: crate::g2::pl1::secret::Secret::new(),
            });
        }
        effects
    }
    pub fn deadline(&self) -> Option<u64> {
        self.flight.as_ref().map(|f| f.due)
    }
    pub fn tick(&mut self, now: u64) -> Vec<Effect> {
        if !self.deadline().is_some_and(|due| now >= due) {
            return Vec::new();
        }
        let mut effects = vec![Effect::Kill(self.generation)];
        let f = self.flight.take().unwrap();
        effects.extend(completion(&f.request, Some("network.no_answer")));
        if f.request.id == 0 {
            effects.push(Effect::Say {
                note: Note::new("network.no_answer", &[]),
                origin: f.request.origin,
            });
        }
        effects
    }
    pub fn cancel(&mut self, id: u64) -> Vec<Effect> {
        if !self.flight.as_ref().is_some_and(|f| f.request.id == id) {
            return Vec::new();
        }
        self.flight = None;
        let generation = self.generation;
        self.generation += 1;
        vec![Effect::Kill(generation)]
    }
}
