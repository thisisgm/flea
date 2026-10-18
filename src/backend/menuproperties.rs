// Multi-item Properties describes held identities and counts folders only under the menu's cancellation.
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

const KIND_NAMES: [(&str, &str); 8] = [
    ("folder", "folders"),
    ("symbolic link", "symbolic links"),
    ("text", "texts"),
    ("image", "images"),
    ("video", "videos"),
    ("audio file", "audio files"),
    ("file", "files"),
    ("special file", "special files"),
];

pub(super) fn single(item: &Selected, meta: &Metadata) -> Result<String, String> {
    let kind = if meta.file_type().is_symlink() { "Symbolic link" } else if meta.is_dir() { "Directory" }
               else if meta.is_file() { "File" } else { "Special file" };
    let target = if meta.file_type().is_symlink() {
        std::fs::read_link(&item.path).map_err(|e| format!("Could not read the link: {}.", crate::error::io_message(&e)))?.to_string_lossy().to_string()
    } else { String::new() };
    Ok(format!(r#""path":"{}","kind":"{}","directory":{},"symlink":{},"target":"{}","bytes":{},"modified":{},"mode":"{:04o}","owner":"{}","uid":{},"gid":{},"tags":{},"photo":{}"#,
        escape(&item.path.to_string_lossy()), kind, meta.is_dir(), meta.file_type().is_symlink(), escape(&target),
        meta.len(), meta.mtime(), meta.mode() & 0o7777, escape(&super::owner::name(meta.uid())), meta.uid(), meta.gid(), super::tags::names_json(&item.path),
        super::metareq::photo_json(&super::metareq::photo(&item.path, true))))
}

struct Summary {
    kinds: String,
    bytes: u64,
    newest: String,
    oldest: String,
    partial: bool,
}

impl Summary {
    fn inspect<'a>(items: &'a [Selected], cancel: &Cancellation) -> Result<(Self, Vec<&'a Selected>), String> {
        let mime = super::mime::Db::load();
        let mut counts = [0usize; KIND_NAMES.len()];
        let (mut bytes, mut newest, mut oldest) = (0u64, i64::MIN, i64::MAX);
        let mut folders = Vec::new();
        for item in items {
            cancel.check()?;
            let meta = item.current()?;
            let group = kind(&meta, mime.lookup(&item.path.to_string_lossy()).unwrap_or(""));
            counts[group] += 1;
            newest = newest.max(meta.mtime());
            oldest = oldest.min(meta.mtime());
            if meta.is_dir() {
                folders.push(item);
            } else {
                bytes = add_bytes(bytes, meta.len())?;
            }
        }
        let kinds = counts.iter().zip(KIND_NAMES).filter(|(n, _)| **n > 0).map(|(n, (single, plural))| {
            format!("{} {}", grouped(*n), if *n == 1 { single } else { plural })
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

fn kind(meta: &Metadata, mime: &str) -> usize {
    if meta.is_dir() { 0 }
    else if meta.file_type().is_symlink() { 1 }
    else if !meta.is_file() { 7 }
    else if mime.starts_with("text/") { 2 }
    else if mime.starts_with("image/") { 3 }
    else if mime.starts_with("video/") { 4 }
    else if mime.starts_with("audio/") { 5 }
    else { 6 }
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
        remote::serve(line, items, replies, cancel)
    } else { serve_local(line, items, replies, cancel) }
}

fn serve_local(line: &str, items: &[Selected], replies: &Sender<OpMsg>, cancel: &Cancellation) -> bool {
    let send = |fields| replies.send(OpMsg::Meta { line: response(line, fields) }).is_ok();
    if cancel.check().is_err() { return send(Ok(r#""cancelled":true,"partial":true,"done":false"#.into())); }
    let (mut summary, folders) = match Summary::inspect(items, cancel) {
        Ok(summary) => summary,
        Err(_) if cancel.check().is_err() => return send(Ok(r#""cancelled":true,"partial":true,"done":false"#.into())),
        Err(error) => return send(Err(error)),
    };
    if !folders.is_empty() && !send(Ok(summary.fields(false, false))) { return false; }
    for folder in folders {
        if cancel.check().is_err() { return send(Ok(summary.fields(false, true))); }
        let (base, partial) = (summary.bytes, summary.partial);
        let mut failure = None;
        let mut cancelled = false;
        let open = menucontents::count(folder, cancel, super::iomount::BULK_DEADLINE, &mut |result| {
            match result {
                Ok(counted) => {
                    summary.bytes = match add_bytes(base, counted.tally.bytes) {
                        Ok(bytes) => bytes,
                        Err(error) => { failure = Some(error); return false; }
                    };
                    summary.partial = partial || counted.tally.partial;
                    cancelled = counted.cancelled;
                    if counted.done || cancelled { true } else { send(Ok(summary.fields(false, false))) }
                }
                Err(error) => { failure = Some(error); true }
            }
        });
        if let Some(error) = failure { return send(Err(error)); }
        if !open { return false; }
        if cancelled || cancel.check().is_err() { return send(Ok(summary.fields(false, true))); }
    }
    // A replacement or disappearance while a folder was walked cannot publish an exact total for the old selection.
    for item in items {
        if cancel.check().is_err() { return send(Ok(summary.fields(false, true))); }
        if let Err(error) = item.current() { return send(Err(error)); }
    }
    send(Ok(summary.fields(true, cancel.check().is_err())))
}
