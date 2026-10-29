// One request, one answer: the reply is on disk before both chooser-owned children are reaped and the window exits.
use super::{reply, RESPONSE_CANCELLED};
use crate::json::escape;

// The line that tells the checks child the request is over, so an in-flight check is cancelled.
pub const CLOSE_LINE: &str = r#"{"c":"picker","op":"close"}"#;

// The two read-only children a chooser owns; neither can write a selected file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Child {
    Checks,
    Listing,
}

// What the front end does next, in order; each runs off the UI thread where it touches a process or a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    CloseChecks,
    WriteReply(String),
    QuitChildren,
    Exit,
}

// A check the window asks the backend for; its id ties the answer to the press that asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check<'a> {
    Mark {
        path: &'a str,
        directory: bool,
        multiple: bool,
    },
    Select {
        paths: &'a [String],
        directory: bool,
    },
    Save {
        folder: &'a str,
        name: &'a str,
    },
    Review {
        review: u64,
    },
    // Re-checks every held mark; an accepting validate answers with the marks it still holds.
    Validate,
}

// Sample output: {"op":"save","folder":"/a","name":"b.txt","c":"picker","id":4}
pub fn check_line(id: u64, check: &Check) -> String {
    let body = match check {
        Check::Mark {
            path,
            directory,
            multiple,
        } => {
            format!(
                r#""op":"mark","path":"{}","directory":{directory},"multiple":{multiple}"#,
                escape(path)
            )
        }
        Check::Select { paths, directory } => format!(r#""op":"select","paths":{},"directory":{directory}"#, crate::json::string_array(paths)),
        Check::Save { folder, name } => format!(
            r#""op":"save","folder":"{}","name":"{}""#,
            escape(folder),
            escape(name)
        ),
        Check::Review { review } => format!(r#""op":"review","review":{review}"#),
        Check::Validate => r#""op":"validate""#.to_owned(),
    };
    format!(r#"{{{body},"c":"picker","id":{id}}}"#)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lifetime {
    answered: bool,
    backend_unavailable: bool,
    quitting: bool,
    stopped: [bool; 2],
    exited: bool,
}

impl Lifetime {
    pub fn answered(&self) -> bool {
        self.answered
    }
    pub fn backend_unavailable(&self) -> bool {
        self.backend_unavailable
    }
    pub fn exited(&self) -> bool {
        self.exited
    }

    // Exactly one answer leaves the window, whichever way it is asked for.
    pub fn finish(&mut self, response: i64, uris: &[String], filter: Option<i64>) -> Vec<Effect> {
        if self.answered {
            return Vec::new();
        }
        let text = reply(response, uris, filter);
        self.answered = true;
        let mut effects = Vec::with_capacity(2);
        if !self.backend_unavailable {
            effects.push(Effect::CloseChecks);
        }
        effects.push(Effect::WriteReply(text));
        effects
    }

    pub fn cancel(&mut self) -> Vec<Effect> {
        self.finish(RESPONSE_CANCELLED, &[], None)
    }

    // A window-manager close takes the same refusal path as Cancel.
    pub fn window_closed(&mut self) -> Vec<Effect> {
        if self.answered {
            return Vec::new();
        }
        self.cancel()
    }

    // Sequenced on the write's completion, saved or failed: a failed write leaves no reply and the portal reports a fault.
    pub fn reply_written(&mut self) -> Vec<Effect> {
        if !self.answered || self.quitting {
            return Vec::new();
        }
        self.quitting = true;
        let mut effects = vec![Effect::QuitChildren];
        effects.extend(self.exit_ready());
        effects
    }

    // A reaped child never stops twice, so the reply's completion or the last stop carries the exit.
    pub fn child_stopped(&mut self, child: Child) -> Vec<Effect> {
        self.stopped[child as usize] = true;
        self.exit_ready()
    }

    // PickerLifecycle's quitReady: the window leaves only after its reply, once both children have stopped, and once.
    fn exit_ready(&mut self) -> Vec<Effect> {
        if self.exited || !self.quitting || !self.stopped.iter().all(|stopped| *stopped) {
            return Vec::new();
        }
        self.exited = true;
        vec![Effect::Exit]
    }

    // True the first time the checks child is lost; every later answer is refused from then on.
    pub fn backend_failed(&mut self) -> bool {
        !std::mem::replace(&mut self.backend_unavailable, true)
    }
}
