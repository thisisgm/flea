// The opener lane: one `flea --open` at a time, its exit read as the pane's next step.
use super::runfile::{ASK_OPEN_WITH, ASK_RUN, IS_DIRECTORY};

/// What the pane does once `flea --open` has exited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Opened {
    Done,
    Directory(String),
    AskRun { path: String, text: String },
    AskOpenWith(String),
    Failed(String),
}

/// The single-flight guard: a second open while one is pending is refused out loud, never queued.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Opener {
    current: String,
    pending: bool,
}

impl Opener {
    pub fn pending(&self) -> bool {
        self.pending
    }

    /// The argv to spawn, or None when a launch is already pending and the caller says busy.
    pub fn start(&mut self, flea: &str, paths: &[String]) -> Option<Vec<String>> {
        if self.pending || paths.is_empty() {
            return None;
        }
        self.current = paths[0].clone();
        self.pending = true;
        let mut argv = vec![flea.to_string(), "--open".to_string()];
        argv.extend(paths.iter().cloned());
        Some(argv)
    }

    /// An ask status without its line is a launch that died mid-answer, so it is the plain failure.
    pub fn exited(&mut self, status: Option<i32>, stdout: &str) -> Option<Opened> {
        if !self.pending {
            return None;
        }
        self.pending = false;
        let path = std::mem::take(&mut self.current);
        Some(match status {
            Some(0) => Opened::Done,
            Some(IS_DIRECTORY) => Opened::Directory(path),
            Some(ASK_RUN) if !stdout.is_empty() => Opened::AskRun { path, text: stdout.to_string() },
            Some(ASK_OPEN_WITH) => Opened::AskOpenWith(path),
            _ => Opened::Failed(path),
        })
    }
}

/// The pane's line for an open dropped while another is pending, in the plain role.
pub fn busy_text(i18n: &crate::i18n::I18n) -> String {
    i18n.tr("dialogs.file.open.busy")
}

/// The pane's line for a launch that failed; the path was proved before, so no cause is named.
pub fn failed_text(i18n: &crate::i18n::I18n) -> String {
    i18n.tr("dialogs.file.open.failed")
}
