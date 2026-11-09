// The cursor row's two-byte shebang probe, answered off the event loop the way meta is: a UI read of a hung mount must never hold later reads.
use crate::backend::opsreq::OpMsg;
use crate::backend::regfile::open_regular;
use crate::json::escape;
use flea::bytename::ExactText as _;
use std::io::Read;
use std::path::{Path, PathBuf};

// Sample input: {"c":"shebang","path":"/home/gm/run.sh","id":3}
pub fn has_shebang(path: &Path) -> bool {
    let Some(mut f) = open_regular(path) else { return false };
    let mut head = [0u8; 2];
    f.read_exact(&mut head).map(|_| head == *b"#!").unwrap_or(false)
}

// Sample output: {"t":"shebang","path":"/home/gm/run.sh","hasShebang":true,"id":3}
pub fn shebang_line(path: &str, has: bool, id: usize) -> String {
    format!(r#"{{"t":"shebang","path":"{}","hasShebang":{},"id":{}}}"#, escape(path), has, id)
}

// Answered on a thread, because an open on a hung mount never returns and the loop waits on nothing.
pub fn spawn(path: PathBuf, id: usize, tx: std::sync::mpsc::Sender<OpMsg>) {
    std::thread::spawn(move || {
        let has = has_shebang(&path);
        let _ = tx.send(OpMsg::Meta {
            line: shebang_line(&path.exact_text(), has, id),
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::fifotest::{mkfifo, within, BOUND};

    #[test]
    fn only_a_regular_file_starting_with_hash_bang_answers_true() {
        let d = crate::backend::testdir::TestDir::new("shebangprobe");
        assert!(has_shebang(&d.file("run.sh", "#!/bin/sh\necho hi\n")));
        assert!(!has_shebang(&d.file("plain.sh", "echo hi\n")), "no marker, no row");
        assert!(!has_shebang(&d.file("short", "#")), "one byte is not two");
        assert!(!has_shebang(&d.file("empty", "")), "an empty file names nothing");
        assert!(!has_shebang(&d.join("never-existed")), "a vanished row answers false");
        assert!(!has_shebang(&d.dir("sub")), "a directory has no first bytes");
    }

    #[test]
    fn a_link_is_followed_the_way_the_open_is() {
        let d = crate::backend::testdir::TestDir::new("shebanglink");
        let target = d.file("run.sh", "#!/bin/sh\n");
        std::os::unix::fs::symlink(&target, d.join("tolink")).unwrap();
        assert!(has_shebang(&d.join("tolink")));
    }

    #[test]
    fn a_fifo_with_no_writer_answers_false_instead_of_hanging() {
        let d = crate::backend::testdir::TestDir::new("shebangfifo");
        let fifo = d.join("pipe");
        mkfifo(&fifo);
        assert!(!within("has_shebang", move || has_shebang(&fifo)), "a fifo is refused before the open rather than inside it");
    }

    #[test]
    fn the_answer_carries_the_path_and_the_caller_id_escaped() {
        assert_eq!(shebang_line("/home/gm/run.sh", true, 3),
            r#"{"t":"shebang","path":"/home/gm/run.sh","hasShebang":true,"id":3}"#);
        assert!(shebang_line("/tmp/say \"hi\".sh", false, 7).contains(r#""path":"/tmp/say \"hi\".sh""#));
        assert!(crate::jsondoc::parse(&shebang_line("/a.sh", false, 1)).is_ok());
    }

    #[test]
    fn a_spawned_probe_answers_through_the_meta_channel() {
        let d = crate::backend::testdir::TestDir::new("shebangspawn");
        let path = d.file("run.sh", "#!/bin/sh\n");
        let (tx, rx) = std::sync::mpsc::channel();
        spawn(path.to_path_buf(), 9, tx);
        match rx.recv_timeout(BOUND) {
            Ok(OpMsg::Meta { line }) => assert_eq!(line, shebang_line(&path.to_string_lossy(), true, 9)),
            _ => panic!("a shebang probe answers one line"),
        }
    }
}
