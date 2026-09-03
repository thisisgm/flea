// Turning an archive or convert REQUEST into wire lines and a background job: the response builders,
// the two run_* bodies a thread executes, and the two start_* entry points the dispatcher calls.
// What those jobs do is archiveops.rs.
use crate::backend::archive::Formats;
use crate::backend::archiveops::{compress, convert_one, extract, split_paths};
use crate::backend::convert;
use crate::backend::opsreq::{op_err, OpMsg};
use crate::json::escape;
use std::path::PathBuf;
use crate::backend::opsdispatch::Ops;
use crate::backend::proto::error_line;
use std::io::Write;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;


pub fn archivestarted_line(id: usize) -> String {
    format!(r#"{{"t":"archivestarted","id":{}}}"#, id)
}

// verified is false only when an extract could not read the archive's own index, so it could not
// confirm the tool produced what the index named. The operator is told which kind of success it was.
pub fn archivedone_line(id: usize, ok: bool, verified: bool, err: &str) -> String {
    format!(r#"{{"t":"archivedone","id":{},"ok":{},"verified":{},"err":"{}"}}"#, id, ok, verified, escape(err))
}

pub fn convertstarted_line(id: usize) -> String {
    format!(r#"{{"t":"convertstarted","id":{}}}"#, id)
}

pub fn convertdone_line(id: usize, ok: bool, path: &str, err: &str) -> String {
    format!(
        r#"{{"t":"convertdone","id":{},"ok":{},"path":"{}","err":"{}"}}"#,
        id, ok, escape(path), escape(err)
    )
}

// Sample output: {"t":"formats","archive":["zip","tar","tar.zst"],"convert":true}
pub fn formats_line(formats: &Formats, can_convert: bool) -> String {
    let names: Vec<String> = formats.names().iter().map(|n| format!(r#""{}""#, escape(n))).collect();
    format!(r#"{{"t":"formats","archive":[{}],"convert":{}}}"#, names.join(","), can_convert)
}

pub fn run_archive(id: usize, compressing: bool, paths: Vec<String>, format: String,
                   archive: PathBuf, dest: PathBuf, formats: &Formats, tx: Sender<OpMsg>) {
    let result = if compressing {
        // A compress has nothing to verify against: it writes the archive rather than reading one.
        match split_paths(&paths) {
            Some((parent, names)) => compress(formats, &parent, &names, &format, &dest).map(|()| true),
            None => Err(op_err("archive", "", "a compress takes absolute paths from one directory")),
        }
    } else {
        extract(formats, &archive, &dest)
    };
    let line = match result {
        Ok(verified) => archivedone_line(id, true, verified, ""),
        Err(e) => archivedone_line(id, false, true, &e.msg),
    };
    let _ = tx.send(OpMsg::Meta { line });
}

pub fn run_convert(id: usize, input: PathBuf, dest: PathBuf, strip: bool, tx: Sender<OpMsg>) {
    let line = match convert_one(&input, &dest, strip) {
        Ok(()) => convertdone_line(id, true, &dest.to_string_lossy(), ""),
        Err(e) => convertdone_line(id, false, "", &e.msg),
    };
    let _ = tx.send(OpMsg::Meta { line });
}

// The dispatch half, kept beside the work so run.rs's own match stays one line per request.
pub fn start_archive(
    out: &mut impl Write,
    ops: &mut Ops,
    formats: Arc<Formats>,
    op: &str,
    paths: Vec<String>,
    format: String,
    archive: PathBuf,
    dest: PathBuf,
) {
    // An op that names neither would otherwise fall through to extract, so it is refused by name.
    if op != "compress" && op != "extract" {
        writeln!(out, "{}", error_line(&op_err("archive", op, "op must be compress or extract"))).ok();
        out.flush().ok();
        return;
    }
    let compressing = op == "compress";
    let id = ops.claim_id();
    writeln!(out, "{}", archivestarted_line(id)).ok();
    out.flush().ok();
    let tx = ops.tx.clone();
    thread::spawn(move || {
        run_archive(id, compressing, paths, format, archive, dest, &formats, tx)
    });
}

pub fn start_convert(out: &mut impl Write, ops: &mut Ops, input: PathBuf, dest: PathBuf, strip: bool) {
    if !convert::available() {
        let e = op_err("convert", "", "ImageMagick is not installed on this box");
        writeln!(out, "{}", error_line(&e)).ok();
        out.flush().ok();
        return;
    }
    let id = ops.claim_id();
    writeln!(out, "{}", convertstarted_line(id)).ok();
    out.flush().ok();
    let tx = ops.tx.clone();
    thread::spawn(move || run_convert(id, input, dest, strip, tx));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn every_line_matches_the_shape_the_operations_design_names() {
        assert_eq!(archivestarted_line(13), r#"{"t":"archivestarted","id":13}"#);
        assert_eq!(archivedone_line(13, true, true, ""),
                   r#"{"t":"archivedone","id":13,"ok":true,"verified":true,"err":""}"#);
        assert_eq!(archivedone_line(13, true, false, ""),
                   r#"{"t":"archivedone","id":13,"ok":true,"verified":false,"err":""}"#);
        assert_eq!(convertstarted_line(15), r#"{"t":"convertstarted","id":15}"#);
        assert_eq!(
            convertdone_line(15, true, "/home/gm/photo.jpg", ""),
            r#"{"t":"convertdone","id":15,"ok":true,"path":"/home/gm/photo.jpg","err":""}"#
        );
        let f = Formats::from_tools(true, false);
        assert_eq!(
            formats_line(&f, true),
            r#"{"t":"formats","archive":["zip","tar","tar.gz","tar.bz2","tar.xz","tar.zst"],"convert":true}"#
        );
        assert_eq!(
            formats_line(&Formats::from_tools(false, false), false),
            r#"{"t":"formats","archive":[],"convert":false}"#
        );
    }









    #[test]
    fn a_destination_already_there_is_refused_before_any_tool_runs() {
        let d = TestDir::new("archrefuse");
        let f = Formats::from_tools(true, true);
        d.file("out.zip", "already here");
        let e = compress(&f, d.path(), &["a.txt".to_string()], "zip", &d.join("out.zip")).unwrap_err();
        assert!(e.msg.contains("already exists"));
        assert_eq!(std::fs::read_to_string(d.join("out.zip")).unwrap(), "already here");

        d.dir("out");
        let e = extract(&f, &d.join("x.zip"), &d.join("out")).unwrap_err();
        assert!(e.msg.contains("already exists"), "merging into a directory in use is the surprise this rules out");
    }

    #[test]
    fn a_format_no_tool_offers_is_a_named_error_rather_than_a_silent_failure() {
        let d = TestDir::new("archnotool");
        let none = Formats::from_tools(false, false);
        let e = compress(&none, d.path(), &["a.txt".to_string()], "zip", &d.join("out.zip")).unwrap_err();
        assert!(e.msg.contains("no tool"), "got {}", e.msg);
    }

    #[test]
    fn a_convert_never_writes_over_the_file_it_was_given() {
        let d = TestDir::new("cvtrefuse");
        let src = d.file("shot.png", "pixels");
        let e = convert_one(&src, &src, false).unwrap_err();
        assert!(e.msg.contains("already exists"));
        assert_eq!(std::fs::read_to_string(&src).unwrap(), "pixels");
    }
}
