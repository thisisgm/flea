// Multi-item Properties describes held identities and counts folders only under the menu's cancellation.
use flea::bytename::ExactText as _;
use super::menu_actions::{response, Selected, SELECTION_CHANGED};
use super::menucontents;
use super::opsreq::OpMsg;
use super::trashmanifest::Cancellation;
use crate::json::{escape, field_usize};
use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;
use std::sync::mpsc::Sender;

#[path = "menuproperties_remote.rs"]
mod remote;

#[derive(Clone, Copy)]
enum Kind { Folder, Link, Text, Image, Video, Audio, File, Special }

const KIND_NAMES: [(Kind, &str, &str); 8] = [
    (Kind::Folder, "folder", "folders"),
    (Kind::Link, "symbolic link", "symbolic links"),
    (Kind::Text, "text", "texts"),
    (Kind::Image, "image", "images"),
    (Kind::Video, "video", "videos"),
    (Kind::Audio, "audio file", "audio files"),
    (Kind::File, "file", "files"),
    (Kind::Special, "special file", "special files"),
];

pub(super) fn single(item: &Selected, meta: &Metadata) -> Result<String, String> {
    let kind = if meta.file_type().is_symlink() {
        "Symbolic link"
    } else if meta.is_dir() {
        "Directory"
    } else if meta.is_file() {
        "File"
    } else {
        "Special file"
    };
    let target = if meta.file_type().is_symlink() {
        std::fs::read_link(&item.path)
            .map_err(|e| format!("Could not read the link: {}.", crate::error::io_message(&e)))?
            .to_string_lossy()
            .to_string()
    } else {
        String::new()
    };
    Ok(format!(
        r#""path":"{}","kind":"{}","directory":{},"symlink":{},"target":"{}","bytes":{},"modified":{},"birth":{},"mode":"{:04o}","owner":"{}","uid":{},"gid":{},"tags":{},"photo":{}"#,
        escape(&item.path.exact_text()),
        kind,
        meta.is_dir(),
        meta.file_type().is_symlink(),
        escape(&target),
        meta.len(),
        meta.mtime(),
        super::birthtime::json(&item.path),
        meta.mode() & 0o7777,
        escape(&super::owner::name(meta.uid())),
        meta.uid(),
        meta.gid(),
        super::tags::names_json(&item.path),
        super::metareq::photo_json(&super::metareq::photo(&item.path, true))
    ))
}

#[derive(Clone, Default)]
struct Summary {
    kinds: String,
    bytes: u64,
    newest: String,
    oldest: String,
    partial: bool,
}

impl Summary {
    fn inspect<'a>(items: &'a [Selected], cancel: &Cancellation, work: Option<&remote::State>) -> Result<(Self, Vec<&'a Selected>), String> {
        let mime = super::mime::Db::load();
        let mut counts = [0usize; KIND_NAMES.len()];
        let (mut bytes, mut newest, mut oldest) = (0u64, i64::MIN, i64::MAX);
        let mut folders = Vec::new();
        for item in items {
            cancel.check()?;
            if let Some(work) = work { work.before(&item.path); }
            let meta = item.current()?;
            let group = kind(&meta, mime.lookup(&item.path.to_string_lossy()).unwrap_or(""));
            counts[group as usize] += 1;
            newest = newest.max(meta.mtime());
            oldest = oldest.min(meta.mtime());
            if meta.is_dir() {
                folders.push(item);
            } else {
                bytes = add_bytes(bytes, meta.len())?;
                if let Some(work) = work { work.seed(bytes); }
            }
        }
        let kinds = KIND_NAMES.iter().filter_map(|(group, single, plural)| {
            let n = counts[*group as usize];
            (n > 0).then(|| format!("{} {}", grouped(n), if n == 1 { single } else { plural }))
        }).collect::<Vec<_>>().join(", ");
        Ok((Self { kinds, bytes, newest: super::trashdate::modified_local(newest)?, oldest: super::trashdate::modified_local(oldest)?, partial: false }, folders))
    }

    fn fields(&self, done: bool, cancelled: bool) -> String {
        format!(r#""kinds":"{}","bytes":{},"newest":"{}","oldest":"{}","partial":{},"done":{}{}"#,
            escape(&self.kinds), self.bytes, escape(&self.newest), escape(&self.oldest), self.partial || cancelled,
            done && !cancelled, if cancelled { r#", "cancelled":true"# } else { "" })
    }
}

fn add_bytes(left: u64, right: u64) -> Result<u64, String> {
    left.checked_add(right).ok_or_else(|| "The selected items are too large to total.".into())
}

fn kind(meta: &Metadata, mime: &str) -> Kind {
    if meta.is_dir() { Kind::Folder }
    else if meta.file_type().is_symlink() { Kind::Link }
    else if !meta.is_file() { Kind::Special }
    else if mime.starts_with("text/") { Kind::Text }
    else if mime.starts_with("image/") { Kind::Image }
    else if mime.starts_with("video/") { Kind::Video }
    else if mime.starts_with("audio/") { Kind::Audio }
    else { Kind::File }
}

fn grouped(count: usize) -> String {
    let digits = count.to_string();
    let mut result = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 { result.push(','); }
        result.push(digit);
    }
    result
}

// Sample input: {"c":"menuaction","op":"properties","id":7} streams an aggregate for the snapshot, never fresh row indices.
pub(super) fn serve(line: &str, id: usize, items: &[Selected], replies: &Sender<OpMsg>, cancel: &Cancellation) -> bool {
    if id == 0 || field_usize(line, "id") != Some(id) || items.len() < 2 {
        return replies.send(OpMsg::Meta { line: response(line, Err(SELECTION_CHANGED.into())) }).is_ok();
    }
    let body = super::iomount::mount_body();
    if items.iter().any(|item| !super::walkmounts::remote_mounts_under(&item.path, &body).is_empty()) {
        remote::serve(line, items, replies, cancel, &body)
    } else { serve_local(line, items, replies, cancel, None) }
}

fn serve_local(line: &str, items: &[Selected], replies: &Sender<OpMsg>, cancel: &Cancellation, work: Option<&remote::State>) -> bool {
    let send = |fields| replies.send(OpMsg::Meta { line: response(line, fields) }).is_ok();
    if cancel.check().is_err() { return send(Ok(r#""cancelled":true,"partial":true,"done":false"#.into())); }
    let (mut summary, folders) = match Summary::inspect(items, cancel, work) {
        Ok(summary) => summary,
        Err(_) if cancel.check().is_err() => return send(Ok(r#""cancelled":true,"partial":true,"done":false"#.into())),
        Err(error) => return send(Err(error)),
    };
    if let Some(work) = work { work.metadata(&summary); }
    if !folders.is_empty() && !send(Ok(summary.fields(false, false))) { return false; }
    for folder in folders {
        if cancel.check().is_err() { return send(Ok(summary.fields(false, true))); }
        let (base, partial) = (summary.bytes, summary.partial);
        let mut failure = None;
        let mut cancelled = false;
        let mut due = std::time::Instant::now() + super::opsreq::PROGRESS_EVERY;
        let mut report = |result: Result<menucontents::Counted, String>| {
            match result {
                Ok(counted) => {
                    summary.bytes = match add_bytes(base, counted.tally.bytes) {
                        Ok(bytes) => bytes,
                        Err(error) => { failure = Some(error); return false; }
                    };
                    summary.partial = partial || counted.tally.partial;
                    cancelled = counted.cancelled;
                    if let Some(work) = work { work.progress(&summary); }
                    if counted.done || cancelled { true }
                    else if work.is_none() || std::time::Instant::now() >= due {
                        due = std::time::Instant::now() + super::opsreq::PROGRESS_EVERY;
                        send(Ok(summary.fields(false, false)))
                    } else { true }
                }
                Err(error) => { failure = Some(error); true }
            }
        };
        let open = match work {
            Some(work) => remote::count(folder, cancel, work, &mut report),
            None => menucontents::count(folder, cancel, super::iomount::BULK_DEADLINE, &mut report),
        };
        if let Some(error) = failure { return send(Err(error)); }
        if !open { return false; }
        if cancelled || cancel.check().is_err() { return send(Ok(summary.fields(false, true))); }
    }
    // A replacement or disappearance while a folder was walked cannot publish an exact total for the old selection.
    for item in items {
        if cancel.check().is_err() { return send(Ok(summary.fields(false, true))); }
        if let Some(work) = work { work.before(&item.path); }
        if let Err(error) = item.current() { return send(Err(error)); }
    }
    send(Ok(summary.fields(true, cancel.check().is_err())))
}
