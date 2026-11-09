//! Follow-on yazi actions share the existing listing, operation and terminal lifecycles.
use super::{
    model::Model,
    wire::{count, flag, number, text, word, Wire},
};
use crate::jsondoc::Json;
use std::{io, path::PathBuf};

#[derive(Default)]
pub struct State {
    pub suspend: bool,
    pub copy_id: usize,
    pub copy_listing: Option<usize>,
    pub copy_text: Option<&'static str>,
    pub copied: Vec<crate::backend::linkheld::Identity>,
    pub follow: Option<(usize, PathBuf, Option<usize>)>,
    pub finder: super::finder::Finder,
}
pub fn act(m: &mut Model, action: &str, w: &mut Wire) -> io::Result<bool> {
    match action {
        "suspend" => m.parity17.suspend = true,
        "copyPath" | "copyRelative" | "copyUri" | "copydirpath" | "copyDirUri" | "copyName"
        | "copyStem" | "copyQuoted" => {
            if m.pending_clipboard || m.parity17.copy_text.is_some() {
                return Ok(true);
            }
            m.parity17.copy_id = m.parity17.copy_id.wrapping_add(1).max(1);
            m.parity17.copy_listing = w.listing;
            m.parity17.copy_text = Some(match action {
                "copyPath" => "path",
                "copyRelative" => "relative",
                "copyQuoted" => "quoted",
                "copyUri" => "url",
                "copydirpath" => "dirpath",
                "copyDirUri" => "dirurl",
                "copyName" => "filename",
                _ => "name_without_ext",
            });
            w.send(vec![
                ("c", word("paths")),
                ("rows", m.indices()),
                ("hold", Json::Bool(true)),
                ("id", number(m.parity17.copy_id)),
            ])?;
        }
        "pasteLink" | "pasteAbsoluteLink" | "pasteHardLink" => {
            if !m.cut {
                super::prompts::paste(m, action, w)?;
            }
        }
        "detailBirth" | "detailOff" => {
            m.detail.mode = if action == "detailBirth" {
                super::detail::Column::Birth
            } else {
                super::detail::Column::Off
            };
            m.say_text(super::words::args(
                "tui.frame.detail",
                vec![super::words::Text::Key(m.detail.mode.key())],
            ));
        }
        "findPrevious" => {
            m.editor = Some(super::editor::Editor::new(
                "findPrevious",
                m.parity17.finder.query.clone(),
                m.path.clone(),
            ))
        }
        "findNext" => super::finder::next(m, false, w)?,
        "followLink" => {
            if let Some(row) = m.rows.get(&m.cursor).filter(|row| !row.link.is_empty()) {
                let path = m.row_path(row);
                m.action_id = m.action_id.wrapping_add(1).max(1);
                w.send(vec![
                    ("c", word("linktarget")),
                    ("target", word(&row.link)),
                    ("path", word(&path.to_string_lossy())),
                    ("id", number(m.action_id)),
                ])?;
                m.parity17.follow = Some((m.action_id, path, w.listing));
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
pub fn receive(m: &mut Model, value: &Json, w: &mut Wire) -> io::Result<bool> {
    if text(value, "t") == "found" {
        super::finder::receive(m, value, w)?;
        return Ok(true);
    }
    if text(value, "t") == "linktarget" {
        if m.parity17
            .follow
            .as_ref()
            .is_some_and(|pending| pending.0 != count(value, "id"))
        {
            return Ok(true);
        }
        if let Some((id, path, listing)) = m.parity17.follow.take() {
            if id == count(value, "id")
                && path.to_string_lossy() == text(value, "path")
                && listing == w.listing
                && m.pending.is_none()
            {
                let folder = PathBuf::from(text(value, "directory"));
                let name = text(value, "name");
                if folder.is_absolute()
                    && std::path::Path::new(name)
                        .file_name()
                        .is_some_and(|n| n == name)
                {
                    m.restore_path = Some(folder.join(name));
                    super::actions::navigate(m, folder, w)?;
                }
            }
        }
        return Ok(true);
    }
    if text(value, "t") == "error"
        && (text(value, "where") == "paths"
            || (text(value, "where") == "stale" && text(value, "path") == "paths"))
    {
        if value.get("id").is_some() && count(value, "id") != m.parity17.copy_id {
            return Ok(true);
        }
        m.parity17.copy_text = None;
        m.pending_clipboard = false;
        if value.get("listing").is_some() && stale(m, value, w) {
            return Ok(true);
        }
    }
    if text(value, "t") == "paths" {
        if value.get("id").is_some() && count(value, "id") != m.parity17.copy_id {
            return Ok(true);
        }
        if value.get("id").is_none() && m.choose_pending {
            return Ok(false);
        }
        if value.get("id").is_some() && m.discard_clipboard {
            m.discard_clipboard = false;
            m.pending_clipboard = false;
            return Ok(true);
        }
        if value.get("listing").is_some() && stale(m, value, w) {
            m.parity17.copy_text = None;
            m.pending_clipboard = false;
            return Ok(true);
        }
        let paths: Vec<String> = value
            .get("paths")
            .and_then(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(Json::as_str)
            .map(str::to_owned)
            .collect();
        if let Some(form) = m.parity17.copy_text.take() {
            if paths.iter().any(|p| !std::path::Path::new(p).is_absolute()) {
                return Ok(true);
            }
            let payload = copy_text(&paths, form, &m.path);
            w.send(vec![
                ("c", word("clipSet")),
                ("op", word("text")),
                ("paths", Json::Arr(vec![word(&payload)])),
            ])?;
            return Ok(true);
        }
        if m.pending_clipboard && !m.discard_clipboard {
            m.parity17.copied = value
                .get("held")
                .and_then(|v| crate::backend::linkheld::decode(v).ok())
                .unwrap_or_default();
            if value.get("id").is_some() {
                m.pending_clipboard = false;
                m.clipboard = paths;
                m.say_text(super::prompts::clipboard_text(m));
                return Ok(true);
            }
        }
        if value.get("id").is_some() {
            return Ok(true);
        }
    }
    if text(value, "t") == "clip" && text(value, "op") == "set" {
        if !flag(value, "ok") {
            m.fail(text(value, "error").into());
        }
        return Ok(true);
    }
    if text(value, "t") == "linked" {
        if count(value, "ok") > 0 {
            super::prompts::record(m, "link", Vec::new());
            m.say_text(super::prompts::undoable_text(
                m,
                super::words::Text::Key("step.link"),
            ));
        }
        if count(value, "failed") > 0 {
            m.fail(text(value, "note").into());
        }
        m.refresh(w)?;
        return Ok(true);
    }
    Ok(false)
}
// A folder's listed line carries no listing stamp, so a copy asked without one is judged by navigation alone.
fn stale(m: &Model, value: &Json, w: &Wire) -> bool {
    let asked = m.parity17.copy_listing;
    asked != w.listing
        || m.pending.is_some()
        || asked.is_some_and(|listing| listing != count(value, "listing"))
}
pub fn copy_text(paths: &[String], form: &str, cwd: &std::path::Path) -> String {
    use flea::ui_logic_mn1::copyas;
    let mut paths: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    if paths.is_empty() && matches!(form, "dirpath" | "dirurl") {
        paths.push(cwd.join("placeholder"));
    }
    paths
        .iter()
        .map(|path| {
            let chosen = match form {
                "dirpath" | "dirurl" => path.parent().unwrap_or(path),
                _ => path.as_path(),
            };
            match form {
                "url" | "dirurl" => uri(&chosen.to_string_lossy()),
                "filename" => path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                "name_without_ext" => path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                // The window's Copy As forms: from the listed folder, and one shell word.
                "relative" => copyas::relative(&chosen.to_string_lossy(), &cwd.to_string_lossy()),
                "quoted" => copyas::one(&chosen.to_string_lossy(), copyas::Format::Quoted, ""),
                _ => chosen.to_string_lossy().into_owned(),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn uri(path: &str) -> String {
    let mut out = String::from("file://");
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
