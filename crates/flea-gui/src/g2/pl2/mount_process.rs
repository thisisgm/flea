//! GIO command construction keeps credentials on stdin and fixes the process locale.
use super::mount::{Effect, Mounts, Phase};
use crate::g2::pl1::{process::Job, secret::Secret};

impl Mounts {
    pub(super) fn run(&mut self, phase: Phase, now: u64) -> Vec<Effect> {
        if phase == Phase::Info {
            self.keep_flight();
        }
        let Some(f) = &mut self.flight else {
            return Vec::new();
        };
        f.phase = phase;
        f.due = now + if phase == Phase::Auth { 35_000 } else { 15_000 };
        let uri = f.request.uri.clone();
        let argv: Vec<String> = match phase {
            Phase::Mount if flea::ui_logic_pl2::protocols::scheme_of(&uri) == "smb" => {
                vec![
                    "gio".into(),
                    "mount".into(),
                    "--anonymous".into(),
                    "--".into(),
                    uri,
                ]
            }
            Phase::Mount => vec!["gio".into(), "mount".into(), "--".into(), uri],
            Phase::Auth => vec![
                "timeout".into(),
                "--".into(),
                "35".into(),
                if self.auth_binary.is_empty() {
                    "/usr/lib/flea/flea-gio-auth".into()
                } else {
                    self.auth_binary.clone()
                },
                uri,
            ],
            Phase::Info => vec!["gio".into(), "info".into(), "--".into(), uri],
            Phase::Shares => vec!["gio".into(), "list".into(), "--".into(), uri],
            _ => return Vec::new(),
        };
        let mut job = Job::c_locale(argv);
        if phase == Phase::Auth {
            let mut input = Secret::from_bytes(f.request.password.expose());
            input.push_str("\n");
            job.stdin = Some(input);
        }
        vec![Effect::Run {
            generation: self.generation,
            phase,
            job,
        }]
    }
}
