// Resolving a desktop id to the file gio launch needs, through the XDG data dirs.
use std::path::PathBuf;

// The most dashes that split independently: 2^8 readings is 256 probes, where one more dash each doubles them.
const SPLIT_DASHES: usize = 8;

// Every dash to slash reading of the id, verbatim first; a dash is one subdir or one of the name, so resolve tries each split. Past SPLIT_DASHES only the verbatim and the all-slash reading remain.
pub(crate) fn spellings(id: &str) -> Vec<String> {
    if id.matches('-').count() > SPLIT_DASHES {
        return vec![format!("applications/{}", id), format!("applications/{}", id.replace('-', "/"))];
    }
    let mut forms = vec![id.to_string()];
    for (at, _) in id.match_indices('-') {
        let n = forms.len();
        for i in 0..n {
            // Both spellings are one byte, so an earlier split never moves a later dash.
            let mut split = forms[i].clone();
            split.replace_range(at..at + 1, "/");
            forms.push(split);
        }
    }
    forms.into_iter().map(|form| format!("applications/{}", form)).collect()
}

// Sample id: "nvim.desktop" answers its applications/ file; "org-gnome-Nautilus.desktop" also tries the dash to slash subdir spelling, the way the data dirs themselves lay vendor files out.
pub fn resolve(id: &str) -> Option<PathBuf> {
    if id.is_empty() || id.contains('/') || id.contains('\0') {
        return None;
    }
    spellings(id).into_iter().filter_map(|s| crate::userfile::data_file(&s)).next()
}

// The text editor the desktop names for text/plain, or "" when it names none.
pub fn text_editor() -> String {
    let out = crate::backend::locale::command("xdg-mime")
        .arg("query")
        .arg("default")
        .arg("text/plain")
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => String::new(),
    }
}

// Sample line: "Default application for 'text/plain': nvim.desktop" names nvim.desktop; "No default applications for 'text/plain'", a translated line and an id without ".desktop" name none.
pub(crate) fn default_id(line: &str) -> &str {
    line.strip_prefix("Default application for ").and_then(|rest| rest.rsplit_once(':')).map(|(_, id)| id.trim()).filter(|id| id.ends_with(".desktop")).unwrap_or_default()
}

// Sample output: "Default application for 'text/plain': nvim.desktop\nRegistered applications:\n\tnvim.desktop"; only the first line can name the default.
pub fn handler_of(output: &str) -> String {
    let line = output.lines().next().unwrap_or_default().trim();
    default_id(line).to_string()
}

// Sample output: "Default application for 'x': a.desktop" runs gio, "No default ..." asks, and a failed lookup is claimed so a refused launch keeps its sentence.
pub fn default_handler_gio(mime: &str) -> String {
    let out = crate::backend::locale::command("gio")
        .arg("mime")
        .arg(mime)
        .output();
    match out {
        Ok(o) if o.status.success() => handler_of(&String::from_utf8_lossy(&o.stdout)),
        _ => String::from("claimed"),
    }
}

// The type the Open with dialog reads (gio info, symlinks followed), so the card and the dialog never name two; None when gio cannot say.
fn content_type(path: &std::path::Path) -> Option<String> {
    crate::backend::menu_registry::content_type(&Default::default(), path, &Default::default()).ok()
}

// True when no application claims the file, which opens the approved Open with dialog instead of an error; a lookup that cannot be had answers claimed, so a refused launch keeps the sentence it has always had.
pub fn unclaimed(path: &std::path::Path) -> bool {
    content_type(path).is_some_and(|mime| default_handler_gio(&mime).is_empty())
}

// The desktop's default for the file's own type, as its id and its entry's own Name, which the AppImage card offers as its first button; None leaves the card the board draws.
pub fn handler(path: &std::path::Path) -> Option<(String, String)> {
    let id = default_handler_gio(&content_type(path)?);
    named_entry(&id).map(|(_, name)| (id, name))
}

// An installed entry's own launchable file and Name=, never a spelling of its id; None when either cannot be read.
pub fn named_entry(id: &str) -> Option<(PathBuf, String)> {
    let app = crate::backend::menu_registry::resolve(id, &Default::default()).ok()?;
    let name = crate::backend::menu_registry::desktop_label(&app.path).ok().flatten().filter(|n| !n.trim().is_empty())?;
    Some((app.path, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The resolver never leaves the ladder, whatever the id holds.
    #[test]
    fn an_empty_relative_or_nul_id_resolves_to_nothing() {
        assert_eq!(resolve(""), None);
        assert_eq!(resolve("../skipped.desktop"), None);
        assert_eq!(resolve("a\0.desktop"), None);
    }

    // A missing id is None rather than a bare name, so no caller can hand gio one.
    #[test]
    fn a_missing_id_is_none_on_the_real_ladder() {
        assert_eq!(resolve("flea-definitely-not-installed-9f3c.desktop"), None);
    }

    // The spelling rule the resolver tries, verbatim first and every subdir split behind it.
    #[test]
    fn dashed_ids_try_the_subdir_spelling_behind_the_verbatim_one() {
        assert_eq!(spellings("nvim.desktop"), ["applications/nvim.desktop"]);
        assert_eq!(
            spellings("org-gnome-Nautilus.desktop"),
            [
                "applications/org-gnome-Nautilus.desktop",
                "applications/org/gnome-Nautilus.desktop",
                "applications/org-gnome/Nautilus.desktop",
                "applications/org/gnome/Nautilus.desktop"
            ]
        );
    }

    // A long id stays a bounded ladder: the cap's own count keeps every split, one dash more keeps the two plain readings.
    #[test]
    fn the_cap_edge_keeps_every_split_and_one_dash_more_keeps_two() {
        let dashed = |dashes: usize| format!("{}.desktop", vec!["a"; dashes + 1].join("-"));
        assert_eq!(spellings(&dashed(SPLIT_DASHES)).len(), 1 << SPLIT_DASHES);
        let past = dashed(SPLIT_DASHES + 1);
        assert_eq!(spellings(&past), [format!("applications/{}", past), format!("applications/{}", past.replace('-', "/"))]);
    }

    // A file name can hold over a hundred dashes, and 2^30 readings would never finish: the ladder stays at most 256, verbatim first, all-slash form present.
    #[test]
    fn a_thirty_dash_id_answers_at_most_256_spellings() {
        let id = format!("{}.desktop", vec!["a"; 31].join("-"));
        let out = spellings(&id);
        assert!(out.len() <= 256, "{} spellings", out.len());
        assert_eq!(out[0], format!("applications/{}", id));
        assert!(out.contains(&format!("applications/{}", id.replace('-', "/"))));
    }

    // A dash is one subdir or one of the name, so every split is tried, verbatim first.
    #[test]
    fn a_dash_inside_a_subdir_name_is_still_found() {
        assert_eq!(
            spellings("kde-org-kate.desktop"),
            [
                "applications/kde-org-kate.desktop",
                "applications/kde/org-kate.desktop",
                "applications/kde-org/kate.desktop",
                "applications/kde/org/kate.desktop"
            ]
        );
    }

    #[test]
    fn the_handler_line_names_the_desktop_id_or_nothing() {
        assert_eq!(handler_of("Default application for 'text/plain': nvim.desktop"), "nvim.desktop");
        assert_eq!(handler_of("No default applications for 'text/plain'"), "");
        assert_eq!(handler_of("junk"), "");
        assert_eq!(handler_of(""), "");
    }

    // An id gio prints without ".desktop" is no desktop id, so it names no handler.
    #[test]
    fn an_id_without_desktop_names_none() {
        assert_eq!(handler_of("Default application for 'x': foo"), "");
        assert_eq!(default_id("Default application for 'x': foo"), "");
    }

    #[test]
    fn the_registry_rows_behind_the_default_are_not_part_of_the_id() {
        assert_eq!(
            handler_of("Default application for \u{201c}application/vnd.appimage\u{201d}: zzflea-appimage.desktop\nNo registered applications\nNo recommended applications\n"),
            "zzflea-appimage.desktop");
        assert_eq!(
            handler_of("Default application for 'text/plain': org.gnome.TextEditor.desktop\n\torg.gnome.TextEditor.desktop\n"),
            "org.gnome.TextEditor.desktop");
        assert_eq!(handler_of("No default applications for 'application/vnd.appimage'\nNo registered applications\nNo recommended applications"), "");
    }

    #[test]
    fn translated_handler_lines_name_nothing() {
        assert_eq!(handler_of("Aplicaci\u{f3}n predeterminada para 'text/plain': nvim.desktop"), "");
        assert_eq!(handler_of("Standardanwendung f\u{fc}r 'text/plain': nvim.desktop"), "");
    }
}
