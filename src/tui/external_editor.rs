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

// The status sh exits with when it cannot find the command it was to run.
const SHELL_NOT_FOUND: i32 = 127;

// A shell-run editor's program named in a sentence when sh could not find it; an editor that ran and exited 127 itself keeps its status.
pub(super) fn missing(selected: &str, status: std::process::ExitStatus) -> Option<io::Error> {
    let program = arguments(selected)
        .ok()
        .and_then(|args| args.into_iter().next())
        .unwrap_or_else(|| selected.trim().to_owned());
    let executable = |path: &Path| crate::backend::jail::executable_file(path);
    let found = if program.contains('/') {
        executable(Path::new(&program))
    } else {
        std::env::var_os("PATH").is_some_and(|dirs| {
            std::env::split_paths(&dirs).any(|dir| executable(&dir.join(&program)))
        })
    };
    (status.code() == Some(SHELL_NOT_FOUND) && !found).then(|| {
        io::Error::other(super::words::args(
            "tui.launch.program_not_found",
            vec![program.into()],
        ))
    })
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
    if args.first().is_none_or(String::is_empty) { return Err(super::words::error("tui.external_editor.the_editor_command_needs_a_program")); }
    Ok(args)
}

#[cfg(test)]
#[path = "external_editor_tests.rs"]
mod tests;
