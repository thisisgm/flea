//! The bridge waiter owns the original request identity even after another open.
use super::mount::{Effect, Mounts, Phase};

impl Mounts {
    pub fn bridge_ready(&mut self, generation: u64, path: &str, now: u64) -> Vec<Effect> {
        if let Some(index) = self
            .bridges
            .iter()
            .position(|b| b.generation == generation && b.path == path)
        {
            let b = self.bridges.remove(index);
            return vec![Effect::Open {
                path: path.into(),
                uri: b.request.uri,
                label: b.request.label,
                origin: b.request.origin,
            }];
        }
        if generation != self.generation {
            return Vec::new();
        }
        let Some(f) = &mut self.flight else {
            return Vec::new();
        };
        if f.phase != Phase::Bridge {
            return Vec::new();
        }
        if f.bridged && path == f.root {
            f.phase = Phase::RepairRoot;
            f.due = now + 15_000;
            return vec![Effect::Peek {
                generation,
                path: path.into(),
                count: 512,
            }];
        }
        if path != f.path {
            return Vec::new();
        }
        self.opened(path)
    }
}
