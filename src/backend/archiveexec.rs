// Archive and conversion workers execute off the backend loop and return existing protocol lines.
use flea::bytename::ExactText as _;
use super::{archivedone_line, collision_message, convertdone_line};
use crate::backend::archive::Formats;
use crate::backend::archiveops::{self, compress, convert_one, split_paths};
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
    compressing: bool,
    paths: Vec<String>,
    format: String,
    archive: PathBuf,
    dest: PathBuf,
    formats: &Formats,
    tx: Sender<OpMsg>,
    selection: Option<Vec<Selected>>,
    cancel: Arc<AtomicBool>,
    password: String,
    encrypt_names: bool,
) {
    let line = archive_line(
        id,
        ArchiveRequest { compressing, paths, format, archive, dest, password, encrypt_names },
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
                compress(formats, &parent, &names, &format, &dest, &cancel, password).map(|()| true)
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
        Ok(verified) => archivedone_line(id, true, verified, ""),
        Err(e) if !compressing && archivepass::asks_again(&e.msg) => {
            archivepass::asks_again_line(id, &e.msg, &archive, &dest)
        }
        Err(e) => archivedone_line(id, false, true, &e.msg),
    }
}

pub(crate) fn run_convert(
    id: usize,
    request_id: usize,
    input: PathBuf,
    dest: PathBuf,
    strip: bool,
    tx: Sender<OpMsg>,
    selection: Option<Vec<Selected>>,
    cancel: Arc<AtomicBool>,
) {
    let result = validate_sources(selection.as_deref(), std::slice::from_ref(&input))
        .map_err(|error| op_err("convert", &input.to_string_lossy(), &error))
        .and_then(|()| convert_one(&input, &dest, strip, &cancel));
    let line = match result {
        Ok(()) => convertdone_line(
            id,
            request_id,
            &input.exact_text(),
            true,
            &dest.exact_text(),
            "",
            false,
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
