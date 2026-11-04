//! The transfer card measures its published bytes; wire rates and other job surfaces keep their own contracts.
use flea::ui_logic_op1::{jobs, JobRow};
use std::collections::VecDeque;

const PUBLISH_MS: u64 = 250;
const WINDOW_MS: u64 = 2000;

#[derive(Default)]
pub(super) struct Rates {
    rows: Vec<(u64, Window)>,
}

fn measured(row: &JobRow) -> bool {
    matches!(row.kind.as_str(), "copy" | "move" | "extract")
        && jobs::state(row) == jobs::State::Running
        && row.bytes.is_some()
        && row.total_bytes.is_some()
}

impl Rates {
    pub fn clear(&mut self) {
        self.rows.clear();
    }

    pub fn publish(&mut self, rows: &mut [JobRow], now: u64) {
        self.rows
            .retain(|(id, _)| rows.iter().any(|row| row.id == *id && measured(row)));
        for row in rows.iter_mut().filter(|row| measured(row)) {
            let at = self.rows.iter().position(|(id, _)| *id == row.id);
            let window = match at {
                Some(at) => &mut self.rows[at].1,
                None => {
                    self.rows.push((row.id, Window::default()));
                    &mut self.rows.last_mut().unwrap().1
                }
            };
            window.publish(row, now);
        }
    }

    // An expired beat has already requested a frame; only its committed publication arms the next one.
    pub fn due(&self, now: u64) -> Option<u64> {
        self.rows
            .iter()
            .filter_map(|(_, window)| window.samples.back()?.0.checked_add(PUBLISH_MS))
            .filter(|due| *due > now)
            .min()
    }
}

#[derive(Default)]
struct Window {
    kind: String,
    to: String,
    seen: Option<(u64, u64, u64)>,
    samples: VecDeque<(u64, u64)>,
    bytes: u64,
    total: u64,
    rate: u64,
}

impl Window {
    fn publish(&mut self, row: &mut JobRow, now: u64) {
        let bytes = row.bytes.unwrap();
        if self.kind != row.kind
            || self.to != row.to
            || self
                .seen
                .is_some_and(|(at, old, done)| now < at || bytes < old || row.done < done)
        {
            self.samples.clear();
            self.kind.clone_from(&row.kind);
            self.to.clone_from(&row.to);
        }
        self.seen = Some((now, bytes, row.done));
        if self
            .samples
            .back()
            .is_none_or(|(at, _)| now.saturating_sub(*at) >= PUBLISH_MS)
        {
            self.samples.push_back((now, bytes));
            // Retain the sample just past the edge, exactly as the released TransferCard.qml does.
            while self.samples.len() > 1 && now - self.samples[1].0 > WINDOW_MS {
                self.samples.pop_front();
            }
            let (at, first) = self.samples[0];
            self.rate = if now > at {
                ((u128::from(bytes - first) * 1000 / u128::from(now - at))
                    .min(u128::from(u64::MAX))) as u64
            } else {
                0
            };
            self.bytes = bytes;
            self.total = row.total_bytes.unwrap();
        }
        row.bytes = Some(self.bytes);
        row.total_bytes = Some(self.total);
        row.rate = self.rate;
        row.left = (self.total > 0 && self.rate > 0)
            .then(|| self.total.saturating_sub(self.bytes) / self.rate);
    }
}

#[cfg(test)]
#[path = "card_rate_tests.rs"]
mod tests;
