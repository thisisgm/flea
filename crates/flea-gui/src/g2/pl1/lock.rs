//! A lock refusal is judged from a fresh mapping snapshot, since gvfs may have already closed it.
use super::devices::{Devices, Effect, Proc};
use super::process::{Job, Outcome};
use flea::jsondoc::Json;
use flea::ui_logic_pl1::lsblk::{children, device_path, text};
use flea::ui_logic_pl1::unlock::{backing_for, lock_argv, lock_note};

/// Listings begun after a refused lock; if none of them can be read, the refusal is reported unconfirmed.
const WITNESS_LISTINGS: u64 = 3;

impl Devices {
    /// Locks the container behind a released crypt; None when the device has no backing container.
    pub(super) fn lock_crypt(
        &mut self,
        device: &str,
        label: &str,
        listing: &str,
    ) -> Option<Vec<Effect>> {
        let backing = backing_for(listing, device);
        if backing.is_empty() {
            return None;
        }
        if self.running(Proc::Lock) {
            self.lock_queue.push((backing, label.to_string()));
            return Some(Vec::new());
        }
        Some(vec![self.start_lock(backing, label.to_string())])
    }

    fn start_lock(&mut self, backing: String, label: String) -> Effect {
        let argv = lock_argv(&backing);
        self.lock_backing = backing;
        self.lock_label = label;
        self.start(
            Proc::Lock,
            Job::plain(&argv.iter().map(String::as_str).collect::<Vec<_>>()),
        )
    }

    pub fn lock_exited(&mut self, outcome: &Outcome, now: u64) -> Vec<Effect> {
        self.running[Proc::Lock as usize] = false;
        let backing = std::mem::take(&mut self.lock_backing);
        let label = std::mem::take(&mut self.lock_label);
        if outcome.code != 0 && !backing.is_empty() {
            self.lock_checks
                .push((backing, label, self.listings_started + 1));
        }
        let mut out = Vec::new();
        if !self.lock_queue.is_empty() {
            let (backing, label) = self.lock_queue.remove(0);
            out.push(self.start_lock(backing, label));
        }
        out.extend(self.poll(now));
        out
    }

    /// A listing already in flight at the failed exit cannot witness its final mapping state.
    pub(super) fn judge_locks(&mut self, body: Option<&str>) -> Vec<Effect> {
        if self.lock_checks.is_empty() {
            return Vec::new();
        }
        let tree = body.and_then(|body| flea::jsondoc::parse(body).ok());
        let nodes = tree
            .as_ref()
            .and_then(|tree| tree.get("blockdevices"))
            .and_then(Json::as_array);
        let mut out = Vec::new();
        let started = self.listings_started;
        self.lock_checks.retain(|(backing, label, from)| {
            if started < *from {
                return true;
            }
            let open = match nodes {
                Some(nodes) => nodes.iter().any(|node| open_mapping(node, backing)),
                None if started + 1 < *from + WITNESS_LISTINGS => return true,
                None => true,
            };
            if open {
                out.push(Effect::Message(lock_note(label), true));
            }
            false
        });
        out
    }
}

fn open_mapping(node: &Json, backing: &str) -> bool {
    if device_path(node) == backing {
        return children(node).iter().any(has_crypt);
    }
    children(node)
        .iter()
        .any(|child| open_mapping(child, backing))
}

fn has_crypt(node: &Json) -> bool {
    text(node, "type") == "crypt" || children(node).iter().any(has_crypt)
}
