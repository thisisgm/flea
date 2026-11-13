// Archive and conversion workers execute off the backend loop and return existing protocol lines.
use flea::bytename::ExactText as _;
use super::{archivedone_line, collision_message, convertdone_line};
use crate::backend::archive::Formats;
use crate::backend::archiveops::{self, compress_leaving, convert_leaving, split_paths};
use crate::backend::archivepass;
use crate::backend::menu_actions::{validate_sources, Selected};
use crate::backend::opsreq::{op_err, OpMsg};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::Arc;

pub(crate) struct ArchiveRequest {
    pub compressing: bool,
    pub paths: Vec<String>,
    pub format: String,
    pub archive: PathBuf,
    pub dest: PathBuf,
    pub password: String,
    pub encrypt_names: bool,
}

#[cfg(test)]
pub(crate) fn run_archive(
    id: usize,
    request: ArchiveRequest,
    formats: &Formats,
    tx: Sender<OpMsg>,
    selection: Option<Vec<Selected>>,
    cancel: Arc<AtomicBool>,
) {
    let compressing = request.compressing;
    let line = archive_line(
        id,
        request,
        formats,
        selection.as_deref(),
        cancel,
        None,
    );
    // An extract frees the slot its terminal line took; a compress claims none and leaves the quit registry when its line is read.
    let message = if compressing {
        OpMsg::DetachedDone { id, line }
    } else {
        OpMsg::SlotDone { line }
    };
    let _ = tx.send(message);
}

pub(crate) fn archive_line(
    id: usize,
    request: ArchiveRequest,
    formats: &Formats,
    selection: Option<&[Selected]>,
    cancel: Arc<AtomicBool>,
    report: Option<&Sender<OpMsg>>,
) -> String {
    let ArchiveRequest { compressing, paths, format, archive, dest, password, encrypt_names } = request;
    let sources: Vec<PathBuf> = if compressing {
        paths.iter().map(flea::bytename::exact).collect()
    } else {
        vec![archive.clone()]
    };
    let result = if let Err(error) = validate_sources(selection, &sources) {
        Err(op_err("archive", "", &error))
    } else if compressing {
        // A compress writes the archive, so nothing verifies it and an empty password is no password.
        let password = (!password.is_empty()).then(|| crate::backend::archiveops::Password {
            secret: &password,
            encrypt_names,
        });
        match split_paths(&paths) {
            Some((parent, names)) => {
                compress_leaving(formats, &parent, &names, &format, &dest, &cancel, password)
                    .map(|left| (true, left))
            }
            None => Err(op_err(
                "archive",
                "",
                "the items to compress must be in one folder",
            )),
        }
    } else {
        // An empty password is no password: the encrypted probe below decides whether to ask.
        let password = (!password.is_empty()).then_some(password.as_str());
        archiveops::extract_reporting(
            formats,
            &archive,
            &dest,
            &cancel,
            password,
            report.map(|tx| (id, tx)),
        )
    };
    match result {
        Ok((verified, left)) => {
            let done = archivedone_line(id, true, verified, "");
            // An extract's success names its archive and asked folder, as its retry line does, so a client tells it from a compress.
            let done = if compressing {
                done
            } else {
                with_field(
                    with_field(done, "path", &archive.exact_text()),
                    "dest",
                    &dest.exact_text(),
                )
            };
            with_left(done, left.as_deref())
        }
        Err(e) if !compressing && archivepass::asks_again(&e.msg) => {
            archivepass::asks_again_line(id, &e.msg, &archive, &dest)
        }
        Err(e) => archivedone_line(id, false, true, &e.msg),
    }
}

// Issue 370: the result landed whole, so a work folder the drive kept is named beside the success, never as a failure.
// Sample output: {"t":"archivedone","id":4,"ok":true,"verified":true,"err":"","left":"/mnt/gdrive/.flea-work-ext-81-0"}
fn with_left(line: String, left: Option<&str>) -> String {
    match left {
        None => line,
        Some(path) => with_field(line, "left", path),
    }
}

fn with_field(line: String, key: &str, value: &str) -> String {
    format!(
        r#"{},"{}":"{}"}}"#,
        &line[..line.len() - 1],
        key,
        crate::json::escape(value)
    )
}

pub(crate) fn run_convert(
    id: usize,
    request: super::ConvertRequest,
    tx: Sender<OpMsg>,
    selection: Option<Vec<Selected>>,
    cancel: Arc<AtomicBool>,
) {
    let super::ConvertRequest { request_id, input, dest, strip } = request;
    let result = validate_sources(selection.as_deref(), std::slice::from_ref(&input))
        .map_err(|error| op_err("convert", &input.to_string_lossy(), &error))
        .and_then(|()| convert_leaving(&input, &dest, strip, &cancel));
    let line = match result {
        Ok(left) => with_left(
            convertdone_line(
                id,
                request_id,
                &input.exact_text(),
                true,
                &dest.exact_text(),
                "",
                false,
            ),
            left.as_deref(),
        ),
        Err(e) => {
            let collision = dest.symlink_metadata().is_ok();
            let message = if collision {
                collision_message(&dest)
            } else {
                e.msg
            };
            convertdone_line(
                id,
                request_id,
                &input.exact_text(),
                false,
                &dest.exact_text(),
                &message,
                collision,
            )
        }
    };
    let _ = tx.send(OpMsg::DetachedDone { id, line });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_folder_rides_on_the_success_line_and_a_clean_one_adds_nothing() {
        let done = archivedone_line(4, true, true, "");
        assert_eq!(with_left(done.clone(), None), done);
        assert_eq!(
            with_left(done, Some("/mnt/g/.flea-work-ext-8-0")),
            r#"{"t":"archivedone","id":4,"ok":true,"verified":true,"err":"","left":"/mnt/g/.flea-work-ext-8-0"}"#
        );
        let converted = convertdone_line(5, 9, "/p/a.heic", true, "/p/a.jpg", "", false);
        assert_eq!(
            with_left(converted, Some("/p/.flea-work-cvt-8-0")),
            r#"{"t":"convertdone","id":5,"requestId":9,"source":"/p/a.heic","ok":true,"path":"/p/a.jpg","err":"","collision":false,"left":"/p/.flea-work-cvt-8-0"}"#
        );
    }

    // Soak S1B-6: a finished extract named nothing, so the window said a compress's receipt for it.
    #[test]
    fn an_extract_success_names_its_archive_and_folder_and_a_compress_success_does_not() {
        let formats = Formats::from_tools(true, false);
        if crate::backend::sandboxprobe::skipped() || !formats.offers("tar") {
            return;
        }
        let d = crate::backend::testdir::TestDir::new("archexecdone");
        d.file("a.txt", "alpha");
        let run = |compressing: bool, paths: Vec<String>, archive: PathBuf, dest: PathBuf| {
            let request = ArchiveRequest {
                compressing,
                paths,
                format: "tar".into(),
                archive,
                dest,
                password: String::new(),
                encrypt_names: false,
            };
            archive_line(
                3,
                request,
                &formats,
                None,
                Arc::new(AtomicBool::new(false)),
                None,
            )
        };
        let made = run(
            true,
            vec![d.join("a.txt").to_string_lossy().to_string()],
            PathBuf::new(),
            d.join("a.tar"),
        );
        assert_eq!(
            made,
            r#"{"t":"archivedone","id":3,"ok":true,"verified":true,"err":""}"#
        );
        // The asked folder is taken, so the files land in "a copy" and the line still names what was asked.
        d.dir("a");
        let done = run(false, Vec::new(), d.join("a.tar"), d.join("a"));
        assert_eq!(
            done,
            format!(
                r#"{{"t":"archivedone","id":3,"ok":true,"verified":true,"err":"","path":"{}","dest":"{}"}}"#,
                d.join("a.tar").display(),
                d.join("a").display()
            )
        );
        assert_eq!(
            std::fs::read_to_string(d.join("a copy/a.txt")).unwrap(),
            "alpha"
        );
    }
}
