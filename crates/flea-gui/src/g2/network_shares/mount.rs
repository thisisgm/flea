//! One single-flight GIO opener shared by rail and path dispatch; effects cross the worker and pane seams.
use crate::g2::device_rail::{
    process::{Job, Outcome},
    secret::Secret,
};
use flea::ui_logic_device_rail::Note;
use flea::ui_logic_network_shares::{self as logic, smb};
use std::collections::HashMap;

use super::mount_lifetime::completion;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Mount,
    Auth,
    Info,
    Shares,
    RepairPath,
    RepairRoot,
    Bridge,
}
#[derive(Debug)]
pub enum Effect {
    Run {
        generation: u64,
        phase: Phase,
        job: Job,
    },
    Kill(u64),
    Peek {
        generation: u64,
        path: String,
        count: usize,
    },
    EnsureBridge {
        generation: u64,
        path: String,
        label: String,
        origin: u64,
    },
    Open {
        path: String,
        uri: String,
        label: String,
        origin: u64,
    },
    OpenFile {
        path: String,
        uri: String,
        label: String,
        origin: u64,
    },
    RefreshMounts,
    Shares {
        uri: String,
        label: String,
        names: Vec<String>,
        origin: u64,
    },
    Completed {
        id: u64,
        uri: String,
        error: Option<&'static str>,
        args: Vec<String>,
        connect_failed: bool,
    },
    Retry {
        uri: String,
        label: String,
        origin: u64,
        error: &'static str,
        args: Vec<String>,
        connect_failed: bool,
        password: Secret,
    },
    Say {
        note: Note,
        origin: u64,
    },
    /// The bridge's Starting line, which stands with the busy mark until Settled.
    Starting {
        note: Note,
        origin: u64,
    },
    Settled {
        origin: u64,
    },
}

#[derive(Debug)]
pub struct Request {
    pub id: u64,
    pub uri: String,
    pub label: String,
    pub origin: u64,
    pub mounted: bool,
    pub authenticated: bool,
    pub password: Secret,
}
#[derive(Debug)]
pub(super) struct Flight {
    pub(super) request: Request,
    pub(super) phase: Phase,
    pub(super) due: u64,
    pub(super) failed: bool,
    pub(super) path: String,
    pub(super) root: String,
    pub(super) bridged: bool,
}
#[derive(Debug)]
pub(super) struct BridgeWait {
    pub(super) generation: u64,
    pub(super) path: String,
    pub(super) request: Request,
}
#[derive(Debug, Default)]
pub struct Mounts {
    pub generation: u64,
    pub auth_binary: String,
    pub(super) flight: Option<Flight>,
    pub(super) bridges: Vec<BridgeWait>,
    pub(super) browsers: HashMap<u64, super::child::BrowserSession>,
    pub(super) passwords: HashMap<String, Secret>,
}

impl Mounts {
    pub fn stop_all(&mut self) {
        self.flight = None;
        self.bridges.clear();
        self.browsers.clear();
        self.passwords.clear();
        self.generation += 1;
    }
    pub fn busy(&self) -> bool {
        self.flight.is_some()
    }
    pub fn open_share(&mut self, mut request: Request, now: u64) -> Vec<Effect> {
        if let Some(effects) = super::validate::refused(&request) {
            return effects;
        }
        if !super::validate::phone(&request.uri) {
            let address = logic::form::Form::saved_address(&request.uri)
                .ok()
                .or_else(|| super::routing::incomplete(&request.uri))
                .expect("validated address");
            request.uri = address.uri;
            if address.scheme == "nfs" {
                request.password.clear();
                request.authenticated = false;
            }
        }
        if self.busy() {
            return if request.id == 0 {
                vec![Effect::Say {
                    note: Note::new("network.another_opening", &[]),
                    origin: request.origin,
                }]
            } else {
                vec![Effect::Completed {
                    id: request.id,
                    uri: request.uri,
                    error: Some("network.another_opening"),
                    args: Vec::new(),
                    connect_failed: false,
                }]
            };
        }
        request.uri = logic::mounts::normalize(&request.uri);
        self.recall(&mut request);
        self.browsers.remove(&request.origin);
        self.generation += 1;
        let phase = if request.mounted {
            Phase::Info
        } else if request.authenticated
            || (!logic::keyless(&request.uri)
                && (!request.password.is_empty() || logic::credentialed(&request.uri)))
        {
            Phase::Auth
        } else {
            Phase::Mount
        };
        self.flight = Some(Flight {
            request,
            phase,
            due: now + 15_000,
            failed: false,
            path: String::new(),
            root: String::new(),
            bridged: false,
        });
        if phase == Phase::Auth
            && self
                .flight
                .as_ref()
                .is_some_and(|f| f.request.password.is_empty())
        {
            return self.fail("network.password_needed");
        }
        self.run(phase, now)
    }
    pub fn exited(
        &mut self,
        generation: u64,
        phase: Phase,
        out: &Outcome,
        now: u64,
    ) -> Vec<Effect> {
        if generation != self.generation || !self.flight.as_ref().is_some_and(|f| f.phase == phase)
        {
            return Vec::new();
        }
        match phase {
            Phase::Mount => {
                self.flight.as_mut().unwrap().failed = out.code != 0;
                self.run(Phase::Info, now)
            }
            Phase::Auth if out.code == 0 => self.run(Phase::Info, now),
            Phase::Auth => self.fail(match out.code {
                124 => "network.no_answer",
                -1 | 126 | 127 => "network.no_prompt",
                77 => "network.password_refused",
                _ => "network.failed",
            }),
            Phase::Info => {
                let f = self.flight.as_mut().unwrap();
                let path = logic::local_path(&out.stdout);
                if out.code == 0 && !path.is_empty() {
                    f.path = path.into();
                    f.due = now + 15_000;
                    if !smb::matches(&f.request.uri, path) {
                        f.phase = Phase::RepairPath;
                        return vec![Effect::Peek {
                            generation,
                            path: path.into(),
                            count: 1,
                        }];
                    }
                    let effect = Effect::EnsureBridge {
                        generation,
                        path: path.into(),
                        label: f.request.label.clone(),
                        origin: f.request.origin,
                    };
                    let mut f = self.flight.take().unwrap();
                    f.request.password.clear();
                    let mut effects = completion(&f.request, None);
                    self.bridges.push(BridgeWait {
                        generation,
                        path: path.into(),
                        request: f.request,
                    });
                    effects.push(effect);
                    return effects;
                }
                if f.failed && logic::keyless(&f.request.uri) && logic::credentialed(&f.request.uri)
                {
                    return self.fail("network.password_needed");
                }
                if logic::bare_root(&f.request.uri) {
                    return self.run(Phase::Shares, now);
                }
                let key = if f.failed {
                    "network.refused"
                } else {
                    "network.no_folder"
                };
                self.fail(key)
            }
            Phase::Shares => {
                let names: Vec<_> = out
                    .stdout
                    .lines()
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
                // A server that mounts for a guest and lists nothing is asking who is asking, so sign-in is offered, not refused.
                if out.code == 0
                    && names.is_empty()
                    && self.flight.as_ref().is_some_and(|f| guest(&f.request))
                {
                    return self.invite();
                }
                if out.code != 0 || names.is_empty() {
                    return self.fail("network.no_folder");
                }
                let mut f = self.flight.take().unwrap();
                self.remember_browser(&mut f.request, &names);
                let mut effects = completion(&f.request, None);
                effects.push(Effect::Shares {
                    uri: f.request.uri,
                    label: f.request.label,
                    names,
                    origin: f.request.origin,
                });
                effects
            }
            _ => Vec::new(),
        }
    }
}

/// The card's sign-in invitation: a reason in muted ink, never an error, with the caret in Username.
pub const GUEST_EMPTY: &str = "network.guest_empty";

/// An SMB request that carries no credential of any kind, which is what a guest listing is.
fn guest(request: &Request) -> bool {
    flea::ui_logic_network_shares::protocols::scheme_of(&request.uri) == "smb"
        && !request.authenticated
        && request.password.is_empty()
        && !logic::credentialed(&request.uri)
}

impl Mounts {
    pub fn bridge_failed(&mut self, generation: u64) {
        self.bridges.retain(|b| b.generation != generation);
    }
}
