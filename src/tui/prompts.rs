// The status row's prompts and receipts, the paste question, and the undo steps an undo of a trash lands the cursor on.
use super::{cells::{ellipsis, text_width}, editor::Editor, input::Key, model::Model, theme::Theme,
    wire::{count, flag, number, text, word, Wire}};
use crate::jsondoc::Json;
use flea::{i18n::I18n, ui_logic_sr1::{scope_label, Kind, Session}};
use std::{collections::BTreeSet, io, path::PathBuf, time::Duration};

#[derive(Default)]
pub struct Prompts {
    pub i18n: I18n,
    // The paste whose collisions question is out, then the question itself once names collide.
    asked: Option<Paste>,
    pub question: Option<Paste>,
    id: usize,
    // The trash requests still waiting for their answer, by request id, so each answer records once.
    trashing: BTreeSet<usize>,
    // This client's journaled steps, newest last; a step another client took leaves them out of step, and they clear.
    undo: Vec<Step>,
    redo: Vec<Step>,
    pub trash: Option<super::trashview::TrashView>,
}
#[cfg(test)]
impl Prompts {
    /// Tests read English only; the fields are spelled out because Self::default() would still load the host's language.
    pub fn english() -> Self {
        Self {
            i18n: crate::i18n::test_catalogs::library_english(),
            asked: None,
            question: None,
            id: 0,
            trashing: BTreeSet::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            trash: None,
        }
    }
}
pub struct Paste {
    id: usize,
    op: &'static str,
    paths: Vec<String>,
    dest: PathBuf,
    fat: Option<&'static str>,
    total: usize,
    name: String,
    held: Vec<crate::backend::linkheld::Identity>,
}
struct Step { op: &'static str, paths: Vec<PathBuf> }

/// The status row: the paste question, an open prompt with its field, or the footer; and where the terminal's caret goes.
pub fn status(m: &Model, theme: &Theme, columns: usize, elapsed: Duration) -> (String, Option<usize>) {
    if let Some(question) = &m.prompts.question { return (ask(m, question, theme, columns), None); }
    let Some(editor) = &m.editor else { return (super::layout::footer(m, theme, columns, elapsed), None); };
    let i18n = &m.prompts.i18n;
    let base = theme.base();
    let label = i18n.tr(match editor.kind {
        "rename" => "prompt.rename",
        // One prompt makes either entry, so the line states the rule and the draft never retitles it.
        "mkdir" | "newfile" => "prompt.create",
        "path" => "prompt.path", "filter" => "prompt.filter", _ => "prompt.find",
    });
    // A row this narrow still owes the field cells to show the name and the caret; past that the rule gives way.
    const FIELD_FLOOR: usize = 12;
    let label_width = text_width(&label);
    // An error sentence must draw cells, or Enter reads as doing nothing on a narrow row: it takes
    // room from the rule first, then from the field, and never past the field's floor.
    let error_room = if editor.error.is_empty() {
        0
    } else {
        text_width(&editor.error).min(columns / 2).max(1)
    };
    let mut used = if label_width + 1 + FIELD_FLOOR + error_room <= columns {
        label_width
    } else {
        columns.saturating_sub(1 + FIELD_FLOOR + error_room)
    };
    let mut out = format!(
        "{}{}",
        theme.muted,
        if used < label_width {
            ellipsis(&label, used)
        } else {
            label.clone()
        }
    );
    used = used.min(columns);
    let ghost = if editor.kind == "path" { m.completion.strip_prefix(editor.value.as_str()).unwrap_or("") } else { "" };
    let (field, cells, caret) =
        editor.field(ghost, columns.saturating_sub(used + 1 + error_room), theme);
    let caret = caret.filter(|_| cells > 0).map(|at| used + 1 + at);
    if cells > 0 {
        out.push_str(&format!("{base} {field}"));
        used += 1 + cells;
    }
    out.push_str(&base);
    let (after, ink, right) = after(m, editor, theme);
    let room = columns.saturating_sub(used + 1);
    if !after.is_empty() && room > 0 && (!right || text_width(&after) < room) {
        let width = text_width(&after).min(room);
        let gap = if right { columns - used - width } else { 1 };
        out.push_str(&format!("{}{ink}{}{base}", " ".repeat(gap), ellipsis(&after, width)));
        used += gap + width;
    }
    out.push_str(&" ".repeat(columns - used));
    (out, caret)
}
/// What follows a prompt's field: its one error sentence, a working note, the filter's count, Find's scope or Rename's keys at the trailing end.
fn after<'a>(m: &Model, editor: &Editor, theme: &'a Theme) -> (String, &'a str, bool) {
    let i18n = &m.prompts.i18n;
    if !editor.error.is_empty() { return (editor.error.clone(), &theme.error, false); }
    if editor.pending { return (i18n.tr(if editor.kind == "rename" { "prompt.renaming" } else { "prompt.working" }), &theme.muted, false); }
    match editor.kind {
        "rename" => (i18n.tr("prompt.renameHint"), &theme.muted, true),
        "filter" => (shown(m), &theme.muted, false),
        "search" => (i18n.tr_args("tui.find.scope_hint", &[&scope(m, m.find.mode)]), &theme.muted, false),
        _ => (String::new(), &theme.muted, false),
    }
}
/// The filter's count: how many rows show of how many the folder holds
fn shown(m: &Model) -> String {
    m.prompts.i18n.tr_args("filter.shown", &[&m.i18n.locale().format_count(m.filter_count() as i64), &m.i18n.locale().format_count(m.total as i64)])
}
/// Find's scope words for the prompt's folder, its root and whether the query reads names or contents
fn scope(m: &Model, contents: bool) -> String {
    let folder = m.editor.as_ref().filter(|e| e.kind == "search").map_or(&m.path, |e| &e.path);
    let home = std::env::var("HOME").unwrap_or_default();
    let root = Session::scope_root(&folder.to_string_lossy(), &home, m.find.session.scope);
    scope_label(&m.prompts.i18n, root.as_deref(), &home, m.find.session.scope, if contents { Kind::Contents } else { Kind::Names })
}
/// The footer's leading runs: an applied filter in muted ink with its count, Find's count and its query in muted ink, or the item count.
pub fn lead<'a>(m: &Model, theme: &'a Theme, count: String) -> Vec<(&'a str, String, usize)> {
    let i18n = &m.prompts.i18n;
    if !m.filter.is_empty() {
        return vec![(theme.muted.as_str(), format!("{} {}", i18n.tr("prompt.filter"), m.filter), 0), (theme.foreground.as_str(), shown(m), 2)];
    }
    if m.search.is_empty() { return vec![(theme.foreground.as_str(), count, 0)]; }
    let mut parts = if m.selected.is_empty() { Vec::new() } else { vec![(theme.foreground.as_str(), count, 0)] };
    parts.push((&theme.foreground, m.search.clone(), 2));
    parts.push((&theme.muted, i18n.tr_args("tui.find.status", &[&m.search_query, &scope(m, m.find.active.as_ref().is_some_and(|q| q.contents))]), 2));
    parts
}
/// Find's status: what it found, and what it has scanned only while it is still walking.
pub fn found(m: &Model, scanned: usize) -> String {
    if m.searching { m.prompts.i18n.tr_args("find.scanning", &[&m.i18n.locale().format_count(m.total as i64), &m.i18n.locale().format_count(scanned as i64)]) }
    else { m.prompts.i18n.tr_args("find.found", &[&m.i18n.locale().format_count(m.total as i64)]) }
}
/// The undo key's clause for a receipt, empty while no undo step stands
fn hint(m: &Model) -> String {
    if m.hints.undo.is_empty() { String::new() } else { format!(" {}", m.prompts.i18n.tr_args("receipt.undoes", &[&m.hints.undo])) }
}
/// A receipt for a step Undo can take back names the undo key after it.
pub fn undoable(m: &Model, line: String) -> String { format!("{line}{}", hint(m)) }
/// A receipt that names the undo key stays for the error time, then clears like every message.
pub fn lasting(m: &Model) -> bool {
    let hint = hint(m);
    !hint.is_empty() && m.message.ends_with(&hint)
}
/// The message run, with a trailing undo key in muted ink one cell after the receipt.
pub fn message<'a>(m: &Model, theme: &'a Theme) -> Vec<(&'a str, String, usize)> {
    let hint = hint(m);
    match m.message.strip_suffix(&hint).filter(|head| !hint.is_empty() && !head.is_empty()) {
        Some(head) => vec![(theme.foreground.as_str(), head.into(), 2), (theme.muted.as_str(), hint.trim_start().into(), 1)],
        None => vec![(theme.foreground.as_str(), m.message.clone(), 2)],
    }
}
/// The clipboard receipt, Cut or Copied with the held count; tests read it in English
#[cfg(test)]
#[allow(dead_code)]
pub fn clipboard(m: &Model) -> String {
    m.prompts.i18n.trn(if m.cut { "clipboard.cut" } else { "clipboard.copied" }, m.clipboard.len() as i64)
}
/// The first trash press says what the second does, "it" for one item and "them" for a selection.
#[cfg(test)]
pub fn trash_arm(m: &Model, arm: &str) -> String { trash_arm_text(m, arm).render(&m.i18n) }
/// The trash arm sentence as words, for one item or a selection, naming the key and its second press
pub fn trash_arm_text(m: &Model, arm: &str) -> super::words::Text {
    let many = m.selected.len() > 1;
    let key = match (many, m.hints.trash.is_empty()) {
        (false, false) => "trash.armOne", (true, false) => "trash.armMany", (false, true) => "trash.armOneAlone", (true, true) => "trash.armManyAlone",
    };
    let arm = m.keycaps.and_then(|snapshot| snapshot.label(arm).ok()).unwrap_or(arm);
    super::words::args(key, vec![arm.into(), m.hints.trash.clone().into()])
}
/// Sends a trash of the selection or the cursor row under a request id, so its answer is recorded once and names what moved.
pub fn trash(m: &mut Model, w: &mut Wire) -> io::Result<()> {
    if m.total == 0 { return Ok(()); }
    m.prompts.id = m.prompts.id.wrapping_add(1).max(1);
    let request = m.prompts.id;
    m.prompts.trashing.insert(request);
    w.send(vec![("c", word("trash")), ("id", number(request)), ("rows", m.indices())])
}
/// The catalog key naming an undo step's op, None for an op no receipt speaks of
fn step_key(op: &str) -> Option<&'static str> {
    let key = match op {
        "trash" => "step.trash", "rename" | "bulkrename" => "step.rename", "duplicate" => "step.duplicate", "mkdir" => "step.mkdir",
        "newfile" => "step.newfile", "copy" => "step.copy", "move" => "step.move", "merge" => "step.merge", "link" => "step.link",
        "permissions" => "step.permissions", _ => return None,
    };
    Some(key)
}
/// Records a step this client took so Undo can take it back, and drops the redo trail
pub fn record(m: &mut Model, op: &'static str, paths: Vec<PathBuf>) {
    m.prompts.undo.push(Step { op, paths });
    m.prompts.redo.clear();
}
/// The receipt for a finished step, its failures raised as one sentence; an undo of this client's trash anchors the cursor on what came back.
pub fn receipt(m: &mut Model, value: &Json) -> String {
    let i18n = &m.prompts.i18n;
    let (ok, failed, skipped) = (count(value, "ok"), count(value, "failed"), count(value, "skipped"));
    let path = PathBuf::from(text(value, "path"));
    match text(value, "t") {
        "renamed" => { record(m, "rename", vec![path]); undoable(m, m.prompts.i18n.tr("receipt.renamed")) }
        "made" => { record(m, "mkdir", vec![path]); undoable(m, m.prompts.i18n.tr("receipt.made")) }
        "duplicated" => { record(m, "duplicate", vec![path]); undoable(m, m.prompts.i18n.tr("receipt.duplicated")) }
        "undone" | "redone" => {
            let undone = text(value, "t") == "undone";
            let line = match step_key(text(value, "op")) {
                Some(step) => i18n.tr_args(if undone { "receipt.undid" } else { "receipt.redid" }, &[&i18n.tr(step)]),
                None => i18n.tr(if undone { "receipt.undidLast" } else { "receipt.redidLast" }),
            };
            let (from, to) = if undone { (&mut m.prompts.undo, &mut m.prompts.redo) } else { (&mut m.prompts.redo, &mut m.prompts.undo) };
            match from.pop().filter(|taken| taken.op == text(value, "op")) {
                Some(taken) => {
                    if undone && taken.op == "trash" && m.search.is_empty() {
                        m.restore_path = taken.paths.iter().find(|path| path.parent() == Some(m.path.as_path())).cloned().or(m.restore_path.take());
                    }
                    to.push(taken);
                }
                None => { m.prompts.undo.clear(); m.prompts.redo.clear(); }
            }
            if undone { line } else { undoable(m, line) }
        }
        "trashed" => {
            if failed > 0 {
                let why = text(value, "err").trim_end_matches('.');
                let sentence = if why.is_empty() { i18n.trn("trash.failed", failed as i64) } else { i18n.trn_args("trash.failedWhy", failed as i64, &[why]) };
                let _ = sentence;
                m.fail_text(super::words::Text::Plural(if why.is_empty() { "trash.failed" } else { "trash.failedWhy" }, failed, if why.is_empty() { vec![] } else { vec![why.into()] }));
            }
            // An answer whose request this client no longer waits on records nothing, so no undo step is taken twice.
            if !m.prompts.trashing.remove(&count(value, "id")) || ok == 0 { return String::new(); }
            let moved: Vec<PathBuf> = value.get("moved").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(PathBuf::from).collect();
            let line = m.prompts.i18n.trn("receipt.trashed", ok as i64);
            record(m, "trash", moved);
            undoable(m, line)
        }
        _ => {
            let mut line = i18n.trn(if m.transfer_moving { "receipt.moved" } else { "receipt.copied" }, ok as i64);
            if skipped > 0 { line.push_str(&format!(" {}", i18n.tr_args("receipt.skipped", &[&i18n.locale().format_count(skipped as i64)]))); }
            if failed > 0 { line.push_str(&format!(" {}", i18n.tr_args("receipt.failed", &[&i18n.locale().format_count(failed as i64)]))); }
            if flag(value, "cancelled") { line.push_str(&format!(" {}", i18n.tr("receipt.cancelled"))); }
            if failed > 0 { m.fail_text(super::words::Text::Plural("transfer.failed", failed, vec![])); }
            if ok == 0 { return line; }
            record(m, if m.transfer_moving { "move" } else { "copy" }, Vec::new());
            undoable(m, line)
        }
    }
}
/// A receipt's words with the undo key appended while an undo step stands
pub fn undoable_text(m: &Model, line: super::words::Text) -> super::words::Text {
    use super::words::{Text, args};
    if m.hints.undo.is_empty() { line } else { Text::Joined(vec![line, Text::from(" "), args("receipt.undoes", vec![Text::from(m.hints.undo.clone())])]) }
}
/// The clipboard receipt as words, plural on the held count
pub fn clipboard_text(m: &Model) -> super::words::Text {
    super::words::Text::Plural(if m.cut { "clipboard.cut" } else { "clipboard.copied" }, m.clipboard.len(), vec![])
}
/// A backend answer's receipt as words instead of ink, for the model to record and say
pub fn receipt_text(m: &Model, value: &Json) -> super::words::Text {
    use super::words::{Text, args};
    let (ok, failed, skipped) = (count(value, "ok"), count(value, "failed"), count(value, "skipped"));
    let line = match text(value, "t") {
        "renamed" => Text::Key("receipt.renamed"),
        "made" => Text::Key("receipt.made"),
        "duplicated" => Text::Key("receipt.duplicated"),
        "trashed" => Text::Plural("receipt.trashed", ok, vec![]),
        "undone" | "redone" => {
            let undone = text(value, "t") == "undone";
            let line = match step_key(text(value, "op")) {
                Some(step) => args(if undone { "receipt.undid" } else { "receipt.redid" }, vec![Text::Key(step)]),
                None => Text::Key(if undone { "receipt.undidLast" } else { "receipt.redidLast" }),
            };
            return if undone { line } else { undoable_text(m, line) };
        }
        _ => {
            let mut parts = vec![Text::Plural(if m.transfer_moving { "receipt.moved" } else { "receipt.copied" }, ok, vec![])];
            for (key, count) in [("receipt.skipped", skipped), ("receipt.failed", failed)] {
                if count > 0 { parts.extend([Text::from(" "), args(key, vec![Text::Count(count)])]); }
            }
            if flag(value, "cancelled") { parts.extend([Text::from(" "), Text::Key("receipt.cancelled")]); }
            let line = Text::Joined(parts);
            return if ok == 0 { line } else { undoable_text(m, line) };
        }
    };
    undoable_text(m, line)
}
/// Paste asks which names already exist before anything is written; a second paste waits for the first question's answer.
pub fn paste(m: &mut Model, action: &str, w: &mut Wire) -> io::Result<()> {
    if m.clipboard.is_empty() { m.say_key("paste.nothing"); return Ok(()); }
    if m.prompts.asked.is_some() || m.prompts.question.is_some() { m.fail_key("paste.waiting"); return Ok(()); }
    m.prompts.id = m.prompts.id.wrapping_add(1).max(1);
    let paste = Paste { id: m.prompts.id, op: match action { "pasteLink" => "relative", "pasteAbsoluteLink" => "absolute", "pasteHardLink" => "hard", _ if m.cut || action == "movePaste" => "move", _ => "copy" }, paths: m.clipboard.clone(),
        dest: m.path.clone(), fat: None, total: 0, name: String::new(), held: m.parity17.copied.clone() };
    if matches!(paste.op, "relative" | "absolute" | "hard") && paste.held.len() != paste.paths.len() {
        m.fail_key("tui.editor.selected_item_changed_reopen_rename");
        return Ok(());
    }
    ask_collisions(&paste, w)?;
    m.prompts.asked = Some(paste);
    Ok(())
}
/// Asks the backend which of the paste's names already exist in the destination
fn ask_collisions(paste: &Paste, w: &mut Wire) -> io::Result<()> {
    let mut fields = vec![("c", word("collisions")), ("id", number(paste.id)), ("paths", Json::Arr(paste.paths.iter().map(|p| word(p)).collect())),
        ("dest", word(&paste.dest.to_string_lossy()))];
    if let Some(fat) = paste.fat { fields.push(("fat", word(fat))); }
    w.send(fields)
}
// Sample input: {"t":"collisions","id":7,"total":1,"names":[{"n":"notes.txt","d":false,"i":"text-plain"}],"mergeable":false}.
/// Takes the collisions answer: a FAT drive asks again under drive-safe names, else the question stands or the transfer starts
pub fn collided(m: &mut Model, value: &Json, w: &mut Wire) -> io::Result<()> {
    if m.prompts.asked.as_ref().map_or(true, |paste| paste.id != count(value, "id")) { return Ok(()); }
    let mut paste = m.prompts.asked.take().unwrap();
    if text(value, "t") == "fatnames" {
        // A FAT-family drive refuses some names; the terminal lands them under the drive-safe names, so nothing is lost, and asks again.
        paste.fat = Some("rename");
        ask_collisions(&paste, w)?;
        m.prompts.asked = Some(paste);
        return Ok(());
    }
    paste.total = count(value, "total");
    if paste.total == 0 { return transfer(paste, "refuse", w); }
    paste.name = value.get("names").and_then(Json::as_array).and_then(|names| names.first()).map(|first| text(first, "n").to_owned()).unwrap_or_default();
    // The cursor waits on the name the paste meets, as the tui-collision board points at it behind the question; a name past the held rows is located.
    if paste.dest == m.path && m.filter.is_empty() {
        match m.rows.iter().find(|(_, row)| row.name == paste.name).map(|(index, _)| *index) {
            Some(index) => m.move_by(index as isize - m.cursor as isize, false, w)?,
            None => {
                let path = paste.dest.join(&paste.name);
                w.send(vec![("c", word("locate")), ("path", word(&path.to_string_lossy()))])?;
                m.restore_path = Some(path);
            }
        }
    }
    m.prompts.question = Some(paste);
    Ok(())
}
/// Sends the paste to the backend with the answer chosen, as a link when the op links
fn transfer(paste: Paste, collide: &str, w: &mut Wire) -> io::Result<()> {
    let linked = matches!(paste.op, "relative" | "absolute" | "hard");
    let mut fields = vec![("c", word(if linked { "link" } else { "transfer" })), ("op", word(paste.op)), ("paths", Json::Arr(paste.paths.iter().map(|p| word(p)).collect())),
        ("dest", word(&paste.dest.to_string_lossy())), ("collide", word(collide)), ("collideId", number(paste.id))];
    if linked { fields.push(("held", crate::backend::linkheld::encode(&paste.held))); }
    if let Some(fat) = paste.fat { fields.push(("fat", word(fat))); }
    w.send(fields)
}
/// Enter or b keeps both, r replaces, s skips, Escape cancels; any other key leaves the question standing.
pub fn answer(m: &mut Model, key: &Key, w: &mut Wire) -> io::Result<()> {
    let cancel = key.name == "Escape" || (m.preset == "yazi" && key.mods == "ctrl" && matches!(key.name.as_str(), "C" | "BracketLeft"));
    let choice = match (key.name.as_str(), key.text.as_str(), key.mods.as_str()) {
        _ if cancel => "",
        ("Return" | "Enter", _, "") | (_, "b", "") => "keep",
        (_, "r", "") => "replace",
        (_, "s", "") => "skip",
        _ => return Ok(()),
    };
    let paste = m.prompts.question.take().unwrap();
    if choice.is_empty() { return Ok(()); }
    transfer(paste, choice, w)
}
/// The collisions question's status row: what collides, then the keys that answer it
fn ask(m: &Model, question: &Paste, theme: &Theme, columns: usize) -> String {
    let i18n = &m.prompts.i18n;
    let folder = question.dest.file_name().map_or_else(|| question.dest.to_string_lossy(), |name| name.to_string_lossy()).into_owned();
    let head = if question.total == 1 { i18n.tr_args("collide.one", &[&question.name, &folder]) }
        else { i18n.trn_args("collide.many", question.total as i64, &[&folder]) };
    let head_width = text_width(&head).min(columns);
    let keys_width = columns.saturating_sub(head_width + 2).min(text_width(&i18n.tr("collide.keys")));
    let base = theme.base();
    let gap = if keys_width > 0 { 2 } else { 0 };
    format!("{}{}{base}{}{}{}{base}{}", theme.foreground, ellipsis(&head, head_width), " ".repeat(gap), theme.muted,
        ellipsis(&i18n.tr("collide.keys"), keys_width), " ".repeat(columns - head_width - gap - keys_width))
}
/// No preview stands for a cursor row the filter does not draw, so a query that matches nothing leaves the pane empty; one lookup, never a copy of the matches.
pub fn previewed(m: &Model) -> bool {
    #[cfg(test)]
    GATE.with(|gate| gate.set((gate.get().0 + 1, gate.get().1)));
    if m.filter.is_empty() { return true; }
    match super::filter::answered(m) {
        Some(rows) => rows.binary_search_by(|row| {
            #[cfg(test)]
            GATE.with(|gate| gate.set((gate.get().0, gate.get().1 + 1)));
            row.cmp(&m.cursor)
        }).is_ok(),
        None => m.rows.get(&m.cursor).is_some_and(|row| row.name.to_lowercase().contains(&m.filter.to_lowercase())),
    }
}
// The gate's calls and comparisons on this thread, for the cost test.
#[cfg(test)]
thread_local! { pub(super) static GATE: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) }; }
/// The terminal's own bar cursor is the caret while a prompt holds a cell for it, and it hides otherwise.
pub fn caret(at: Option<usize>, lines: usize) -> String {
    at.map_or_else(|| "\x1b[?25l".into(), |x| format!("\x1b[{lines};{}H\x1b[6 q\x1b[?25h", x + 2))
}

#[cfg(test)]
#[path = "prompts_tests.rs"]
mod tests;
