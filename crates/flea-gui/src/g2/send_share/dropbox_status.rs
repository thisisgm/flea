//! Dropbox readiness comes from the daemon, using the shipped four-second captured status job.
use super::{
    seams::{Action, RunKind},
    ProviderSend,
};
use flea::{
    i18n::I18n,
    ui_logic_send_share::{dropbox, providers::Role},
};

pub(super) const TIMEOUT_SECONDS: u64 = 4;

pub(super) fn killed(code: i32) -> bool {
    const KILL_SIGNAL: i32 = 9;
    const KILLED_EXIT: i32 = 137;
    code == KILL_SIGNAL || code == KILLED_EXIT
}

#[cfg(test)]
#[path = "dropbox_status_tests.rs"]
mod tests;

#[derive(Default)]
pub(super) struct Daemon {
    pub checking: bool,
    pub status: dropbox::Status,
    answered: bool,
    generation: u64,
    waiting: bool,
}

impl ProviderSend {
    pub(super) fn dropbox_ready(&self) -> bool {
        self.facts.dropbox_ready
            && self.db.answered
            && !self.db.checking
            && self.db.status.reason.is_empty()
    }

    pub(super) fn dropbox_refresh(&mut self) -> Option<Action> {
        if !self.row_on(Role::Dropbox)
            || !self.facts.dropbox_ready
            || self.facts.dropbox_command.is_empty()
        {
            return None;
        }
        if self.db.checking {
            self.db.waiting = true;
            return None;
        }
        self.db.generation += 1;
        self.db.checking = true;
        self.db.answered = false;
        self.db.status = dropbox::Status::default();
        Some(Action::Run(
            RunKind::DropboxStatus(self.db.generation),
            vec![
                "timeout".into(),
                "--signal=KILL".into(),
                TIMEOUT_SECONDS.to_string(),
                self.facts.dropbox_command.clone(),
                "status".into(),
            ],
        ))
    }

    pub(super) fn dropbox_done(
        &mut self,
        generation: u64,
        code: i32,
        stdout: &str,
        stderr: &str,
        i18n: &I18n,
    ) -> Vec<Action> {
        if !self.db.checking || generation != self.db.generation {
            return Vec::new();
        }
        self.db.checking = false;
        self.db.answered = true;
        let timeout = i18n.tr("send.providers.dropbox_timeout");
        let error = if killed(code) {
            timeout.as_str()
        } else {
            stderr
        };
        self.db.status = dropbox::status(stdout, code, error);
        let waited = self.db.waiting && self.menu_open;
        self.db.waiting = false;
        if waited {
            self.dropbox_refresh().into_iter().collect()
        } else {
            Vec::new()
        }
    }
}
