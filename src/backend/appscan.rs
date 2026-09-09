// The desktop's installed applications, scanned the way gio resolves ids: one recursive walk per
// applications dir on the XDG ladder, with the same should_show rules its own AppChooser applies.
// The Open with flyout asks gio which apps serve one type; the dialog lists what is installed
// whether this type names it or not, so the scan is what the dialog's whole list is made of.
use crate::backend::opsreq::OpMsg;
use crate::backend::thumbspec::is_runnable;
use crate::json::escape;
use crate::userfile;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// One installed, launchable, showable application: the display name its entry carries, the file
// `gio launch` takes, the id `gio mime <type> <id>` takes when the operator asks for always, and
// the Icon= the dialog's rows draw, empty when the entry wrote none.
pub struct AppEntry {
    pub name: String,
    pub path: String,
    pub id: String,
    pub icon: String,
}

// The applications dirs the id resolution scans, user data first. This is g_get_user_data_dir and
// g_get_system_data_dirs in the order gio resolves ids in, which is the same ladder userfile.rs
// already walks for install proofs.
pub(crate) fn applications_dirs() -> Vec<PathBuf> {
    userfile::data_dirs().into_iter().map(|d| d.join("applications")).collect()
}

// The id→file map gio builds: one recursive scan per applications dir, where a subdirectory's name
// becomes a "name-" prefix on every .desktop it holds, so applications/kde4/konsole.desktop is the
// file for the id kde4-konsole.desktop. Across dirs the first dir carrying the id wins, which is
// gio's own per-dir tables walked in ladder order; within one dir the scan's own later entry wins,
// which is g_hash_table_insert's rule inside one table.
pub(crate) fn table_of(dirs: &[PathBuf]) -> HashMap<String, PathBuf> {
    let mut table: HashMap<String, PathBuf> = HashMap::new();
    for dir in dirs {
        scan_dir(dir, "", &mut table, SCAN_DEPTH);
    }
    table
}

// glib's own scan has no bound; this one stops here, because a symlinked loop inside an
// applications dir would otherwise recurse until the stack did, and sixteen levels is deeper than
// any applications tree on any box this ships to.
const SCAN_DEPTH: u8 = 16;

fn scan_dir(dir: &Path, prefix: &str, table: &mut HashMap<String, PathBuf>, depth: u8) {
    if depth == 0 {
        return;
    }
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for item in read.flatten() {
        let path = item.path();
        let name = item.file_name().to_string_lossy().to_string();
        if name.ends_with(".desktop") {
            // Only a dir the id is not already in inserts, because a later dir in the ladder must
            // not take a file the earlier one already owns; within one dir the scan's own later
            // entry then wins, which is g_hash_table_insert's rule inside one table.
            table.entry(format!("{prefix}{name}")).or_insert(path);
        } else if std::fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false) {
            scan_dir(&path, &format!("{prefix}{name}-"), table, depth - 1);
        }
    }
}

// The Name= of the desktop entry's own group, untranslated, which is the name LC_ALL=C gio shows
// too. Reading stops at the next group header, so a [Desktop Action]'s own Name never wins.
pub(crate) fn entry_name(text: &str) -> Option<String> {
    facts_of(text).name
}

// The facts the showability rule reads, out of the [Desktop Entry] group alone. A key after the
// first group header is an action's, and no action's key can make the entry itself launchable.
pub(crate) struct Facts {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub type_: Option<String>,
    pub hidden: bool,
    pub no_display: bool,
    pub try_exec: Option<String>,
    pub exec: Option<String>,
    pub only_show_in: Option<Vec<String>>,
    pub not_show_in: Option<Vec<String>>,
}

pub(crate) fn facts_of(text: &str) -> Facts {
    let mut facts = Facts {
        name: None, icon: None, type_: None, hidden: false, no_display: false,
        try_exec: None, exec: None, only_show_in: None, not_show_in: None,
    };
    let mut in_entry = false;
    for line in text.lines() {
        if line.starts_with('[') {
            if in_entry {
                break;
            }
            in_entry = line.trim() == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let (key, value) = match line.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => continue,
        };
        match key {
            "Name" if facts.name.is_none() => facts.name = Some(value.to_string()),
            "Icon" if facts.icon.is_none() => facts.icon = Some(value.to_string()),
            "Type" => facts.type_ = Some(value.to_string()),
            "Hidden" => facts.hidden = value == "true",
            "NoDisplay" => facts.no_display = value == "true",
            "TryExec" => facts.try_exec = Some(value.to_string()),
            "Exec" => facts.exec = Some(value.to_string()),
            "OnlyShowIn" => facts.only_show_in = Some(split_names(value)),
            "NotShowIn" => facts.not_show_in = Some(split_names(value)),
            _ => {}
        }
    }
    facts
}

fn split_names(value: &str) -> Vec<String> {
    value.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

// The one desktop environment this rule is judged against, the way g_app_info_should_show reads it:
// every colon-separated name of XDG_CURRENT_DESKTOP, empty when the session does not say. An empty
// ladder can satisfy no OnlyShowIn and is refused by none of NotShowIn, which is what gio answers.
fn current_desktops() -> Vec<String> {
    std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default()
        .split(':').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

// g_app_info_should_show for the facts the scan already read: an entry that launches, and that no
// key it wrote itself asks off every application menu. Type is optional in the spec and missing
// means Application, the same reading g_desktop_app_info_new applies.
pub(crate) fn should_show(facts: &Facts) -> bool {
    if facts.hidden || facts.no_display || facts.name.is_none() || facts.exec.is_none() {
        return false;
    }
    if let Some(t) = &facts.type_ {
        if t != "Application" {
            return false;
        }
    }
    if let Some(program) = &facts.try_exec {
        if !is_runnable(program) {
            return false;
        }
    }
    let desktops = current_desktops();
    if let Some(only) = &facts.only_show_in {
        if !only.iter().any(|name| desktops.iter().any(|d| d == name)) {
            return false;
        }
    }
    if let Some(not) = &facts.not_show_in {
        if not.iter().any(|name| desktops.iter().any(|d| d == name)) {
            return false;
        }
    }
    true
}

// One entry per installed, showable application, sorted by its own Name. The order is a decision
// of this table and not of gio's, whose registered order is a per-type judgement the dialog is not
// making; a lowercase compare keeps apple beside Apple the way the listing's own order does.
pub fn all() -> Vec<AppEntry> {
    let table = table_of(&applications_dirs());
    let mut out: Vec<AppEntry> = Vec::new();
    for (id, path) in &table {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let facts = facts_of(&text);
        if !should_show(&facts) {
            continue;
        }
        let name = facts.name.unwrap_or_else(|| id.trim_end_matches(".desktop").to_string());
        out.push(AppEntry {
            name,
            path: path.to_string_lossy().to_string(),
            id: id.clone(),
            icon: facts.icon.unwrap_or_default(),
        });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
    out
}

// Sample output: {"t":"applications","apps":[{"name":"Image Viewer","path":"/usr/share/applications/org.gnome.eog.desktop","id":"org.gnome.eog.desktop"}],"ms":1.234}
pub fn applications_line(apps: &[AppEntry], ms: f64) -> String {
    let apps: Vec<String> = apps
        .iter()
        .map(|a| {
            format!(
                r#"{{"name":"{}","path":"{}","id":"{}","icon":"{}"}}"#,
                escape(&a.name),
                escape(&a.path),
                escape(&a.id),
                escape(&a.icon)
            )
        })
        .collect();
    format!(r#"{{"t":"applications","apps":[{}],"ms":{:.3}}}"#, apps.join(","), ms)
}

// Answers on a thread, the way the registered handlers do: the scan reads every entry on the box
// before it answers, and the loop waits on nothing.
pub fn spawn_all(tx: std::sync::mpsc::Sender<OpMsg>) {
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let apps = all();
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        let _ = tx.send(OpMsg::Applications { line: applications_line(&apps, ms) });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    fn entry(body: impl AsRef<str>) -> Facts {
        facts_of(body.as_ref())
    }

    #[test]
    fn the_facts_read_the_entry_group_and_stop_at_the_first_other_group() {
        let f = entry(
            "[Desktop Entry]\nName=Probe\nExec=true %f\nNoDisplay=false\n\n[Desktop Action one]\nName=Action\nNoDisplay=true\n",
        );
        assert_eq!(f.name.as_deref(), Some("Probe"));
        assert_eq!(f.exec.as_deref(), Some("true %f"));
        assert!(!f.no_display, "an action's NoDisplay never reaches the entry");
    }

    #[test]
    fn a_group_that_is_not_the_entry_declares_nothing() {
        let f = entry("[X-Other]\nName=Not an entry\nExec=true %f\n");
        assert_eq!(f.name, None);
        assert_eq!(f.exec, None);
    }

    #[test]
    fn the_show_rules_are_gios_own() {
        // Type is optional and missing means Application, the same reading g_desktop_app_info applies.
        assert!(should_show(&entry("[Desktop Entry]\nName=P\nExec=true %f\n")));
        assert!(should_show(&entry("[Desktop Entry]\nType=Application\nName=P\nExec=true %f\n")));
        assert!(!should_show(&entry("[Desktop Entry]\nType=Link\nName=P\nExec=true %f\n")));
        assert!(!should_show(&entry("[Desktop Entry]\nHidden=true\nName=P\nExec=true %f\n")));
        assert!(!should_show(&entry("[Desktop Entry]\nNoDisplay=true\nName=P\nExec=true %f\n")));
        assert!(!should_show(&entry("[Desktop Entry]\nExec=true %f\n")));
        assert!(!should_show(&entry("[Desktop Entry]\nName=P\n")));
        // A program TryExec names must be runnable, or the entry is a row nothing could ever start.
        assert!(!should_show(&entry("[Desktop Entry]\nName=P\nExec=true %f\nTryExec=/flea/no-such-bin\n")));
        assert!(should_show(&entry("[Desktop Entry]\nName=P\nExec=true %f\nTryExec=/bin/sh\n")));
    }

    #[test]
    fn only_show_in_and_not_show_in_are_judged_against_the_session() {
        let plain = "[Desktop Entry]\nName=P\nExec=true %f\n";
        // A session this fixture does name is shown and one it does not is hidden, each rule alone.
        std::env::set_var("XDG_CURRENT_DESKTOP", "FLEATEST");
        assert!(should_show(&entry(plain.to_string() + "OnlyShowIn=FLEATEST;GNOME;\n")));
        assert!(!should_show(&entry(plain.to_string() + "OnlyShowIn=GNOME;\n")));
        assert!(should_show(&entry(plain.to_string() + "NotShowIn=GNOME;\n")));
        assert!(!should_show(&entry(plain.to_string() + "NotShowIn=FLEATEST;\n")));
        // Every colon-separated name of the ladder is checked, which is the spec's own shape.
        std::env::set_var("XDG_CURRENT_DESKTOP", "FLEATEST:OTHER");
        assert!(should_show(&entry(plain.to_string() + "OnlyShowIn=OTHER;\n")));
        assert!(!should_show(&entry(plain.to_string() + "NotShowIn=OTHER;\n")));
        std::env::remove_var("XDG_CURRENT_DESKTOP");
        // An unset session satisfies no OnlyShowIn and is refused by none of NotShowIn.
        assert!(!should_show(&entry(plain.to_string() + "OnlyShowIn=GNOME;\n")));
        assert!(should_show(&entry(plain.to_string() + "NotShowIn=GNOME;\n")));
    }

    #[test]
    fn the_all_list_covers_the_ladder_once_and_sorts_by_its_own_name() {
        let d = TestDir::new("appscanall");
        d.dir("user/applications");
        d.dir("sys/applications");
        std::fs::write(d.join("user/applications/zview.desktop"), "[Desktop Entry]\nName=Zed View\nExec=z %f\nIcon=utilities-terminal\n").unwrap();
        std::fs::write(d.join("sys/applications/aview.desktop"), "[Desktop Entry]\nName=Apple Viewer\nExec=a %f\n").unwrap();
        // The same id in a later dir of the ladder is the earlier dir's file, the resolution rule.
        std::fs::write(d.join("sys/applications/zview.desktop"), "[Desktop Entry]\nName=Sys Copy\nExec=z2 %f\n").unwrap();
        // A name-prefixed subdirectory resolves its files to prefixed ids, the same as gio's scan.
        d.dir("sys/applications/kde4");
        std::fs::write(d.join("sys/applications/kde4/konsole.desktop"), "[Desktop Entry]\nName=Konsole\nExec=k %f\n").unwrap();
        let dirs = vec![d.join("user/applications"), d.join("sys/applications")];
        let table = table_of(&dirs);
        let mut apps: Vec<AppEntry> = Vec::new();
        for (id, path) in &table {
            let text = std::fs::read_to_string(path).unwrap();
            let facts = facts_of(&text);
            if !should_show(&facts) {
                continue;
            }
            apps.push(AppEntry {
                name: facts.name.unwrap_or_else(|| id.trim_end_matches(".desktop").to_string()),
                path: path.to_string_lossy().to_string(),
                id: id.clone(),
                icon: facts.icon.unwrap_or_default(),
            });
        }
        apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
        let names: Vec<&str> = apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Apple Viewer", "Konsole", "Zed View"], "ladder order wins and the sort is by name");
        assert_eq!(apps[2].id, "zview.desktop");
        assert!(apps[2].path.ends_with("user/applications/zview.desktop"));
        assert_eq!(apps[2].icon, "utilities-terminal", "the entry's own Icon= rides the row");
        assert_eq!(apps[0].icon, "", "an entry that wrote no Icon= answers an empty one");
        assert_eq!(apps[1].id, "kde4-konsole.desktop");
    }

    #[test]
    fn an_unreadable_entry_drops_out_rather_than_listing_a_nameless_row() {
        let d = TestDir::new("appscanunreadable");
        d.dir("share/applications");
        let path = d.join("share/applications/broken.desktop");
        std::fs::write(&path, "[Desktop Entry]\nName=Broken\nExec=b %f\n").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o000);
        std::fs::set_permissions(&path, perms).unwrap();
        let table = table_of(&[d.join("share/applications")]);
        assert!(std::fs::read_to_string(&table["broken.desktop"]).is_err());
    }

    #[test]
    fn the_line_escapes_every_string_field_like_the_registered_one() {
        let apps = vec![
            AppEntry { name: "say \"hi\"".to_string(), path: "/tmp/a \"b\".desktop".to_string(), id: "a.desktop".to_string(), icon: "say \"hi\"".to_string() },
            AppEntry { name: "Viewer".to_string(), path: "/usr/share/applications/v.desktop".to_string(), id: "v.desktop".to_string(), icon: String::new() },
        ];
        let line = applications_line(&apps, 1.5);
        assert!(line.starts_with(r#"{"t":"applications","apps":["#), "{}", line);
        assert!(line.contains(r#"{"name":"say \"hi\"","path":"/tmp/a \"b\".desktop","id":"a.desktop","icon":"say \"hi\""}"#), "{}", line);
        assert!(line.ends_with(r#"{"name":"Viewer","path":"/usr/share/applications/v.desktop","id":"v.desktop","icon":""}],"ms":1.500}"#), "{}", line);
        assert_eq!(applications_line(&[], 0.0), r#"{"t":"applications","apps":[],"ms":0.000}"#);
    }
}
