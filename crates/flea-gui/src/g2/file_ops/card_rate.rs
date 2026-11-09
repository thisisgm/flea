//! The transfer card draws one published sample per beat and measures its bytes; wire rates and other job surfaces keep their own contracts.
use flea::ui_logic_file_ops::{jobs, JobRow};
use std::collections::VecDeque;

const PUBLISH_MS: u64 = 250;
const WINDOW_MS: u64 = 2000;
// Per-member plus per-byte cost puts the true time left between the two paces, so they are shown only within this factor.
const PACES_AGREE: u64 = 2;
// Two members finished inside the window bound the member pace; one alone may have started long before the window.
const PACE_MEMBERS: u64 = 2;
// 30,000 stored members extract in under 10 s, so a member's own cost beyond its bytes stays under this ceiling.
const MEMBER_COST_MS: u128 = 1;

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
    samples: VecDeque<(u64, u64, u64)>,
    // The beat's own row: its head, file, count and bytes are drawn together, as TransferCard.qml drew `shown`.
    shown: JobRow,
    rate: u64,
    left: Option<u64>,
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
            .is_none_or(|(at, ..)| now.saturating_sub(*at) >= PUBLISH_MS)
        {
            self.samples.push_back((now, bytes, row.done));
            // Retain the sample just past the edge, exactly as the released TransferCard.qml does.
            while self.samples.len() > 1 && now - self.samples[1].0 > WINDOW_MS {
                self.samples.pop_front();
            }
            let (at, first, first_done) = self.samples[0];
            let span = u128::from(now.saturating_sub(at));
            self.rate = (u128::from(bytes - first) * 1000)
                .checked_div(span)
                .map_or(0, |rate| rate.min(u128::from(u64::MAX)) as u64);
            let total = row.total_bytes.unwrap();
            let bytes_left = total.saturating_sub(bytes);
            let by_bytes = (total > 0 && self.rate > 0).then(|| bytes_left / self.rate);
            // Only an extraction counts every member; copy and move count top-level items.
            let by_members = (row.kind == "extract"
                && row.done.saturating_sub(first_done) >= PACE_MEMBERS
                && row.total > row.done)
                .then(|| {
                    (u128::from(row.total - row.done) * span
                        / u128::from(row.done - first_done)
                        / 1000)
                        .min(u128::from(u64::MAX)) as u64
                });
            let members_left = row.total.saturating_sub(row.done);
            self.left = match (by_bytes, by_members) {
                (Some(bytes), Some(members)) => agreed(bytes, members),
                (bytes, None)
                    if row.kind != "extract"
                        || bytes_speak(row.item_bytes, members_left, bytes_left, self.rate) =>
                {
                    bytes
                }
                _ => None,
            };
            self.shown.clone_from(row);
        }
        let shown = &self.shown;
        row.head.clone_from(&shown.head);
        row.file.clone_from(&shown.file);
        row.done = shown.done;
        row.total = shown.total;
        row.item_bytes = shown.item_bytes;
        row.bytes = shown.bytes;
        row.total_bytes = shown.total_bytes;
        row.rate = self.rate;
        row.left = self.left;
    }
}

// A sub-second pace counts as one second, so 0 s against 2 s still agrees.
fn agreed(bytes: u64, members: u64) -> Option<u64> {
    let (low, high) = (bytes.min(members), bytes.max(members));
    (high <= low.max(1).saturating_mul(PACES_AGREE)).then_some(high)
}

// Without a member pace, the bytes time an extraction only if the member in flight is not far smaller than an average remaining one and the members after it cost no more than the bytes.
fn bytes_speak(item_bytes: u64, members_left: u64, bytes_left: u64, rate: u64) -> bool {
    let factor = u128::from(PACES_AGREE);
    let in_flight = u128::from(item_bytes) * u128::from(members_left);
    let after_ms = u128::from(members_left.saturating_sub(1)) * MEMBER_COST_MS;
    let left = u128::from(bytes_left);
    // A row with no member left to count has only its bytes.
    members_left == 0
        || (left <= in_flight * factor && after_ms * u128::from(rate) <= left * 1000 * (factor - 1))
}

#[cfg(test)]
#[path = "card_rate_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "card_rate_sample_tests.rs"]
mod sample_tests;
