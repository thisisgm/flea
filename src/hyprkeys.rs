// The Hyprland config Flea writes: the two Omarchy file-manager keys for flea --default, and the
// picker's floating treatment for flea --picker, each one additive block in ~/.config/hypr/bindings.lua.
use crate::userfile::{config_home, replace_file};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

// Markers at line start, so the inverse finds the block whatever the user wrote after it.
const BEGIN: &str = "-- flea --default: begin. Written by `flea --default`; `flea --default off` removes the block whole.";
const END: &str = "-- flea --default: end.";
const FLOAT_BEGIN: &str = "-- flea --picker: begin. Written by `flea --picker`; `flea --picker off` removes the block whole.";
const FLOAT_END: &str = "-- flea --picker: end.";
// The picker's own app id, ui/picker.qml's AppId pragma, which is not the window's.
const PICKER_APP_ID: &str = "com.thisisgm.flea.picker";
// Omarchy binds both to Nautilus in default/hypr/bindings/applications.lua; the cwd one opens on the active terminal's directory.
const BINDS: [(&str, &str, &str); 2] = [
    ("SUPER + SHIFT + F", "File manager", "flea --gui"),
    ("SUPER + ALT + SHIFT + F", "File manager (cwd)", "flea --gui \"$(omarchy-cmd-terminal-cwd)\""),
];
// hyprctl binds reports a chord as a mask: SUPER is 64, ALT is 8, SHIFT is 1.
const MODMASKS: [u32; 2] = [65, 73];

pub fn claim() -> Result<String, String> {
    let path = bindings_path()?;
    let text = read(&path)?;
    if has_block(&text) {
        return Ok(format!("keys: already Flea's, the flea --default block is in {}", path.display()));
    }
    if let Some(errors) = configerrors().filter(|e| !e.is_empty()) {
        return Err(format!("hyprctl configerrors already reports a problem, fix that first so a change here can be told apart from it: {}", errors));
    }
    let were = were_line(current_descriptions());
    replace_file(&path, &with_block(&text))?;
    let keys = format!("{} and {}", BINDS[0].0, BINDS[1].0);
    if !reload() {
        return Ok(format!(
            "keys: {} open Flea, {}; block appended to {}, not reloaded because hyprctl could not be reached, so run hyprctl reload inside the session",
            keys, were, path.display()
        ));
    }
    if let Some(errors) = configerrors().filter(|e| !e.is_empty()) {
        // Never leave a config that does not load: put the file back as it was and reload that.
        replace_file(&path, &text)?;
        reload();
        return Err(format!("the block broke the Hyprland config and was removed again; hyprctl configerrors said: {}", errors));
    }
    Ok(format!("keys: {} open Flea, {}; block appended to {} and reloaded", keys, were, path.display()))
}

pub fn release() -> Result<String, String> {
    let path = bindings_path()?;
    let text = read(&path)?;
    let Some(without) = without_block(&text)? else {
        return Ok(format!("keys: nothing to undo, no flea --default block in {}", path.display()));
    };
    replace_file(&path, &without)?;
    if !reload() {
        return Ok(format!(
            "keys: block removed from {}, not reloaded because hyprctl could not be reached, so run hyprctl reload inside the session",
            path.display()
        ));
    }
    Ok(format!("keys: block removed from {} and reloaded, so Omarchy's own bindings are back", path.display()))
}

// flea --picker's half: Omarchy tags xdg-desktop-portal-gtk's windows "+floating-window" because
// prompts belong in the floating treatment, and its own tag rules then float, centre and size them.
// A chooser is that same kind of window, so this asks for the same tag rather than inventing a size.
pub fn float_claim() -> Result<String, String> {
    let path = bindings_path()?;
    let text = read(&path)?;
    if begin_at(&text, FLOAT_BEGIN).is_some() {
        return Ok(format!("window: already floating, the flea --picker block is in {}", path.display()));
    }
    if let Some(errors) = configerrors().filter(|e| !e.is_empty()) {
        return Err(format!("hyprctl configerrors already reports a problem, fix that first so a change here can be told apart from it: {}", errors));
    }
    replace_file(&path, &appended(&text, &float_block()))?;
    if !reload() {
        return Ok(format!(
            "window: the picker takes Omarchy's floating treatment; block appended to {}, not reloaded because hyprctl could not be reached, so run hyprctl reload inside the session",
            path.display()
        ));
    }
    if let Some(errors) = configerrors().filter(|e| !e.is_empty()) {
        // Never leave a config that does not load: put the file back as it was and reload that.
        replace_file(&path, &text)?;
        reload();
        return Err(format!("the block broke the Hyprland config and was removed again; hyprctl configerrors said: {}", errors));
    }
    Ok(format!("window: the picker takes Omarchy's floating treatment; block appended to {} and reloaded", path.display()))
}

pub fn float_release() -> Result<String, String> {
    let path = bindings_path()?;
    let text = read(&path)?;
    let Some(without) = cut(&text, FLOAT_BEGIN, FLOAT_END)? else {
        return Ok(format!("window: nothing to undo, no flea --picker block in {}", path.display()));
    };
    replace_file(&path, &without)?;
    reload();
    Ok(format!("window: block removed from {}, so the picker tiles like any other window", path.display()))
}

pub fn float_block() -> String {
    format!("\n{}\no.window(\"{}\", {{ tag = \"+floating-window\" }})\n{}\n", FLOAT_BEGIN, PICKER_APP_ID, FLOAT_END)
}

fn bindings_path() -> Result<PathBuf, String> {
    Ok(config_home()?.join("hypr").join("bindings.lua"))
}

// Omarchy ships the file and hyprland.lua requires it, so a missing one is a broken box and not a first run.
fn read(path: &std::path::Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{} could not be read ({:?}), and it is where the key bindings go; Omarchy ships it", path.display(), e.kind()))
}

// The unbind comes first, or both bindings fire: the override shape the Omarchy manual documents.
pub fn block() -> String {
    let mut out = format!("\n{}\n", BEGIN);
    for (key, description, command) in BINDS {
        out.push_str(&format!("hl.unbind(\"{}\")\n", key));
        out.push_str(&format!("o.bind(\"{}\", \"{}\", {{ launch = '{}' }})\n", key, description, command));
    }
    out.push_str(END);
    out.push('\n');
    out
}

// The marker counts only at the start of a line, so a comment that quotes it is not a block.
fn begin_at(text: &str, begin: &str) -> Option<usize> {
    text.match_indices(begin).map(|(i, _)| i).find(|&i| i == 0 || text.as_bytes()[i - 1] == b'\n')
}

pub fn has_block(text: &str) -> bool {
    begin_at(text, BEGIN).is_some()
}

pub fn with_block(text: &str) -> String {
    appended(text, &block())
}

fn appended(text: &str, block: &str) -> String {
    let mut out = text.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(block);
    out
}

// Removes the block and the blank line with_block put in front of it; Err names a begin marker with no end.
pub fn without_block(text: &str) -> Result<Option<String>, String> {
    cut(text, BEGIN, END)
}

fn cut(text: &str, begin: &str, end_marker: &str) -> Result<Option<String>, String> {
    let Some(mut start) = begin_at(text, begin) else {
        return Ok(None);
    };
    let Some(end_offset) = text[start..].find(end_marker) else {
        return Err(format!("the begin marker \"{}\" has no end marker, so remove the block by hand", begin));
    };
    let mut end = start + end_offset + end_marker.len();
    if text[end..].starts_with('\n') {
        end += 1;
    }
    let bytes = text.as_bytes();
    let after_blank_line = start >= 1 && bytes[start - 1] == b'\n' && (start == 1 || bytes[start - 2] == b'\n');
    if after_blank_line {
        start -= 1;
    }
    Ok(Some(format!("{}{}", &text[..start], &text[end..])))
}

// One record per binding, blank-line separated, as hyprctl binds prints it:
//   bindd
//   	modmask: 65
//   	key: F
//   	description: File manager
// Returns the description on each chord in BINDS, in that order; None is a chord nothing is bound to.
pub fn descriptions(binds: &str) -> [Option<String>; 2] {
    let mut found: [Option<String>; 2] = [None, None];
    for record in binds.split("\n\n") {
        let field = |name: &str| record.lines().find_map(|l| l.trim().strip_prefix(name).map(|v| v.trim().to_string()));
        if field("key:").as_deref() != Some("F") {
            continue;
        }
        let Some(mask) = field("modmask:").and_then(|m| m.parse::<u32>().ok()) else {
            continue;
        };
        for (slot, wanted) in MODMASKS.iter().enumerate() {
            if mask == *wanted && found[slot].is_none() {
                found[slot] = Some(field("description:").unwrap_or_default());
            }
        }
    }
    found
}

fn current_descriptions() -> Option<[Option<String>; 2]> {
    let out = Command::new("hyprctl").arg("binds").stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(descriptions(&String::from_utf8_lossy(&out.stdout)))
}

fn were_line(before: Option<[Option<String>; 2]>) -> String {
    let Some(before) = before else {
        return "what they ran before could not be read because hyprctl is not reachable from here".to_string();
    };
    let name = |d: &Option<String>| d.as_deref().map(|s| format!("\"{}\"", s)).unwrap_or_else(|| "nothing".to_string());
    format!("were {} and {}", name(&before[0]), name(&before[1]))
}

fn reload() -> bool {
    Command::new("hyprctl")
        .arg("reload")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// None when hyprctl cannot be reached, otherwise what configerrors printed, which is empty when the config loads.
fn configerrors() -> Option<String> {
    let out = Command::new("hyprctl").arg("configerrors").stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEYS: [&str; 2] = ["SUPER + SHIFT + F", "SUPER + ALT + SHIFT + F"];

    #[test]
    fn the_float_block_asks_for_omarchys_own_tag_and_names_only_the_picker() {
        let b = float_block();
        assert!(b.contains("o.window(\"com.thisisgm.flea.picker\", { tag = \"+floating-window\" })"));
        // The window's own app id is a prefix of the picker's, so a rule naming it would take both.
        assert!(!b.contains("\"com.thisisgm.flea\""));
        assert_eq!(b.matches("o.window(").count(), 1);
        // Its own markers, so flea --picker off cannot take flea --default's block with it.
        assert!(b.contains(FLOAT_BEGIN) && b.contains(FLOAT_END) && !b.contains(BEGIN));
    }

    #[test]
    fn the_two_blocks_are_removed_one_at_a_time() {
        let both = with_block(&appended("-- user\n", &float_block()));
        let without_float = cut(&both, FLOAT_BEGIN, FLOAT_END).expect("float cut").expect("the float block is there");
        assert!(!without_float.contains(FLOAT_BEGIN) && without_float.contains(BEGIN));
        let without_keys = without_block(&both).expect("keys cut").expect("the keys block is there");
        assert!(without_keys.contains(FLOAT_BEGIN) && !without_keys.contains(BEGIN));
    }

    #[test]
    fn the_block_unbinds_each_key_before_it_binds_it_and_touches_no_other() {
        let b = block();
        for key in KEYS {
            let unbind = b.find(&format!("hl.unbind(\"{}\")", key)).expect("unbind");
            let bind = b.find(&format!("o.bind(\"{}\", ", key)).expect("bind");
            assert!(unbind < bind, "{} is bound before it is unbound", key);
        }
        assert_eq!(b.matches("hl.unbind(").count(), 2);
        assert_eq!(b.matches("o.bind(").count(), 2);
        // The same launcher Omarchy's own { launch = } bindings use, and the cwd one reads the terminal's directory.
        assert!(b.contains("{ launch = 'flea --gui' }"));
        assert!(b.contains("{ launch = 'flea --gui \"$(omarchy-cmd-terminal-cwd)\"' }"));
    }

    #[test]
    fn adding_then_removing_the_block_is_the_identity() {
        for text in ["", "a\n", "-- a comment\no.bind(\"SUPER + X\", nil, \"x\")\n"] {
            let added = with_block(text);
            assert!(has_block(&added));
            assert_eq!(without_block(&added).expect("well formed"), Some(text.to_string()), "round trip of {:?}", text);
        }
        // A file with no final newline gains one, and nothing else.
        assert_eq!(without_block(&with_block("a")).expect("well formed"), Some("a\n".to_string()));
    }

    #[test]
    fn a_line_the_user_added_after_the_block_survives_its_removal() {
        let text = format!("{}o.bind(\"SUPER + Y\", nil, \"y\")\n", with_block("a\n"));
        assert_eq!(without_block(&text).expect("well formed"), Some("a\no.bind(\"SUPER + Y\", nil, \"y\")\n".to_string()));
    }

    #[test]
    fn a_file_without_the_block_has_nothing_to_remove() {
        assert!(!has_block("a\n"));
        assert_eq!(without_block("a\n").expect("well formed"), None);
        // The marker counts only at the start of a line.
        let quoted = format!("-- do not write {} here\n", block().lines().nth(1).expect("begin"));
        assert!(!has_block(&quoted));
    }

    #[test]
    fn a_begin_marker_with_no_end_marker_is_refused() {
        let begin = block().lines().nth(1).expect("begin").to_string();
        assert!(without_block(&format!("a\n{}\nhl.unbind(\"SUPER + SHIFT + F\")\n", begin)).is_err());
    }

    // Read off this box's hyprctl binds, with SUPER + F's fullscreen record as the distractor.
    const BINDS_TEXT: &str = "bindd\n\tmodmask: 64\n\tsubmap: \n\tkey: F\n\tkeycode: 0\n\tcatchall: false\n\tdescription: Full screen\n\tdispatcher: __lua\n\targ: 27\n\nbindd\n\tmodmask: 65\n\tsubmap: \n\tkey: F\n\tkeycode: 0\n\tcatchall: false\n\tdescription: File manager\n\tdispatcher: __lua\n\targ: 272\n\nbindd\n\tmodmask: 73\n\tsubmap: \n\tkey: F\n\tkeycode: 0\n\tcatchall: false\n\tdescription: File manager (cwd)\n\tdispatcher: __lua\n\targ: 274\n\n";

    #[test]
    fn descriptions_are_read_off_hyprctl_binds_by_chord() {
        assert_eq!(descriptions(BINDS_TEXT), [Some("File manager".to_string()), Some("File manager (cwd)".to_string())]);
        assert_eq!(descriptions("bindd\n\tmodmask: 65\n\tkey: G\n\tdescription: Other\n\n"), [None, None]);
        assert_eq!(descriptions(""), [None, None]);
    }
}
