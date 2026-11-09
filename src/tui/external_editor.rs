use super::terminal::Terminal;
use std::io;
use std::path::Path;
use std::process::Command;

pub fn run(terminal: &mut Terminal, path: &Path) -> io::Result<()> {
    let command = command(chosen().as_deref(), path)?;
    terminal.run_command(command)
}

// One choice for E and bulk rename: $VISUAL, then $EDITOR, as the convention orders them; a blank value is unset.
pub(super) fn chosen() -> Option<String> {
    pick(std::env::var("VISUAL").ok(), std::env::var("EDITOR").ok())
}
fn pick(visual: Option<String>, editor: Option<String>) -> Option<String> {
    visual.filter(|s| !s.trim().is_empty()).or_else(|| editor.filter(|s| !s.trim().is_empty()))
}

pub(super) fn command(selected: Option<&str>, path: &Path) -> io::Result<Command> {
    let selected = selected.ok_or_else(|| super::words::error("tui.external_editor.set_editor_or_visual_to_edit_a_file_in_this_terminal"))?;
    let args = arguments(selected)?;
    let meta = path.metadata()?;
    if !path.is_absolute() || !meta.is_file() { return Err(super::words::error("tui.external_editor.only_a_regular_file_can_be_edited")); }
    let mut command = Command::new(&args[0]);
    command.args(&args[1..]).arg("--").arg(path);
    Ok(command)
}

// Quotes group arguments and backslashes quote characters; no expansion or shell evaluation occurs.
// Sample input: 'editor program' --wait "two words".
fn arguments(text: &str) -> io::Result<Vec<String>> {
    let mut args = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut started = false;
    for c in text.chars() {
        if escaped { word.push(c); escaped = false; started = true; continue; }
        if c == '\\' && quote != Some('\'') { escaped = true; started = true; continue; }
        if let Some(q) = quote {
            if c == q { quote = None; } else { word.push(c); }
        } else if matches!(c, '\'' | '"') {
            quote = Some(c);
            started = true;
        } else if c.is_whitespace() {
            if started { args.push(std::mem::take(&mut word)); started = false; }
        } else {
            word.push(c);
            started = true;
        }
    }
    if quote.is_some() || escaped { return Err(super::words::error("tui.external_editor.the_editor_command_has_an_unfinished_quote_or_escape")); }
    if started { args.push(word); }
    if args.first().map_or(true, String::is_empty) { return Err(super::words::error("tui.external_editor.the_editor_command_needs_a_program")); }
    Ok(args)
}

#[cfg(test)]
#[path = "external_editor_tests.rs"]
mod tests;
