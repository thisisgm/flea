// Turning an archive or convert REQUEST into wire lines and a background job: the response builders,
// the two run_* bodies a thread executes, and the two start_* entry points the dispatcher calls.
// What those jobs do is archiveops.rs.
use crate::backend::archive::Formats;
use crate::backend::archiveops::{compress, convert_one, extract, split_paths};
use crate::backend::convert;
use crate::backend::opsreq::{op_err, OpMsg};
use crate::error::io_message;
use crate::json::escape;
use std::path::PathBuf;
use crate::backend::opsdispatch::Ops;
use crate::backend::proto::error_line;
use std::io::Write;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use super::menu_actions::{validate_sources, Selected};
use super::opsdispatch::menu_sources;


pub fn archivestarted_line(id: usize) -> String {
    format!(r#"{{"t":"archivestarted","id":{}}}"#, id)
}

// verified is false only when an extract could not read the archive's own index, so it could not
// confirm the tool produced what the index named. The operator is told which kind of success it was.
pub fn archivedone_line(id: usize, ok: bool, verified: bool, err: &str) -> String {
    format!(r#"{{"t":"archivedone","id":{},"ok":{},"verified":{},"err":"{}"}}"#, id, ok, verified, escape(err))
}

pub fn convertstarted_line(id: usize, request_id: usize, source: &str) -> String {
    format!(r#"{{"t":"convertstarted","id":{},"requestId":{},"source":"{}"}}"#, id, request_id, escape(source))
}

pub fn convertdone_line(id: usize, request_id: usize, source: &str, ok: bool, path: &str, err: &str, collision: bool) -> String {
    format!(
        r#"{{"t":"convertdone","id":{},"requestId":{},"source":"{}","ok":{},"path":"{}","err":"{}","collision":{}}}"#,
        id, request_id, escape(source), ok, escape(path), escape(err), collision
    )
}

fn check_convert(input: &std::path::Path, dest: &std::path::Path, selection: Option<&[Selected]>) -> Result<bool, String> {
    if !input.is_absolute() || !dest.is_absolute() || dest.file_name().is_none() {
        return Err("Conversion requires absolute source and output paths.".into());
    }
    validate_sources(selection, std::slice::from_ref(&input.to_path_buf()))?;
    input.symlink_metadata().map_err(|error| format!("Could not inspect {}: {}.", input.display(), io_message(&error)))?;
    match dest.symlink_metadata() {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("Could not inspect output {}: {}.", dest.display(), io_message(&error))),
    }
}

fn collision_message(dest: &std::path::Path) -> String {
    format!("{} already exists", dest.file_name().unwrap_or_default().to_string_lossy())
}

// Sample output: {"t":"formats","archive":["zip","tar","tar.zst"],"convert":true}
pub fn formats_line(formats: &Formats, can_convert: bool) -> String {
    let names: Vec<String> = formats.names().iter().map(|n| format!(r#""{}""#, escape(n))).collect();
    format!(r#"{{"t":"formats","archive":[{}],"convert":{},"extract":{{"archive":{},"sevenZip":{}}}}}"#,
        names.join(","), can_convert, formats.offers("tar"), formats.offers("7z"))
}

pub(crate) fn run_archive(id: usize, compressing: bool, paths: Vec<String>, format: String,
                   archive: PathBuf, dest: PathBuf, formats: &Formats, tx: Sender<OpMsg>, selection: Option<Vec<Selected>>) {
    let sources: Vec<PathBuf> = if compressing { paths.iter().map(PathBuf::from).collect() } else { vec![archive.clone()] };
    let result = if let Err(error) = validate_sources(selection.as_deref(), &sources) {
        Err(op_err("archive", "", &error))
    } else if compressing {
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

pub(crate) fn run_convert(id: usize, request_id: usize, input: PathBuf, dest: PathBuf, strip: bool, tx: Sender<OpMsg>, selection: Option<Vec<Selected>>) {
    let result = validate_sources(selection.as_deref(), std::slice::from_ref(&input))
        .map_err(|error| op_err("convert", &input.to_string_lossy(), &error))
        .and_then(|()| convert_one(&input, &dest, strip));
    let line = match result {
        Ok(()) => convertdone_line(id, request_id, &input.to_string_lossy(), true, &dest.to_string_lossy(), "", false),
        Err(e) => {
            let collision = dest.symlink_metadata().is_ok();
            let message = if collision { collision_message(&dest) } else { e.msg };
            convertdone_line(id, request_id, &input.to_string_lossy(), false, &dest.to_string_lossy(), &message, collision)
        }
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
    menu_id: usize,
) {
    // An op that names neither would otherwise fall through to extract, so it is refused by name.
    if op != "compress" && op != "extract" {
        writeln!(out, "{}", error_line(&op_err("archive", op, "op must be compress or extract"))).ok();
        out.flush().ok();
        return;
    }
    let selection = match menu_sources(ops, menu_id) {
        Ok(selection) => selection,
        Err(message) => { writeln!(out, "{}", error_line(&op_err("archive", "", &message))).ok(); out.flush().ok(); return; }
    };
    let compressing = op == "compress";
    let id = ops.claim_id();
    writeln!(out, "{}", archivestarted_line(id)).ok();
    out.flush().ok();
    let tx = ops.tx.clone();
    thread::spawn(move || {
        run_archive(id, compressing, paths, format, archive, dest, &formats, tx, selection)
    });
}

pub fn start_convert(out: &mut impl Write, ops: &mut Ops, input: PathBuf, dest: PathBuf, strip: bool,
                     menu_id: usize, request_id: usize, check: bool) {
    let selection = menu_sources(ops, menu_id);
    let result = match &selection {
        Ok(items) => check_convert(&input, &dest, items.as_deref()),
        Err(error) => Err(error.clone()),
    }.and_then(|collision| if collision || convert::available() { Ok(collision) }
        else { Err("ImageMagick is not installed on this box.".into()) });
    let collision = matches!(result, Ok(true));
    let error = if collision { collision_message(&dest) } else { result.as_ref().err().cloned().unwrap_or_default() };
    if check {
        writeln!(out, r#"{{"t":"convertchecked","requestId":{},"source":"{}","path":"{}","ok":{},"collision":{},"error":"{}"}}"#,
            request_id, escape(&input.to_string_lossy()), escape(&dest.to_string_lossy()), result.is_ok(), collision, escape(&error)).ok();
        out.flush().ok();
        return;
    }
    let id = ops.claim_id();
    if result.is_err() || collision {
        writeln!(out, "{}", convertdone_line(id, request_id, &input.to_string_lossy(), false, &dest.to_string_lossy(), &error, collision)).ok();
        out.flush().ok();
        return;
    }
    writeln!(out, "{}", convertstarted_line(id, request_id, &input.to_string_lossy())).ok();
    out.flush().ok();
    let tx = ops.tx.clone();
    thread::spawn(move || run_convert(id, request_id, input, dest, strip, tx, selection.unwrap()));
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
        assert_eq!(convertstarted_line(15, 7, "/home/gm/photo.png"),
            r#"{"t":"convertstarted","id":15,"requestId":7,"source":"/home/gm/photo.png"}"#);
        assert_eq!(
            convertdone_line(15, 7, "/home/gm/photo.png", true, "/home/gm/photo.jpg", "", false),
            r#"{"t":"convertdone","id":15,"requestId":7,"source":"/home/gm/photo.png","ok":true,"path":"/home/gm/photo.jpg","err":"","collision":false}"#
        );
        let f = Formats::from_tools(true, false);
        assert_eq!(
            formats_line(&f, true),
            r#"{"t":"formats","archive":["zip","tar","tar.gz","tar.bz2","tar.xz","tar.zst"],"convert":true,"extract":{"archive":true,"sevenZip":false}}"#
        );
        assert_eq!(
            formats_line(&Formats::from_tools(false, false), false),
            r#"{"t":"formats","archive":[],"convert":false,"extract":{"archive":false,"sevenZip":false}}"#
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

    #[test]
    fn conversion_inspection_reports_plain_causes_before_running_a_tool() {
        let d = TestDir::new("convert-inspection-errors");
        let missing = d.join("missing.png");
        let output = d.join("output.jpg");
        assert_eq!(check_convert(&missing, &output, None).unwrap_err(),
            format!("Could not inspect {}: file or folder not found.", missing.display()));
        let source = d.file("photo.png", "source");
        let blocked = d.file("not-a-folder", "keep");
        let invalid_output = blocked.join("output.jpg");
        assert_eq!(check_convert(&source, &invalid_output, None).unwrap_err(),
            format!("Could not inspect output {}: a path component is not a folder.", invalid_output.display()));
        assert!(!output.exists());
        assert_eq!(std::fs::read_to_string(source).unwrap(), "source");
        assert_eq!(std::fs::read_to_string(blocked).unwrap(), "keep");
    }

    #[test]
    fn conversion_activation_rechecks_a_collision_after_an_absent_output_probe() {
        let d = TestDir::new("convert-probe-race");
        let source = d.file("photo.png", "source");
        let destination = d.join("photo (converted).jpg");
        let selected = vec![Selected::inspect(source.to_str().unwrap()).unwrap()];
        assert_eq!(check_convert(&source, &destination, Some(&selected)).unwrap(), false);
        d.file("photo (converted).jpg", "collision");
        let (tx, _) = std::sync::mpsc::channel();
        let mut ops = Ops::new(tx);
        let mut output = Vec::new();
        start_convert(&mut output, &mut ops, source.clone(), destination.clone(), false, 0, 73, false);
        let reply = String::from_utf8(output).unwrap();
        assert_eq!(crate::json::field_str(&reply, "t").as_deref(), Some("convertdone"));
        assert_eq!(crate::json::field_usize(&reply, "requestId"), Some(73));
        assert_eq!(crate::json::field_str(&reply, "source").as_deref(), source.to_str());
        assert_eq!(crate::json::field_str(&reply, "path").as_deref(), destination.to_str());
        assert!(crate::json::field_bool(&reply, "collision"));
        assert!(!crate::json::field_bool(&reply, "ok"));
        assert_eq!(std::fs::read_to_string(&destination).unwrap(), "collision");
        assert_eq!(std::fs::read_to_string(&source).unwrap(), "source");
        assert!(check_convert(std::path::Path::new("relative.png"), &destination, None).is_err());
        assert!(check_convert(&source, std::path::Path::new("relative.jpg"), None).is_err());
        assert!(source.is_absolute() && source.starts_with(d.path()) && d.path().join(".flea-test-sandbox").is_file());
        std::fs::rename(&source, d.join("original.png")).unwrap();
        d.file("photo.png", "replacement");
        assert!(check_convert(&source, &destination, Some(&selected)).unwrap_err().contains("changed"));
    }

    #[test]
    fn an_expired_conversion_menu_replies_to_both_original_request_identities() {
        let d = TestDir::new("convert-expired-menu");
        let source = d.file("photo.png", "source");
        let destination = d.join("photo.jpg");
        let (tx, _) = std::sync::mpsc::channel();
        let mut ops = Ops::new(tx);
        for check in [true, false] {
            let mut output = Vec::new();
            start_convert(&mut output, &mut ops, source.clone(), destination.clone(), true, 9, 74, check);
            let reply = String::from_utf8(output).unwrap();
            assert_eq!(crate::json::field_usize(&reply, "requestId"), Some(74));
            assert_eq!(crate::json::field_str(&reply, "source").as_deref(), source.to_str());
            assert_eq!(crate::json::field_str(&reply, "path").as_deref(), destination.to_str());
            assert!(!crate::json::field_bool(&reply, "ok"));
            assert!(reply.contains("Menu selection expired"));
            assert!(!destination.exists());
        }
    }

    #[test]
    fn missing_converter_keeps_check_and_activation_request_identity() {
        if std::env::var_os("FLEA_CONVERT_MISSING_CHILD").is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "backend::archivereq::tests::missing_converter_keeps_check_and_activation_request_identity"])
                .env("PATH", "").env("FLEA_CONVERT_MISSING_CHILD", "1").output().unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"), "the isolated missing-helper check did not run");
            return;
        }
        let d = TestDir::new("convert-missing-helper");
        let source = d.file("photo.png", "source");
        let destination = d.join("photo.jpg");
        let (tx, _) = std::sync::mpsc::channel();
        let mut ops = Ops::new(tx);
        for check in [true, false] {
            let mut output = Vec::new();
            start_convert(&mut output, &mut ops, source.clone(), destination.clone(), false, 0, 75, check);
            let reply = String::from_utf8(output).unwrap();
            assert_eq!(crate::json::field_usize(&reply, "requestId"), Some(75));
            assert_eq!(crate::json::field_str(&reply, "source").as_deref(), source.to_str());
            assert_eq!(crate::json::field_str(&reply, "path").as_deref(), destination.to_str());
            assert!(!crate::json::field_bool(&reply, "ok"));
            assert!(reply.contains("ImageMagick is not installed"));
            assert!(!destination.exists());
        }
    }
}
