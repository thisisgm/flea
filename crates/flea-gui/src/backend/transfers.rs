//! The paths a backend's transfers read and write, kept from the send until the backend refuses or ends them, so an eject can see them.
use super::Backend;
use flea::json::{field_bool, field_str, field_str_array, field_usize};
use flea::ui_logic_device_rail::mounts::inside;
use std::sync::PoisonError;

/// What a transfer does, which an eject it refuses names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Copy,
    Move,
    Archive,
    Send,
    Action,
}

impl Kind {
    /// Sample input: "copy", "move", "compress", "extract", "send" or "action", as a job or a roots line names it.
    /// Anything else copies, as the backend's own transfer does.
    fn named(kind: &str) -> Self {
        match kind {
            "move" => Self::Move,
            "archive" | "compress" | "extract" => Self::Archive,
            "send" => Self::Send,
            "action" | "actions" => Self::Action,
            _ => Self::Copy,
        }
    }

    /// The catalog key of the refusal that names this kind and the volume it holds.
    pub(crate) fn refusal(self) -> &'static str {
        match self {
            Self::Copy => "device.copy_running",
            Self::Move => "device.move_running",
            Self::Archive => "device.archive_running",
            Self::Send => "device.send_running",
            Self::Action => "device.action_running",
        }
    }
}

/// One transfer, archive, action or send command, from its send until the backend answers it with its real paths.
struct Sent {
    seq: u64,
    command: &'static str,
    kind: Kind,
    dest: String,
    id: Option<usize>,
    /// An action run's or a send's request id, which its actionstarted or roots line names.
    request: Option<usize>,
    /// A plugin place runs this one, so the backend never answers it.
    routed: bool,
    paths: Vec<String>,
    /// A Copy to or Move to menu names no paths: until the backend names them, the listed folder stands for its rows.
    folder: Option<String>,
}

/// What a roots line named: a job's paths while it waits, runs or can be retried, or a shelf drop's until it ends.
struct Held {
    id: usize,
    job: bool,
    kind: Kind,
    live: bool,
    paths: Vec<String>,
}

#[derive(Default)]
pub(crate) struct Transfers {
    sent: Vec<Sent>,
    held: Vec<Held>,
    seq: u64,
    stopped: bool,
}

impl Transfers {
    /// Sample input: {"c":"transfer","op":"copy","paths":["/a/f"],"dest":"/media/S"}; `folder` stands for a menu's selection.
    pub(crate) fn sent(&mut self, line: &str, folder: Option<&str>) -> Option<u64> {
        let command = kept(line).filter(|_| !self.stopped)?;
        let dest = field_str(line, "dest").unwrap_or_default();
        let mut paths = field_str_array(line, "paths");
        paths.extend(field_str(line, "path"));
        paths.push(dest.clone());
        paths.retain(|path| !path.is_empty());
        let kind = match command {
            "transfer" => Kind::named(&field_str(line, "op").unwrap_or_default()),
            other => Kind::named(other),
        };
        self.seq += 1;
        self.sent.push(Sent {
            seq: self.seq,
            command,
            kind,
            dest,
            id: None,
            request: matches!(command, "actions" | "send")
                .then(|| field_usize(line, "id"))
                .flatten(),
            routed: false,
            paths,
            folder: field_usize(line, "menuId").and(folder.map(str::to_owned)),
        });
        Some(self.seq)
    }

    /// A line that never reached the backend is dropped; one a plugin place took waits for that place instead.
    pub(crate) fn placed(&mut self, seq: u64, routed: Result<bool, ()>) {
        match routed {
            Err(()) => self.sent.retain(|sent| sent.seq != seq),
            Ok(true) => {
                if let Some(sent) = self.sent.iter_mut().find(|sent| sent.seq == seq) {
                    sent.routed = true;
                    sent.folder = None;
                }
            }
            Ok(false) => {}
        }
    }

    /// A plugin place answers the lines it took in the order it took them; `line` is the one it has just answered.
    pub(crate) fn routed_done(&mut self, line: &str) {
        let oldest = self.sent.iter().position(|sent| sent.routed);
        if let Some(index) = oldest.filter(|_| kept(line).is_some()) {
            self.sent.remove(index);
        }
    }

    /// Sample input: {"t":"transferstarted","id":12,"n":1,"moving":false}, {"t":"roots","id":12,"job":true,"paths":["/a/f","/media/S"]} or {"t":"error","where":"transfer","path":"",...}.
    /// True for a roots line, which is this record's alone.
    pub(crate) fn answered(&mut self, line: &str) -> bool {
        let id = field_usize(line, "id");
        let path = field_str(line, "path").unwrap_or_default();
        match field_str(line, "t").as_deref() {
            Some("roots") => {
                self.roots(line, id);
                return true;
            }
            Some("transferstarted") => {
                self.start("transfer", id);
                // A shelf drop's line names no op, so its start says whether it moves.
                let moved = field_bool(line, "moving");
                if let Some(sent) = self
                    .sent
                    .iter_mut()
                    .find(|sent| sent.id == id && id.is_some())
                {
                    sent.kind = if moved { Kind::Move } else { Kind::Copy };
                }
            }
            Some("archivestarted") => self.start("archive", id),
            Some("transferdone") => {
                self.end("transfer", id);
            }
            // A disk image is refused with an archivedone under an id no start line gave.
            Some("archivedone") => {
                if !self.end("archive", id) {
                    self.refuse("archive", None);
                }
            }
            // A background run starts job `id`, whose roots line follows; a refused or terminal run holds nothing.
            Some("actionstarted") => {
                let request = field_usize(line, "requestId");
                let run = |sent: &Sent| sent.command == "actions" && sent.request == request;
                match (id, field_bool(line, "ok")) {
                    (Some(id), true) => self
                        .sent
                        .iter_mut()
                        .filter(|sent| run(sent))
                        .for_each(|sent| sent.id = Some(id)),
                    _ => self.sent.retain(|sent| !run(sent)),
                }
            }
            // A refused rclone send starts no job, so no roots line will take its paths over.
            Some("rclonestarted") if !field_bool(line, "ok") => {
                let request = field_usize(line, "requestId");
                if let Some(index) = self.pending_send(request) {
                    self.sent.remove(index);
                }
            }
            Some("job" | "jobs") if self.held.iter().any(|held| held.job) => self.jobs(line),
            Some("error") => match field_str(line, "where").as_deref() {
                // An undo or a link can fail under "transfer" too, naming a path of its own.
                Some("transfer") => self.refuse("transfer", Some(&path)),
                Some("archive") => self.refuse("archive", None),
                Some("stale") if path == "transfer" => self.refuse("transfer", None),
                _ => {}
            },
            _ => {}
        }
        false
    }

    /// The backend process has exited, so nothing it was sent still runs and nothing sent now will.
    pub(crate) fn stop(&mut self) {
        self.stopped = true;
        self.sent.clear();
        self.held.clear();
    }

    /// The backend's own names for what a job or a shelf drop reads and writes replace the client's guess.
    fn roots(&mut self, line: &str, id: Option<usize>) {
        let Some(id) = id.filter(|_| !self.stopped) else {
            return;
        };
        // An action's run ends with its job; a transfer or archive keeps its record for the done line that ends it.
        self.sent
            .retain(|sent| sent.command != "actions" || sent.id != Some(id));
        // A send's job names the request that started it, whose own paths it now holds instead.
        if let Some(index) = self.pending_send(field_usize(line, "requestId")) {
            self.sent.remove(index);
        }
        for sent in self.sent.iter_mut().filter(|sent| sent.id == Some(id)) {
            sent.paths.clear();
            sent.folder = None;
        }
        self.held.retain(|held| held.id != id);
        self.held.push(Held {
            id,
            job: field_bool(line, "job"),
            kind: Kind::named(&field_str(line, "kind").unwrap_or_default()),
            live: true,
            paths: field_str_array(line, "paths"),
        });
    }

    /// Sample input: {"t":"job","job":{"id":12,"kind":"copy",...,"state":"running",...},"unfinished":1} or {"t":"jobs","jobs":[{"id":12,...}],...}.
    /// Each job object opens with its id, as `Job::json` writes it; failed may retry, while done or unlisted lets go.
    fn jobs(&mut self, line: &str) {
        let states: Vec<(usize, String)> = line
            .match_indices(r#"{"id":"#)
            .filter_map(|(at, _)| {
                let job = &line[at..];
                Some((field_usize(job, "id")?, field_str(job, "state")?))
            })
            .collect();
        if field_str(line, "t").as_deref() == Some("jobs") {
            self.held
                .retain(|held| !held.job || states.iter().any(|(id, _)| *id == held.id));
        }
        for (id, state) in states {
            if state == "done" {
                self.held.retain(|held| !held.job || held.id != id);
            }
            for held in self
                .held
                .iter_mut()
                .filter(|held| held.job && held.id == id)
            {
                held.live = matches!(state.as_str(), "waiting" | "running");
            }
        }
    }

    // Commands are read in order and each is answered at once, so an answer belongs to the oldest unanswered command of its kind.
    fn oldest(&self, command: &str) -> Option<usize> {
        self.sent
            .iter()
            .position(|sent| sent.command == command && sent.id.is_none() && !sent.routed)
    }

    /// The oldest send of `request` no roots line has taken over yet.
    fn pending_send(&self, request: Option<usize>) -> Option<usize> {
        request?;
        self.sent
            .iter()
            .position(|sent| sent.command == "send" && sent.request == request)
    }

    fn start(&mut self, command: &str, id: Option<usize>) {
        if let (Some(index), Some(id)) = (self.oldest(command), id) {
            self.sent[index].id = Some(id);
        }
    }

    /// `named` is the path a refusal names: empty, or the refused command's own destination.
    fn refuse(&mut self, command: &str, named: Option<&str>) {
        let Some(index) = self.oldest(command) else {
            return;
        };
        if named.is_none_or(|path| path.is_empty() || path == self.sent[index].dest) {
            self.sent.remove(index);
        }
    }

    /// Whether a record of `command` held the job id `id` and has now ended; a shelf drop of that id ends with it.
    fn end(&mut self, command: &str, id: Option<usize>) -> bool {
        let held = self.sent.len();
        self.sent
            .retain(|sent| sent.command != command || id.is_none() || sent.id != id);
        self.held
            .retain(|held| held.job || id.is_none() || Some(held.id) != id);
        self.sent.len() != held
    }

    fn touch(&self, volume: &str) -> bool {
        !volume.is_empty() && self.touch_where(&|path| inside(path, volume)).is_some()
    }

    /// The kind of the oldest transfer that names a path `on` accepts.
    fn touch_where(&self, on: &dyn Fn(&str) -> bool) -> Option<Kind> {
        let sent = self
            .sent
            .iter()
            .find(|sent| sent.paths.iter().chain(&sent.folder).any(|path| on(path)))
            .map(|sent| sent.kind);
        let held = || {
            self.held
                .iter()
                .filter(|held| held.live)
                .find(|held| held.paths.iter().any(|path| on(path)))
                .map(|held| held.kind)
        };
        sent.or_else(held)
    }
}

/// The command a kept line starts: a transfer, an archive, an action run or a LocalSend, AirDrop or rclone send.
fn kept(line: &str) -> Option<&'static str> {
    let op = field_str(line, "op");
    match field_str(line, "c").as_deref() {
        Some("transfer") => Some("transfer"),
        Some("archive") => Some("archive"),
        Some("actions") if op.as_deref() == Some("run") => Some("actions"),
        Some("localsend" | "airdrop" | "rclone") if op.as_deref() == Some("send") => Some("send"),
        _ => None,
    }
}

impl super::Writable {
    fn newest(&self) -> Option<&str> {
        self.0.last().map(|(path, _)| path.as_str())
    }
}

impl Backend {
    /// Menu producer requests share the ordered backend pipe; a transfer is recorded before it can be answered.
    pub fn line(&self, line: String) -> Result<(), super::Failure> {
        if field_str(&line, "c").as_deref() == Some("formats") {
            self.formats_requests
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let folders = self.folders.lock().unwrap_or_else(PoisonError::into_inner);
        let seq = self.lock().sent(&line, folders.newest());
        drop(folders);
        let placed = self.place(super::Control::Line(line));
        if let Some(seq) = seq {
            let routed = placed.as_ref().map(|routed| *routed).map_err(drop);
            self.lock().placed(seq, routed);
        }
        placed.map(drop)
    }

    /// Whether a transfer sent here and not yet refused or ended reads or writes under `volume`.
    pub fn transfers_on(&self, volume: &str) -> bool {
        self.lock().touch(volume)
    }

    /// The kind of a transfer sent here and not yet refused or ended that names a path `on` accepts.
    pub(crate) fn transfers_where(&self, on: &dyn Fn(&str) -> bool) -> Option<Kind> {
        self.lock().touch_where(on)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Transfers> {
        self.transfers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// A fixture backend has no worker, so a test hands it each reply the worker would read.
    #[cfg(test)]
    pub fn test_answered(&self, line: &str) {
        self.lock().answered(line);
    }
}

#[cfg(test)]
#[path = "transfers_tests.rs"]
mod tests;
