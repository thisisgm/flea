use flea::bytename::ExactText as _;
use super::{
    input::Key,
    model::Model,
    wire::{word, Wire},
};
use std::{io, path::PathBuf};

pub fn edit(m: &mut Model, key: &Key, w: &mut Wire) -> io::Result<()> {
    let mut editor = m.editor.take().unwrap();
    let kind = editor.kind;
    if editor.pending {
        m.editor = Some(editor);
        return Ok(());
    }
    if key.name == "Escape"
        || (m.preset == "yazi"
            && key.mods == "ctrl"
            && matches!(key.name.as_str(), "C" | "BracketLeft"))
    {
        if kind == "filter" {
            m.apply_filter(String::new(), w)?;
        }
        if kind == "search" {
            if let Some(active) = &m.find.active {
                m.find.mode = active.contents;
                m.find.session.scope = active.scope;
            }
        }
        return Ok(());
    }
    // Only a plain Return or keypad Enter commits; Shift, Ctrl or Alt with Return leaves the line as it is.
    if matches!(key.name.as_str(), "Return" | "Enter") && !key.mods.is_empty() {
        m.editor = Some(editor);
        return Ok(());
    }
    // Down on a typing Find or Filter line commits it without acting on a row, so its results can be read first.
    let browse = matches!(kind, "search" | "filter") && key.name == "Down" && key.mods.is_empty();
    if key.name == "Return" || key.name == "Enter" || browse {
        if !editor.valid_localized(&m.i18n) {
            m.editor = Some(editor);
            return Ok(());
        }
        let value = &editor.value;
        match kind {
            "path" => {
                let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
                let p = super::completion::lexical(&super::completion::expand(
                    value,
                    &home,
                    &editor.path,
                ));
                super::actions::navigate(m, p, w)?;
            }
            "findPrevious" => super::finder::query(m, value.clone(), true, w)?,
            "filter" => m.apply_filter(value.clone(), w)?,
            "search" => {
                if value.is_empty() {
                    return Ok(());
                }
                let started = super::contents::start(m, value.clone(), editor.path.clone(), w)?;
                if !started {
                    m.editor = Some(editor);
                    return Ok(());
                }
            }
            "rename" => {
                w.send(vec![
                    ("c", word("rename")),
                    ("path", word(&editor.path.exact_text())),
                    ("to", word(value)),
                ])?;
            }
            "mkdir" | "newfile" => {
                let default_base = editor
                    .default_base
                    .as_deref()
                    .filter(|base| kind == "mkdir" && *base == value.as_str());
                let mut fields = vec![
                    ("c", word(kind)),
                    ("op", word("newFile")),
                    ("id", super::wire::number(m.action_id)),
                    ("path", word(&editor.path.exact_text())),
                    (
                        "name",
                        word(if default_base.is_some() { "" } else { value }),
                    ),
                ];
                if let Some(base) = default_base {
                    fields.push(("defaultBase", word(base)));
                }
                w.send(fields)?;
            }
            _ => {}
        }
        if matches!(kind, "rename" | "mkdir" | "newfile") {
            editor.pending = true;
            m.editor = Some(editor);
        }
        return if browse {
            Ok(())
        } else {
            enter_once::arm(m, kind, w)
        };
    } else if key.name == "Tab" && kind == "path" {
        if !m.completion.is_empty() {
            editor.replace(m.completion.clone());
        }
    } else if key.name == "G" && key.mods == "ctrl" && kind == "search" {
        m.find.mode = !m.find.mode;
    } else if key.name == "Tab" && kind == "search" {
        // Tab skips a scope that would search this same folder again; with none left the scope stays.
        if let Some(scope) = super::prompts::tab_scope(&m.find.session, &editor.path) {
            m.find.session.scope = scope;
        }
    } else {
        editor.update(key);
    }
    if kind == "filter" {
        m.apply_filter(editor.value.clone(), w)?;
    } else if kind == "findPrevious" {
        super::finder::query(m, editor.value.clone(), true, w)?;
    }
    if kind == "path" {
        m.completion = m.completer.request(&editor.value, &editor.path);
    }
    m.editor = Some(editor);
    Ok(())
}

#[path = "enter_once.rs"]
pub(super) mod enter_once;
