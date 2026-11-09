//! The paths a backend's transfers read and write, kept from the send until the backend refuses or ends them, so an eject can see them.
use super::Backend;
use flea::json::{field_str, field_str_array, field_usize};
use flea::ui_logic_device_rail::mounts::inside;
use std::sync::PoisonError;

/// One transfer or archive command: its command name, its destination, the job id its start line gave it, and every path it names.
struct Sent {
    command: &'static str,
    dest: String,
    id: Option<usize>,
    paths: Vec<String>,
}

#[derive(Default)]
pub(crate) struct Transfers {
    sent: Vec<Sent>,
    stopped: bool,
}

impl Transfers {
    /// Sample input: {"c":"transfer","op":"copy","paths":["/a/f"],"dest":"/media/S"}; `folder` stands for a menu's selection.
    pub(crate) fn sent(&mut self, line: &str, folder: Option<&str>) {
        let command = match field_str(line, "c").as_deref() {
            Some("transfer") => "transfer",
            Some("archive") => "archive",
            _ => return,
        };
        if self.stopped {
            return;
        }
        let dest = field_str(line, "dest").unwrap_or_default();
        let mut paths = field_str_array(line, "paths");
        paths.extend(field_str(line, "path"));
        paths.push(dest.clone());
        // A Copy to or Move to menu names no paths: the backend takes the rows of its listed folder.
        if field_usize(line, "menuId").is_some() {
            paths.extend(folder.map(str::to_owned));
        }
        paths.retain(|path| !path.is_empty());
        self.sent.push(Sent {
            command,
            dest,
            id: None,
            paths,
        });
    }

    /// Sample input: {"t":"transferstarted","id":12,"n":1,"moving":false}, {"t":"transferdone","id":12,...} or {"t":"error","where":"transfer","path":"",...}.
    pub(crate) fn answered(&mut self, line: &str) {
        let id = field_usize(line, "id");
        let path = field_str(line, "path").unwrap_or_default();
        match field_str(line, "t").as_deref() {
            Some("transferstarted") => self.start("transfer", id),
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
            Some("error") => match field_str(line, "where").as_deref() {
                // An undo or a link can fail under "transfer" too, naming a path of its own.
                Some("transfer") => self.refuse("transfer", Some(&path)),
                Some("archive") => self.refuse("archive", None),
                Some("stale") if path == "transfer" => self.refuse("transfer", None),
                _ => {}
            },
            _ => {}
        }
    }

    /// The backend process has exited, so nothing it was sent still runs and nothing sent now will.
    pub(crate) fn stop(&mut self) {
        self.stopped = true;
        self.sent.clear();
    }

    // Commands are read in order and each is answered at once, so an answer belongs to the oldest unanswered command of its kind.
    fn oldest(&self, command: &str) -> Option<usize> {
        self.sent
            .iter()
            .position(|sent| sent.command == command && sent.id.is_none())
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

    /// Whether a record of `command` held the job id `id` and has now ended.
    fn end(&mut self, command: &str, id: Option<usize>) -> bool {
        let held = self.sent.len();
        self.sent
            .retain(|sent| sent.command != command || id.is_none() || sent.id != id);
        self.sent.len() != held
    }

    fn touch(&self, volume: &str) -> bool {
        !volume.is_empty()
            && self
                .sent
                .iter()
                .any(|sent| sent.paths.iter().any(|path| inside(path, volume)))
    }
}

impl super::Writable {
    fn newest(&self) -> Option<&str> {
        self.0.last().map(|(path, _)| path.as_str())
    }
}

impl Backend {
    pub(super) fn record_transfer(&self, line: &str) {
        let folders = self.folders.lock().unwrap_or_else(PoisonError::into_inner);
        self.transfers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .sent(line, folders.newest());
    }

    /// Whether a transfer sent here and not yet refused or ended reads or writes under `volume`.
    pub fn transfers_on(&self, volume: &str) -> bool {
        self.transfers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .touch(volume)
    }

    /// A fixture backend has no worker, so a test hands it each reply the worker would read.
    #[cfg(test)]
    pub fn test_answered(&self, line: &str) {
        self.transfers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .answered(line);
    }
}

#[cfg(test)]
mod tests {
    use super::Transfers;

    const ONTO: &str = r#"{"c":"transfer","op":"copy","paths":["/home/u/a"],"dest":"/media/s/in"}"#;
    const ELSEWHERE: &str = r#"{"c":"transfer","op":"copy","paths":["/home/a"],"dest":"/home/b"}"#;
    const STARTED: &str = r#"{"t":"transferstarted","id":12,"n":1,"moving":false}"#;
    const DONE: &str =
        r#"{"t":"transferdone","id":12,"ok":1,"failed":0,"skipped":0,"cancelled":false}"#;
    const IDLE: &str = r#"{"t":"jobs","jobs":[],"unfinished":0,"newest":null,"head":""}"#;
    const BUSY: &str =
        r#"{"t":"error","where":"transfer","path":"","msg":"an operation is already running"}"#;

    #[test]
    fn a_transfer_names_its_sources_and_destination_until_it_ends() {
        let mut kept = Transfers::default();
        kept.sent(r#"{"c":"thumb","path":"/media/s/a.jpg"}"#, None);
        assert!(
            !kept.touch("/media/s"),
            "only transfers and archives are kept"
        );
        kept.sent(ONTO, None);
        assert!(kept.touch("/media/s"), "sent and not yet answered");
        kept.answered(IDLE);
        assert!(
            kept.touch("/media/s"),
            "an idle line written before the backend read the transfer ends nothing"
        );
        kept.answered(STARTED);
        assert!(kept.touch("/media/s") && kept.touch("/home/u"));
        assert!(!kept.touch("/media/st"));
        kept.answered(r#"{"t":"transferdone","id":9,"ok":1}"#);
        assert!(
            kept.touch("/media/s"),
            "another job's end ends nothing here"
        );
        kept.answered(DONE);
        assert!(!kept.touch("/media/s"), "its own end lets it go");
        kept.sent(
            r#"{"c":"transfer","op":"move","menuId":4,"dest":"/home/u"}"#,
            Some("/media/s/dcim"),
        );
        assert!(
            kept.touch("/media/s"),
            "a menu's rows come from the listed folder"
        );
    }

    #[test]
    fn a_refusal_lets_go_of_the_oldest_unanswered_transfer_only() {
        let mut kept = Transfers::default();
        kept.sent(ONTO, None);
        kept.sent(
            r#"{"c":"transfer","op":"copy","paths":["/home/a"],"dest":"/media/t"}"#,
            None,
        );
        kept.answered(BUSY);
        assert!(!kept.touch("/media/s"), "the refusal answers the oldest");
        assert!(kept.touch("/media/t"), "the newer one is still unread");
        kept.answered(r#"{"t":"error","where":"stale","path":"transfer","msg":"x"}"#);
        assert!(!kept.touch("/media/t"), "a stale-row refusal is a refusal");
        kept.sent(ONTO, None);
        kept.answered(r#"{"t":"error","where":"rename","path":"/home/a","msg":"x"}"#);
        assert!(
            kept.touch("/media/s"),
            "another command's error refuses nothing"
        );
        kept.answered(r#"{"t":"error","where":"transfer","path":"/home/u/b.txt","msg":"x"}"#);
        assert!(
            kept.touch("/media/s"),
            "an undo failing under transfer names its own path"
        );
        kept.answered(
            r#"{"t":"error","where":"transfer","path":"/media/s/in","msg":"/media/s is not responding."}"#,
        );
        assert!(
            !kept.touch("/media/s"),
            "a refusal naming the destination is this one's"
        );
    }

    #[test]
    fn a_backend_that_exited_holds_nothing() {
        let mut kept = Transfers::default();
        kept.sent(ONTO, None);
        kept.answered(STARTED);
        kept.stop();
        assert!(!kept.touch("/media/s"), "its copy died with it");
        kept.sent(ONTO, None);
        assert!(
            !kept.touch("/media/s"),
            "a line sent after the exit never runs"
        );
    }

    #[test]
    fn a_start_pairs_with_its_own_command_kind() {
        let mut kept = Transfers::default();
        kept.sent(
            r#"{"c":"archive","op":"extract","path":"/home/u/x.zip","dest":"/media/t"}"#,
            None,
        );
        kept.sent(ONTO, None);
        kept.answered(STARTED);
        kept.answered(r#"{"t":"archivestarted","id":13}"#);
        kept.answered(DONE);
        assert!(
            !kept.touch("/media/s"),
            "the copy's start and end were its own"
        );
        assert!(kept.touch("/media/t"), "the extract still runs");
        kept.answered(r#"{"t":"archivedone","id":13,"ok":true,"verified":true,"err":""}"#);
        assert!(!kept.touch("/media/t"));
        kept.sent(
            r#"{"c":"archive","op":"extract","path":"/media/t/disk.iso","dest":"/home/u"}"#,
            None,
        );
        kept.sent(ONTO, None);
        kept.answered(r#"{"t":"transferstarted","id":14,"n":1,"moving":false}"#);
        kept.answered(r#"{"t":"archivedone","id":14,"ok":false,"verified":true,"err":"x"}"#);
        assert!(!kept.touch("/media/t"), "a refused disk image never starts");
        assert!(
            kept.touch("/media/s"),
            "an archive's end never ends a transfer"
        );
    }

    #[test]
    fn a_long_paste_never_pushes_out_a_transfer_that_still_runs() {
        let mut kept = Transfers::default();
        kept.sent(
            r#"{"c":"transfer","op":"copy","paths":["/media/s/dcim"],"dest":"/home/u"}"#,
            None,
        );
        let many: Vec<String> = (0..300).map(|n| format!("\"/home/a/{n}\"")).collect();
        let paste = format!(
            r#"{{"c":"transfer","op":"copy","paths":[{}],"dest":"/home/b"}}"#,
            many.join(",")
        );
        kept.sent(&paste, None);
        kept.answered(STARTED);
        kept.answered(r#"{"t":"transferstarted","id":13,"n":300,"moving":false}"#);
        kept.answered(r#"{"t":"transferdone","id":13,"ok":300}"#);
        kept.sent(ELSEWHERE, None);
        assert!(kept.touch("/media/s"));
    }
}
