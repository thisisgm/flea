// COPY-SRC (0.3.10): the sentences the top-level files print, pinned as written, for the lines no function can be called to reach.
const SOURCES: [(&str, &str); 21] = [
    ("chooser.rs", include_str!("chooser.rs")),
    ("defaults.rs", include_str!("defaults.rs")),
    ("favourites.rs", include_str!("favourites.rs")),
    ("gui.rs", include_str!("gui.rs")),
    ("gui/launch.rs", include_str!("gui/launch.rs")),
    ("hyprkeys.rs", include_str!("hyprkeys.rs")),
    ("launchargs.rs", include_str!("launchargs.rs")),
    ("main.rs", include_str!("main.rs")),
    ("open.rs", include_str!("open.rs")),
    ("paths.rs", include_str!("paths.rs")),
    ("shelf.rs", include_str!("shelf.rs")),
    ("shelfcli.rs", include_str!("shelfcli.rs")),
    ("shelfdrag.rs", include_str!("shelfdrag.rs")),
    ("shelfops.rs", include_str!("shelfops.rs")),
    ("shelfplaces.rs", include_str!("shelfplaces.rs")),
    ("shelfundo.rs", include_str!("shelfundo.rs")),
    ("shelfzip.rs", include_str!("shelfzip.rs")),
    ("summon.rs", include_str!("summon.rs")),
    ("terminal.rs", include_str!("terminal.rs")),
    ("uistatecli.rs", include_str!("uistatecli.rs")),
    ("update.rs", include_str!("update.rs")),
];

// Each file's code without its test modules, whether they sit inline or in a file of their own, and without comment lines, so a comment may still say a retired word.
fn code_of(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut kept = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        let next = lines.get(at + 1).copied().unwrap_or("");
        if line == "#[cfg(test)]" && next.starts_with("mod ") && next.ends_with('{') {
            // An inline module runs to the closing brace in the first column.
            while at < lines.len() && lines[at] != "}" {
                at += 1;
            }
        } else if line == "#[cfg(test)]" && next.starts_with("#[path") {
            at += 2;
        } else if !line.trim_start().starts_with("//") {
            kept.push(line);
        }
        at += 1;
    }
    kept.join("\n")
}

fn code(file: &str) -> String {
    let (_, source) = SOURCES.iter().find(|(name, _)| *name == file).unwrap_or_else(|| panic!("{} is not a listed source", file));
    code_of(source)
}

// A sentence, the file that prints it and how many times, for the sites that have no seam to call.
const WRITTEN: [(&str, &str, usize); 35] = [
    ("chooser.rs", "is not installed in any portal folder, so the file chooser was not changed. Install the package first.", 1),
    ("chooser.rs", "{} has no parent folder", 1),
    ("chooser.rs", "{} could not be created: {}", 1),
    ("chooser.rs", "{} could not be read: {}", 3),
    ("chooser.rs", "{} could not be removed: {}", 1),
    ("chooser.rs", "The desktop is unknown, so Flea cannot tell whether this desktop reads {}. Run {} inside the desktop session.", 1),
    ("defaults.rs", "is not installed in any applications folder, so nothing was made the default. Install the package first.", 1),
    ("defaults.rs", "{}: skipped, {} is not installed in any data folder", 1),
    ("defaults.rs", "{}: Flea, written to {}, which is read before every system folder", 1),
    ("defaults.rs", "{}: Flea's registration and the folders it created removed from {}", 1),
    ("defaults.rs", "{}: Flea's registration removed from {}", 1),
    ("defaults.rs", "{} could not be read: {}", 4),
    ("defaults.rs", "{} could not be created: {}", 1),
    ("defaults.rs", "{} could not be removed: {}", 1),
    ("favourites.rs", "A favorite name needs visible text.", 2),
    ("favourites.rs", "Favorites changed in another window; try again.", 1),
    ("favourites.rs", "This favorite is damaged and cannot be renamed. Remove it instead.", 1),
    ("gui/launch.rs", "flea: the window could not start: {}: {error}", 2),
    ("hyprkeys.rs", "block appended to {}. Hyprland could not be reached, so run hyprctl reload inside the session.", 2),
    ("hyprkeys.rs", "block removed from {}. Hyprland could not be reached, so run hyprctl reload inside the session.", 1),
    ("launchargs.rs", "--tui and --gui cannot be used together", 1),
    ("open.rs", "flea: no application on this system opened that file", 1),
    ("paths.rs", "flea: Flea's interface files are missing (/usr/share/flea/ui/{}). Reinstall flea.", 1),
    ("shelfcli.rs", "flea: {} could not be shown: {}", 2),
    ("shelfops.rs", "flea: the transfer stopped before it finished", 1),
    ("shelfops.rs", "flea: omarchy-tailscale-send is not installed, so nothing was sent", 1),
    ("shelfops.rs", "flea: Tailscale is not installed, so there are no peers to show", 1),
    ("shelfops.rs", "flea: the cancel could not be written: {}", 1),
    ("shelfplaces.rs", "{} was not written, so the chooser answered nothing: {}", 2),
    ("shelfzip.rs", "flea: those items have no folder in common to archive them from", 1),
    ("shelfzip.rs", "flea: the staged archive stayed behind at {}: {}", 1),
    ("terminal.rs", "flea: that folder could not be opened in a terminal; check that it still exists", 1),
    ("uistatecli.rs", "flea: the shelf plugin was not installed ({})", 1),
    ("uistatecli.rs", "flea: the shelf plugin was not removed ({})", 1),
    ("update.rs", "pacman could not find the package that installed Flea", 1),
];

#[test]
fn every_rewritten_sentence_stands_in_its_file_as_often_as_it_is_printed() {
    for (file, sentence, times) in WRITTEN {
        let held = code(file).matches(sentence).count();
        assert_eq!(held, times, "{} should carry {:?} {} time(s)", file, sentence, times);
    }
}

// What these files said before the pass, as whole phrases; none may come back by a revert or a merge.
const RETIRED: [&str; 31] = [
    "no directory to write in",
    "no directory in common",
    "is the only directory those paths",
    "is not installed in any portal directory",
    "has no directory\"",
    "it is unknown whether",
    "is not installed in any applications directory",
    "is not installed in any data directory",
    "the data home is read before every system directory",
    "the directories it created went with it",
    "xdg-mime is not on PATH",
    "exited {}\"",
    "already reports a problem, fix that first",
    "the block broke the Hyprland config",
    "not reloaded because hyprctl could not be reached",
    "and it is where the key bindings go",
    "could not start the shell",
    "the shell config is missing",
    "mutually exclusive",
    "gio open refused",
    "could not describe the package",
    "vercmp could not compare",
    "favorite label needs visible text",
    "refresh before editing",
    "remove it explicitly",
    "the shelf plugin was not {} ({})",
    "this box has no",
    "the transfer engine stopped",
    "the shelf would not take the pile",
    "that directory could not be opened in a terminal",
    "the staged archive stayed behind at {} ({:?})",
];

#[test]
fn no_retired_sentence_comes_back() {
    for (file, _) in SOURCES {
        let held = code(file);
        for retired in RETIRED {
            assert!(!held.contains(retired), "{} still says {:?}", file, retired);
        }
    }
}

// The lines that print an ErrorKind in Debug form: a `{:?}` and a `.kind()` in one statement, wherever the format and its arguments break across lines.
fn debug_kinds(held: &str) -> Vec<String> {
    let lines: Vec<&str> = held.lines().collect();
    let mut found = Vec::new();
    for (at, line) in lines.iter().enumerate() {
        if !line.contains("{:?}") { continue; }
        // A statement ends at its semicolon, so read on to it, at most five lines.
        let mut statement = String::new();
        for next in lines.iter().skip(at).take(5) {
            statement.push_str(next);
            if next.trim_end().ends_with(';') || next.trim_end().ends_with('{') { break; }
        }
        if statement.contains("kind()") { found.push(line.trim().to_string()); }
    }
    found
}

// An ErrorKind printed in Debug form reads "PermissionDenied"; error::io_message says it in words.
#[test]
fn no_message_prints_an_error_kind_in_debug_form() {
    for (file, _) in SOURCES.iter().filter(|(file, _)| *file != "favourites.rs") {
        let lines = debug_kinds(&code(file));
        assert!(lines.is_empty(), "{} prints an error kind in Debug form: {:?}", file, lines);
    }
}

// The pre-pass shelfzip.rs shape put the format on one line and the kind on the next; the check reads across it.
#[test]
fn a_debug_kind_split_over_two_lines_is_still_caught() {
    let split = "return Err(format!(\"{} is the only folder ({:?})\",\n    dir.display(),\n    e.kind()));";
    assert_eq!(debug_kinds(split).len(), 1, "the format line is reported");
    assert!(debug_kinds("return Err(format!(\"{} is the only folder ({})\", dir.display(), io_message(&e)));").is_empty());
    assert!(debug_kinds("println!(\"{:?}\", name);\nlet k = e.kind();").is_empty(), "a kind in the next statement is not this one");
}
