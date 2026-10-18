use super::terminal::Terminal;
use std::io;
use std::path::Path;
use std::process::Command;

pub fn run(terminal: &mut Terminal, path: &Path) -> io::Result<()> {
    let editor = std::env::var("EDITOR").ok();
    let visual = std::env::var("VISUAL").ok();
    let command = command(editor.as_deref(), visual.as_deref(), path)?;
    terminal.run_command(command)
}

pub(super) fn command(editor: Option<&str>, visual: Option<&str>, path: &Path) -> io::Result<Command> {
    let selected = editor.filter(|s| !s.trim().is_empty()).or_else(|| visual.filter(|s| !s.trim().is_empty()))
        .ok_or_else(|| io::Error::other("Set $EDITOR or $VISUAL to edit a file in this terminal"))?;
    let args = arguments(selected)?;
    let meta = path.metadata()?;
    if !path.is_absolute() || !meta.is_file() { return Err(io::Error::other("Only a regular file can be edited")); }
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
    if quote.is_some() || escaped { return Err(io::Error::other("The editor command has an unfinished quote or escape")); }
    if started { args.push(word); }
    if args.first().map_or(true, String::is_empty) { return Err(io::Error::other("The editor command needs a program")); }
    Ok(args)
}

#[cfg(test)]
#[path = "external_editor_tests.rs"]
mod tests;
