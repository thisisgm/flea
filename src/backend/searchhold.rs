// A query asked while the session walk runs waits up to the listing swap's cap for the walk to land; see docs/protocol.md "searchquery".
use crate::backend::events::Event;
use crate::backend::searchreq::{emit_session_answer, Answer};
use crate::backend::searchsession::Session;
use crate::backend::state::{State, Tables};
use crate::backend::thumbs::Pool;
use std::io::Write;
use crate::backend::events::Sender;
use std::time::{Duration, Instant};

// The listing swap's cap, ui/js/Swap.js HOLD_MS, so a key waits no longer for a walk than a folder waits for a listing.
pub(crate) const HOLD: Duration = Duration::from_millis(150);

// The newest query asked while the walk runs, not yet answered; the session's `hold_until` is its cap.
pub(crate) struct Held {
    pub gen: usize,
    pub query: String,
    pub count: usize,
}

impl Session {
    // Answers at once unless a walk runs that will announce its landing and its one hold window is still open; then the query waits.
    pub(crate) fn ask(&mut self, gen: usize, query: &str, count: usize, loop_tx: &Sender<Event>) -> Option<Answer> {
        if gen <= self.last_answered_gen || self.held.as_ref().is_some_and(|held| gen <= held.gen) {
            return None;
        }
        #[cfg(test)]
        if query == super::searchjobs::INJECTED_PANIC_QUERY {
            let mut index = self.index.lock().unwrap();
            index.push("alpha-panic.txt", false);
            panic!("injected panic after index mutation");
        }
        let now = Instant::now();
        // An empty query answers the folder's own listing, and past the cap the walk is a slow source drawn from at once.
        if query.is_empty() || !self.walking() || self.announced || self.hold_until.is_some_and(|until| until <= now) {
            self.held = None;
            return self.query(gen, query, count);
        }
        if self.hold_until.is_none() {
            let until = now + HOLD;
            self.hold_until = Some(until);
            let timer = loop_tx.clone();
            std::thread::spawn(move || {
                // Sleeps to the stamped cap, not from the thread's own start, so the event never arrives before the cap it checks.
                std::thread::sleep(until.saturating_duration_since(Instant::now()));
                let _ = timer.send(Event::SearchHeld);
            });
        }
        self.held = Some(Held { gen, query: query.to_string(), count });
        None
    }

    // The held query answered now: from the whole index once the walk landed, from the partial one past the cap.
    pub(crate) fn answer_held(&mut self) -> Option<Answer> {
        let held = self.held.take()?;
        self.query(held.gen, &held.query, held.count)
    }

    // A stop answers the newest query asked, so a held one becomes the standing query unanswered.
    pub(crate) fn stand_held(&mut self) {
        if let Some(held) = self.held.take() {
            self.last_answered_gen = held.gen;
            self.standing = Some((held.gen, held.query, held.count));
        }
    }
}

// The cap passed with the query still held: the partial index answers it, and the walk's landing re-runs it.
pub fn on_search_held(out: &mut impl Write, st: &mut State, tb: &Tables, pool: &Pool) {
    let answer = match st.search_session.as_mut() {
        Some(session) if session.hold_until.is_some_and(|until| until <= Instant::now()) => session.answer_held(),
        _ => None,
    };
    if let Some(answer) = answer {
        emit_session_answer(out, st, tb, pool, &answer);
    }
}
