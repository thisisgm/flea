//! The compositor-presented frames that first showed a search generation's rows and its settled answer.
use flea::{json::escape, ui_logic_sr1::Session};

/// One presented submission: its frame id and, when the compositor gave them, its time and clock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    pub frame: u64,
    pub at_ns: Option<u64>,
    pub clock: Option<u32>,
}

impl Shown {
    fn json(&self) -> String {
        let or_null =
            |value: Option<u64>| value.map_or("null".to_owned(), |value| value.to_string());
        format!(
            "{{\"frame\":{},\"ns\":{},\"clock\":{}}}",
            self.frame,
            or_null(self.at_ns),
            or_null(self.clock.map(u64::from))
        )
    }
}

/// What one presented scene held of a search: its generation, whether its answer had settled, its top painted row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scene {
    pub gen: u64,
    pub settled: bool,
    pub row: usize,
    pub name: String,
}

/// The newest generation's record; a scene of any other generation never credits it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Presented {
    gen: u64,
    first: Option<(Shown, usize, String)>,
    settled: Option<Shown>,
    // The window's lost-feedback count when this generation was first seen, and now.
    blind_base: Option<u64>,
    blind: u64,
    // The oldest submission still awaiting its terminal, which may yet prove to be first.
    oldest: Option<u64>,
    // Feedback stopped (the window closed) while this generation was observed.
    unobserved: bool,
}

// Frame ids order submissions, not presentation, so the earlier compositor time wins when both are known.
fn earlier(new: &Shown, old: &Shown) -> bool {
    match (new.at_ns, old.at_ns) {
        (Some(new), Some(old)) => new < old,
        _ => new.frame < old.frame,
    }
}

impl Presented {
    fn start(&mut self, current: u64) {
        if self.gen != current {
            let (blind, oldest) = (self.blind, self.oldest);
            *self = Self {
                gen: current,
                blind_base: Some(blind),
                blind,
                oldest,
                ..Self::default()
            };
        }
    }

    /// A lost submission or terminal after this generation began means no presented frame is provably its first.
    pub fn coverage(&mut self, current: u64, blind: u64, oldest: Option<u64>, observed: bool) {
        if !observed {
            // Without an observer nothing is claimed, and an observed generation can no longer be completed.
            self.unobserved |= self.blind_base.is_some();
            return;
        }
        (self.blind, self.oldest) = (blind, oldest);
        if current != 0 {
            self.start(current);
        }
    }

    pub fn record(&mut self, current: u64, scene: &Scene, shown: Shown) {
        if scene.gen != current || current == 0 {
            return;
        }
        self.start(current);
        if self
            .first
            .as_ref()
            .is_none_or(|(old, ..)| earlier(&shown, old))
        {
            self.first = Some((shown, scene.row, scene.name.clone()));
        }
        if scene.settled && self.settled.as_ref().is_none_or(|old| earlier(&shown, old)) {
            self.settled = Some(shown);
        }
    }

    /// flea.searchStamps: the stopwatch first, then the answer and the presented frames, all of the newest gen.
    pub fn stamps_json(&self, session: &Session) -> String {
        let stamps = session.stamps;
        let mine = self.gen == stamps.gen && stamps.gen != 0;
        let first = self.first.as_ref().filter(|_| mine);
        let settled = self.settled.as_ref().filter(|_| mine);
        let waiting = |frame: u64| self.oldest.is_some_and(|oldest| oldest < frame);
        let unproven = first.is_some_and(|(shown, ..)| waiting(shown.frame))
            || settled.is_some_and(|shown| waiting(shown.frame));
        let coverage = match self.blind_base.filter(|_| mine) {
            None => "none",
            Some(base) if self.blind > base || self.unobserved => "lost",
            // An earlier submission without its terminal could still be the first one shown.
            Some(_) if unproven => "pending",
            Some(_) => "complete",
        };
        // The answer fields belong to the gen that answered, so an unanswered newest gen reports none of them.
        let answered = session.answered == stamps.gen && stamps.gen != 0;
        let fact = |value: String| if answered { value } else { "null".to_owned() };
        format!(
            concat!(
                "{{\"gen\":{},\"keyAt\":{},\"firstRowsAt\":{},\"settledAt\":{},",
                "\"query\":\"{}\",\"indexed\":{},\"total\":{},\"walking\":{},\"cancelled\":{},\"partial\":{},",
                "\"firstRow\":{},\"firstName\":{},\"presentedFirst\":{},\"presentedSettled\":{},\"coverage\":\"{}\"}}"
            ),
            stamps.gen,
            stamps.key_at,
            stamps.first_rows_at,
            stamps.settled_at,
            escape(&session.query),
            fact(session.scanned.to_string()),
            fact(session.total.to_string()),
            fact(session.running.to_string()),
            fact(session.cancelled.to_string()),
            fact(session.partial.as_deref().map_or("false".to_owned(), |reason| format!("\"{}\"", escape(reason)))),
            first.map_or("null".to_owned(), |(_, row, _)| row.to_string()),
            first.map_or("null".to_owned(), |(.., name)| format!("\"{}\"", escape(name))),
            first.map_or("null".to_owned(), |(shown, ..)| shown.json()),
            settled.map_or("null".to_owned(), Shown::json),
            coverage,
        )
    }
}
