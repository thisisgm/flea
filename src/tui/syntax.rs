use super::model::Model;
use super::theme::Theme;
use super::wire::{count, number, text, word, Wire};
use crate::jsondoc::Json;
use std::io;

// The backend's slot for the TUI's one text preview, and how many lines one request colours.
const SLOT: &str = "tui";
const WINDOW: usize = 400;
// Syntax inks in bat's ANSI order, so a run's role indexes theme.syntax.
pub const SYNTAX_INKS: usize = 6;

// The backend's syntax runs for the shown text: per source line, each run's role and byte length.
#[derive(Default)]
pub struct Code {
    tokens: usize,
    token: usize,
    generation: usize,
    asked: usize,
    first: usize,
    lines: Vec<Vec<(u8, usize)>>,
    // Each laid-out text row's source line and byte offset, from the preview layout.
    pub parts: Vec<(usize, usize)>,
}

impl Code {
    // A new preview drops the last file's runs; the token count keeps rising so an older reply never matches.
    pub fn forget(&mut self) {
        self.token = 0;
        self.lines.clear();
    }
}

// Asks for the coloured window around the shown lines, sending the text the preview already holds, never the path.
pub fn request(m: &mut Model, wire: &mut Wire) -> io::Result<()> {
    if m.preview_body.is_empty() || m.selected.len() > 1 || (!m.preview_visible && !m.quicklook) {
        return Ok(());
    }
    let code = &m.preview_state.code;
    let (Some(top), Some(bottom)) = (code.parts.first().map(|part| part.0), code.parts.last().map(|part| part.0)) else { return Ok(()) };
    let covered = code.token != 0 && code.generation == m.preview_generation && top >= code.asked && bottom < code.asked + WINDOW;
    let Some(name) = super::pane_state::content(m).or_else(|| super::trashview::content(m)).and_then(|path| path.file_name().map(|name| name.to_string_lossy().into_owned())) else { return Ok(()) };
    if covered {
        return Ok(());
    }
    let first = top.saturating_sub(WINDOW / 4).max(1);
    let code = &mut m.preview_state.code;
    code.tokens += 1;
    code.token = code.tokens;
    code.generation = m.preview_generation;
    code.asked = first;
    wire.send(vec![
        ("c", word("code")),
        ("token", number(code.token)),
        ("slot", word(SLOT)),
        ("text", word(&m.preview_body)),
        ("name", word(&name)),
        ("lang", word("")),
        ("first", number(first)),
        ("count", number(WINDOW)),
    ])
}

// Keeps a reply's runs only for lines whose runs spell exactly the line the preview shows.
// Sample input: {"t":"code","token":4,"slot":"tui","state":"styled","first":1,"total":2,"lines":[[5,"fn",0," main"],[]]}
pub fn receive(m: &mut Model, value: &Json) {
    let code = &mut m.preview_state.code;
    if text(value, "slot") != SLOT || count(value, "token") != code.token || code.token == 0 || code.generation != m.preview_generation {
        return;
    }
    code.first = count(value, "first").max(1);
    code.lines.clear();
    if text(value, "state") != "styled" {
        return;
    }
    let replies = value.get("lines").and_then(Json::as_array).unwrap_or(&[]);
    for (line, reply) in m.preview_body.lines().skip(code.first - 1).zip(replies) {
        let mut runs = Vec::new();
        let mut spelled = String::new();
        let parts = reply.as_array().unwrap_or(&[]);
        for pair in parts.chunks(2) {
            let (Some(Json::Num(role)), Some(Json::Str(run))) = (pair.first(), pair.get(1)) else { break };
            let role = role.parse::<u8>().unwrap_or(0).min(SYNTAX_INKS as u8);
            let role = if role == KEYWORD_ROLE && operator(run) { 0 } else { role };
            runs.push((role, run.len()));
            spelled.push_str(run);
        }
        code.lines.push(if spelled == line { runs } else { Vec::new() });
    }
}

// A laid-out text row in its syntax inks, fitted to width; a row without verified runs draws in foreground.
pub fn paint(m: &Model, index: usize, row: &str, width: usize, theme: &Theme) -> String {
    let code = &m.preview_state.code;
    let runs = code.parts.get(index)
        .filter(|_| code.generation == m.preview_generation)
        .and_then(|(source, offset)| Some((source.checked_sub(code.first).and_then(|i| code.lines.get(i))?, *offset)));
    let mut out = theme.foreground.clone();
    let mut used = 0;
    let mut role = 0;
    for (at, c) in row.char_indices() {
        let cell = super::cells::width(c);
        if used + cell > width {
            break;
        }
        if let Some((runs, offset)) = runs {
            let next = role_at(runs, offset + at);
            if next != role {
                role = next;
                out.push_str(if role == 0 { &theme.foreground } else { &theme.syntax[role as usize - 1] });
            }
        }
        out.push(c);
        used += cell;
    }
    out.push_str(&theme.foreground);
    out.push_str(&" ".repeat(width - used));
    out
}

// bat's ansi theme inks keywords and operators alike in magenta; a diff's `+` and a comment's `*` take other roles.
const KEYWORD_ROLE: u8 = 5;
// An operator alone keeps the foreground, as the tui-list-code board draws `=` (CTL2 ruling Q2, 2026-10-30).
fn operator(run: &str) -> bool {
    run.chars().any(|c| !c.is_whitespace()) && run.chars().all(|c| c.is_whitespace() || "=+-*%<>!&|^~?:".contains(c))
}

fn role_at(runs: &[(u8, usize)], at: usize) -> u8 {
    let mut end = 0;
    for (role, length) in runs {
        end += length;
        if at < end {
            return *role;
        }
    }
    0
}

#[cfg(test)]
#[path = "syntax_tests.rs"]
mod tests;
