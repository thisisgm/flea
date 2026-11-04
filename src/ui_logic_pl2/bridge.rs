//! One bounded bridge ensure; callers run checks off-thread and generation-check every answer.
pub const ENSURE_MS: u64 = 15_000;
pub const STARTING_MS: u64 = 250;
pub const POLL_MS: u64 = 150;

pub fn display_name(label: &str, path: &str) -> String {
    if label.is_empty() {
        path.rsplit('/').next().unwrap_or(path).into()
    } else {
        label.into()
    }
}

pub fn bridge_dir(runtime: &str) -> String {
    if runtime.is_empty() {
        "/run/user/1000/gvfs".into()
    } else {
        format!("{runtime}/gvfs")
    }
}

pub fn needs_bridge(path: &str, root: &str) -> bool {
    !root.is_empty()
        && (path == root
            || path
                .strip_prefix(root)
                .is_some_and(|tail| tail.starts_with('/')))
}

pub fn fuse_argv(binary: &str, dir: &str) -> Vec<String> {
    vec![
        if binary.is_empty() {
            "/usr/lib/gvfsd-fuse".into()
        } else {
            binary.into()
        },
        dir.into(),
        "-f".into(),
    ]
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    Directory,
    File,
    Waiting,
    Classify,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    CheckDirectory,
    CheckFile,
    Start,
    Poll,
    Classify,
    Ready(bool),
    Show,
    Failed,
    Busy,
}
#[derive(Debug, Default)]
pub struct Bridge {
    pub phase: Phase,
    pub starts: usize,
    due: u64,
    show: u64,
    poll: u64,
    shown: bool,
}

impl Bridge {
    pub fn ensure(&mut self, now: u64) -> Action {
        if self.phase != Phase::Idle {
            return Action::Busy;
        }
        self.phase = Phase::Directory;
        self.due = now + ENSURE_MS;
        self.show = now + STARTING_MS;
        self.shown = false;
        Action::CheckDirectory
    }
    pub fn checked(&mut self, found: bool, now: u64) -> Option<Action> {
        match self.phase {
            Phase::Directory if found => {
                self.cancel();
                Some(Action::Ready(true))
            }
            Phase::Directory => {
                self.phase = Phase::File;
                Some(Action::CheckFile)
            }
            Phase::File if found => {
                self.cancel();
                Some(Action::Ready(false))
            }
            Phase::File => {
                self.phase = Phase::Waiting;
                self.starts += 1;
                self.poll = now + POLL_MS;
                Some(Action::Start)
            }
            Phase::Waiting if found => {
                self.phase = Phase::Classify;
                Some(Action::Classify)
            }
            Phase::Waiting => {
                self.poll = now + POLL_MS;
                None
            }
            Phase::Classify => {
                self.cancel();
                Some(Action::Ready(found))
            }
            Phase::Idle => None,
        }
    }
    pub fn tick(&mut self, now: u64) -> Vec<Action> {
        if self.phase == Phase::Idle {
            return Vec::new();
        }
        if now >= self.due {
            self.cancel();
            return vec![Action::Failed];
        }
        let mut out = Vec::new();
        if self.phase == Phase::Waiting {
            if !self.shown && now >= self.show {
                self.shown = true;
                out.push(Action::Show);
            }
            if now >= self.poll {
                self.poll = now + POLL_MS;
                out.push(Action::Poll);
            }
        }
        out
    }
    pub fn deadline(&self) -> Option<u64> {
        (self.phase != Phase::Idle).then(|| {
            if self.phase == Phase::Waiting {
                self.due
                    .min(self.poll)
                    .min(if self.shown { self.due } else { self.show })
            } else {
                self.due
            }
        })
    }
    pub fn cancel(&mut self) {
        self.phase = Phase::Idle;
    }
}
