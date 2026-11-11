// The paste question: which names already exist before anything is written, the transfer that answers it, and its status row.
use super::super::{
    cells::{ellipsis, text_width},
    input::Key,
    model::Model,
    theme::Theme,
    wire::{count, number, text, word, Wire},
};
use super::Paste;
use crate::jsondoc::Json;
use flea::bytename::ExactText as _;
use std::io;

// Paste asks which names already exist before anything is written; a second paste waits for the first question's answer.
pub fn paste(m: &mut Model, action: &str, w: &mut Wire) -> io::Result<()> {
    if m.clipboard.is_empty() {
        m.say_key("paste.nothing");
        return Ok(());
    }
    if m.prompts.asked.is_some() || m.prompts.question.is_some() {
        m.fail_key("paste.waiting");
        return Ok(());
    }
    m.prompts.id = m.prompts.id.wrapping_add(1).max(1);
    let paste = Paste {
        id: m.prompts.id,
        op: match action {
            "pasteLink" => "relative",
            "pasteAbsoluteLink" => "absolute",
            "pasteHardLink" => "hard",
            _ if m.cut || action == "movePaste" => "move",
            _ => "copy",
        },
        paths: m.clipboard.clone(),
        dest: m.path.clone(),
        fat: None,
        total: 0,
        name: String::new(),
        held: m.parity17.copied.clone(),
    };
    if matches!(paste.op, "relative" | "absolute" | "hard") && paste.held.len() != paste.paths.len()
    {
        m.fail_key("tui.editor.selected_item_changed_reopen_rename");
        return Ok(());
    }
    ask_collisions(&paste, w)?;
    m.prompts.asked = Some(paste);
    Ok(())
}
fn ask_collisions(paste: &Paste, w: &mut Wire) -> io::Result<()> {
    let mut fields = vec![
        ("c", word("collisions")),
        ("id", number(paste.id)),
        (
            "paths",
            Json::Arr(paste.paths.iter().map(|p| word(p)).collect()),
        ),
        ("dest", word(&paste.dest.exact_text())),
    ];
    if let Some(fat) = paste.fat {
        fields.push(("fat", word(fat)));
    }
    w.send(fields)
}
// Sample input: {"t":"collisions","id":7,"total":1,"names":[{"n":"notes.txt","d":false,"i":"text-plain"}],"mergeable":false}.
pub fn collided(m: &mut Model, value: &Json, w: &mut Wire) -> io::Result<()> {
    let Some(mut paste) = m.prompts.asked.take() else {
        return Ok(());
    };
    if paste.id != count(value, "id") {
        m.prompts.asked = Some(paste);
        return Ok(());
    }
    if text(value, "t") == "fatnames" {
        // A FAT-family drive refuses some names; the terminal lands them under the drive-safe names, so nothing is lost, and asks again.
        paste.fat = Some("rename");
        ask_collisions(&paste, w)?;
        m.prompts.asked = Some(paste);
        return Ok(());
    }
    paste.total = count(value, "total");
    if paste.total == 0 {
        return transfer(paste, "refuse", w);
    }
    // The exact name, so a name that is not UTF-8 matches its row and locates that file, never its lossy twin.
    paste.name = value
        .get("names")
        .and_then(Json::as_array)
        .and_then(|names| names.first())
        .map(|first| {
            if first.get("x").is_some() {
                text(first, "x")
            } else {
                text(first, "n")
            }
            .to_owned()
        })
        .unwrap_or_default();
    // The cursor waits on the name the paste meets, as the tui-collision board points at it behind the question; a name past the held rows is located.
    if paste.dest == m.path && m.filter.is_empty() {
        match m
            .rows
            .iter()
            .find(|(_, row)| row.name == paste.name)
            .map(|(index, _)| *index)
        {
            Some(index) => m.move_by(index as isize - m.cursor as isize, false, w)?,
            None => {
                let path = paste.dest.join(flea::bytename::text_path(&paste.name));
                w.send(vec![
                    ("c", word("locate")),
                    ("path", word(&path.exact_text())),
                ])?;
                m.restore_path = Some(path);
            }
        }
    }
    m.prompts.question = Some(paste);
    Ok(())
}
fn transfer(paste: Paste, collide: &str, w: &mut Wire) -> io::Result<()> {
    let linked = matches!(paste.op, "relative" | "absolute" | "hard");
    let mut fields = vec![
        ("c", word(if linked { "link" } else { "transfer" })),
        ("op", word(paste.op)),
        (
            "paths",
            Json::Arr(paste.paths.iter().map(|p| word(p)).collect()),
        ),
        ("dest", word(&paste.dest.exact_text())),
        ("collide", word(collide)),
        ("collideId", number(paste.id)),
    ];
    if linked {
        fields.push(("held", crate::backend::linkheld::encode(&paste.held)));
    }
    if let Some(fat) = paste.fat {
        fields.push(("fat", word(fat)));
    }
    w.send(fields)
}
// Enter or b keeps both, r replaces, s skips, Escape cancels; any other key leaves the question standing.
pub fn answer(m: &mut Model, key: &Key, w: &mut Wire) -> io::Result<()> {
    let cancel = key.name == "Escape"
        || (m.preset == "yazi"
            && key.mods == "ctrl"
            && matches!(key.name.as_str(), "C" | "BracketLeft"));
    let choice = match (key.name.as_str(), key.text.as_str(), key.mods.as_str()) {
        _ if cancel => "",
        ("Return" | "Enter", _, "") | (_, "b", "") => "keep",
        (_, "r", "") => "replace",
        (_, "s", "") => "skip",
        _ => return Ok(()),
    };
    let Some(paste) = m.prompts.question.take() else {
        return Ok(());
    };
    if choice.is_empty() {
        return Ok(());
    }
    transfer(paste, choice, w)
}
pub(super) fn ask(m: &Model, question: &Paste, theme: &Theme, columns: usize) -> String {
    let i18n = &m.prompts.i18n;
    let folder = question
        .dest
        .file_name()
        .map_or_else(
            || question.dest.to_string_lossy(),
            |name| name.to_string_lossy(),
        )
        .into_owned();
    let head = if question.total == 1 {
        i18n.tr_args(
            "collide.one",
            &[&flea::bytename::display(&question.name), &folder],
        )
    } else {
        i18n.trn_args("collide.many", question.total as i64, &[&folder])
    };
    let head_width = text_width(&head).min(columns);
    let keys_width = columns
        .saturating_sub(head_width + 2)
        .min(text_width(&i18n.tr("collide.keys")));
    let base = theme.base();
    let gap = if keys_width > 0 { 2 } else { 0 };
    format!(
        "{}{}{base}{}{}{}{base}{}",
        theme.foreground,
        ellipsis(&head, head_width),
        " ".repeat(gap),
        theme.muted,
        ellipsis(&i18n.tr("collide.keys"), keys_width),
        " ".repeat(columns - head_width - gap - keys_width)
    )
}
