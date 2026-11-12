//! Omdrop's status: asked, answered, asked again later while an Off waits on it, and held while `omdrop dir` runs.
use super::super::airdrop::Act;
use super::{Omdrop, Phase};
use flea::ui_logic_send_share::airdrop::Readiness;
use flea::ui_logic_send_share::omdrop;

const RETRY_MS: u64 = 2_000;
const RETRIES: u32 = 5;

// One `omdrop dir` runs at a time: a folder chosen meanwhile waits, the latest replacing any earlier one.
#[derive(Default)]
pub(super) enum Dir {
    #[default]
    Idle,
    Running,
    Queued(String),
}

// What follows an `omdrop dir` that returned: the folder chosen meanwhile, or the one fresh status.
pub enum DirDone {
    Next(Vec<String>),
    Asked(Vec<Act>),
}

// Failed statuses in a row while an `on` or `off` awaits its confirmation, and the token of the one retry timer still
// allowed to ask.
#[derive(Default)]
pub(super) struct Retry {
    failures: u32,
    token: u64,
    armed: Option<u64>,
}

impl Retry {
    // The token only counts up, so a timer armed before a reset never matches a later one.
    pub(super) fn disarm(&mut self) {
        self.failures = 0;
        self.armed = None;
    }
}

impl Omdrop {
    // Omdrop's state can change behind Flea, so its status is asked on every open, but not while on, off or dir runs.
    pub fn ask_status(&mut self) -> Vec<Act> {
        let running = matches!(self.phase, Phase::Starting | Phase::Stopping);
        let dir = !matches!(self.dir, Dir::Idle);
        if !self.active() || self.status_wait.is_some() || running || dir {
            return Vec::new();
        }
        self.retry.armed = None;
        self.status_gen += 1;
        self.status_wait = Some(self.status_gen);
        vec![Act::Doctor(
            self.status_gen,
            omdrop::status_argv(&self.path),
        )]
    }

    // A status asked before an `omdrop on`, `off` or `dir`, or of a replaced omdrop, describes what came before, so its
    // answer must not land and no retry may follow it.
    pub(super) fn forget_status(&mut self) {
        self.status_gen += 1;
        self.status_wait = None;
        self.retry.disarm();
    }

    // Omdrop's status sets the radio and the receiver as Omdrop has them, unless Flea's `omdrop on` or `off` still runs.
    pub fn status_done(
        &mut self,
        generation: u64,
        exit_code: i32,
        stdout: &str,
        now_ms: u64,
    ) -> Vec<Act> {
        if self.status_wait != Some(generation) {
            return Vec::new();
        }
        self.status_wait = None;
        // A failed run leaves Omdrop's state unknown: the confirmed receiver, folder and name stand, but nothing reads ready.
        let Some(status) = omdrop::status(stdout, exit_code) else {
            self.adapter.readiness = Readiness::NotReady;
            let mut acts = vec![Act::Refresh];
            // Only an answer ends Confirming and carries out an Off chosen during it, so one is asked again later.
            if self.phase == Phase::Confirming && self.retry.failures < RETRIES {
                self.retry.failures += 1;
                self.retry.token += 1;
                self.retry.armed = Some(self.retry.token);
                acts.push(Act::StatusLater(self.retry.token, RETRY_MS));
            }
            return acts;
        };
        self.retry.disarm();
        self.adapter = status.adapter.clone();
        let on = status.on;
        self.status = status;
        let mut acts = match (self.phase, on) {
            (Phase::Starting | Phase::Stopping, _) | (Phase::On, true) | (Phase::Off, false) => {
                Vec::new()
            }
            (Phase::Off, true) => {
                self.phase = Phase::On;
                self.browse(now_ms)
            }
            // Flea's own on just came up: its first lookup starts even with the menu shut, so the next open has names.
            (Phase::Confirming, true) => {
                self.phase = Phase::On;
                self.look(now_ms)
            }
            (Phase::On | Phase::Confirming, false) => {
                self.phase = Phase::Off;
                self.drop_devices()
            }
        };
        // An Off chosen while on or off ran is carried out once Omdrop's state is known.
        if self.chosen.take().is_some() {
            acts.extend(self.receiving_off());
        }
        acts.push(Act::Refresh);
        acts
    }

    // A retry's delay passed: it asks only if nothing has asked, answered or changed what it waited for since.
    pub fn status_due(&mut self, token: u64) -> Vec<Act> {
        if self.retry.armed != Some(token) {
            return Vec::new();
        }
        self.retry.armed = None;
        self.ask_status()
    }

    // Save received files to: Omdrop keeps its folder itself, so the choice becomes `omdrop dir`, run now unless one
    // still runs, when it waits for that one to return.
    pub fn folder_argv(&mut self, folder: &str) -> Option<Vec<String>> {
        self.forget_status();
        self.status.dir = folder.to_string();
        if matches!(self.dir, Dir::Idle) {
            self.dir = Dir::Running;
            return Some(omdrop::dir_argv(&self.path, folder));
        }
        self.dir = Dir::Queued(folder.to_string());
        None
    }

    // The `omdrop dir` run on `path` returned: the folder chosen meanwhile runs next, else every status asked before or
    // during the runs is dropped and one fresh one asked. A run of a replaced omdrop changes nothing.
    pub fn folder_done(&mut self, path: &str) -> DirDone {
        if path != self.path {
            return DirDone::Asked(Vec::new());
        }
        self.forget_status();
        match std::mem::take(&mut self.dir) {
            Dir::Queued(folder) => {
                self.dir = Dir::Running;
                DirDone::Next(omdrop::dir_argv(&self.path, &folder))
            }
            Dir::Idle | Dir::Running => DirDone::Asked(self.ask_status()),
        }
    }
}
