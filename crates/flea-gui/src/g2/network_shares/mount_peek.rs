//! A repair peek's listing opens the share, asks for the bridge once, or names who holds the mount.
use super::mount::{Effect, Mounts, Phase};
use super::mount_lifetime::completion;
use flea::ui_logic_network_shares::smb;

impl Mounts {
    #[allow(clippy::too_many_arguments)]
    pub fn peeked(
        &mut self,
        generation: u64,
        path: &str,
        names: &[String],
        failed: bool,
        login: &str,
        now: u64,
    ) -> Vec<Effect> {
        if generation != self.generation {
            return Vec::new();
        }
        let Some(f) = &mut self.flight else {
            return Vec::new();
        };
        if f.phase == Phase::RepairPath && path == f.path {
            if !failed {
                return self.opened(path);
            }
            let Some((prefix, _)) = path.split_once("/gvfs/") else {
                return self.fail("network.no_folder");
            };
            f.root = format!("{prefix}/gvfs");
            f.phase = Phase::RepairRoot;
            f.due = now + 15_000;
            return vec![Effect::Peek {
                generation,
                path: f.root.clone(),
                count: 512,
            }];
        }
        if f.phase != Phase::RepairRoot || path != f.root {
            return Vec::new();
        }
        if (failed || names.is_empty()) && !f.bridged {
            f.bridged = true;
            f.phase = Phase::Bridge;
            f.due = now + 15_000;
            return vec![Effect::EnsureBridge {
                generation,
                path: path.into(),
                label: f.request.label.clone(),
                origin: f.request.origin,
            }];
        }
        let paths = names
            .iter()
            .map(|n| format!("{path}/{n}"))
            .collect::<Vec<_>>();
        let picked = smb::resolve(&f.request.uri, &paths, login);
        if !picked.path.is_empty() {
            return self.opened(&picked.path);
        }
        let key = if picked.users.is_empty() {
            if f.failed {
                "network.refused"
            } else {
                "network.no_folder"
            }
        } else {
            "network.mounted_as"
        };
        let mut effects = self.fail(key);
        for effect in &mut effects {
            match effect {
                Effect::Completed { args, .. } | Effect::Retry { args, .. }
                    if key == "network.mounted_as" =>
                {
                    args.push(picked.users.join(", "))
                }
                Effect::Say { note, .. } if key == "network.mounted_as" => {
                    note.args.push(picked.users.join(", "))
                }
                _ => {}
            }
        }
        effects
    }
    pub(super) fn opened(&mut self, path: &str) -> Vec<Effect> {
        let f = self.flight.take().unwrap();
        let mut effects = completion(&f.request, None);
        effects.push(Effect::Open {
            path: path.into(),
            uri: f.request.uri,
            label: f.request.label,
            origin: f.request.origin,
        });
        effects
    }
}
