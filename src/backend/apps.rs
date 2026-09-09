// The desktop's own applications database, read through gio the way the plain Open handoff reaches
// it. Flea decides nothing about which programs open a type: that judgement is gio's, and this
// module only asks it and then resolves the ids it names to launchable desktop entry files. Like
// thumb and meta, a type is looked at only when a client asked for one row.
use crate::backend::opsreq::OpMsg;
use crate::json::escape;
use crate::userfile;
use std::collections::HashMap;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

// One application one row can be opened with: the desktop entry file gio launch takes, and the
// display name its own Name= carries.
pub struct App {
    pub label: String,
    pub path: String,
}

// gio mime answers in about 17 ms warm here; this bounds a wedged gio the way every other child
// this backend spawns is bounded, and a wedged one answers an empty list rather than stranding the
// row the client asked about.
const HANDLERS_TIMEOUT: Duration = Duration::from_secs(10);
const SIGKILL: i32 = 9;

// std offers no way to kill a child from another thread without owning it; the same one-line
// declaration metareq.rs already carries, because this crate takes no crates.
extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

// The applications dirs the id resolution scans, user data first. This is g_get_user_data_dir and
// g_get_system_data_dirs in the order gio resolves ids in, which is the same ladder userfile.rs
// already walks for install proofs.
fn applications_dirs() -> Vec<PathBuf> {
    userfile::data_dirs().into_iter().map(|d| d.join("applications")).collect()
}

// The one structural fact the parse reads: every app id `LC_ALL=C gio mime` prints arrives
// tab-indented, in both of its sections, and the recommended section is a subset of the registered
// one, so first-occurrence dedupe gives the registered list in gio's own precedence order. No
// header wording is read at all, so no locale can change the answer — the pin gio's own output
// needed for the network shares is not needed here by construction.
pub fn parse_registered(output: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in output.lines() {
        let id = match line.strip_prefix('\t') {
            Some(id) => id.trim(),
            None => continue,
        };
        if id.is_empty() || out.iter().any(|seen| seen == id) {
            continue;
        }
        out.push(id.to_string());
    }
    out
}

// The id→file map gio builds: one recursive scan per applications dir, where a subdirectory's name
// becomes a "name-" prefix on every .desktop it holds, so applications/kde4/konsole.desktop is the
// file for the id kde4-konsole.desktop. Within one dir the last scan wins, and across dirs the
// first dir carrying the id wins, which is g_hash_table_insert inside g_dir_read_name order.
// The id→file map gio builds: one recursive scan per applications dir, where a subdirectory's name
// becomes a "name-" prefix on every .desktop it holds, so applications/kde4/konsole.desktop is the
// file for the id kde4-konsole.desktop. Across dirs the first dir carrying the id wins, which is
// gio's own per-dir tables walked in ladder order.
pub fn table_of(dirs: &[PathBuf]) -> HashMap<String, PathBuf> {
    let mut table: HashMap<String, PathBuf> = HashMap::new();
    for dir in dirs {
        scan_dir(dir, "", &mut table, SCAN_DEPTH);
    }
    table
}

// One scan serves every id, so a type with eleven handlers costs the scan once, not eleven times.
fn from_table(id: &str, table: &HashMap<String, PathBuf>) -> Option<(PathBuf, String)> {
    let path = table.get(id)?;
    let name = std::fs::read_to_string(path).ok().and_then(|text| entry_name(&text));
    Some((path.clone(), name.unwrap_or_else(|| id.trim_end_matches(".desktop").to_string())))
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
fn entry_name(text: &str) -> Option<String> {
    let mut in_entry = false;
    for line in text.lines() {
        if line.starts_with('[') {
            if in_entry {
                return None;
            }
            in_entry = line.trim() == "[Desktop Entry]";
            continue;
        }
        if in_entry {
            if let Some(value) = line.strip_prefix("Name=") {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

// One launchable application per id gio named, in gio's order; an id whose desktop file has
// vanished since the listing is dropped, which is what gio's own resolution does too.
pub fn list(mime_type: &str) -> Vec<App> {
    let output = match gio_mime(mime_type) {
        Some(output) => output,
        None => return Vec::new(),
    };
    let table = table_of(&applications_dirs());
    parse_registered(&output)
        .into_iter()
        .filter_map(|id| {
            from_table(&id, &table).map(|(path, label)| App {
                label,
                path: path.to_string_lossy().to_string(),
            })
        })
        .collect()
}

// gio is the desktop's registry, not a decoder over untrusted bytes: the only input is a MIME type
// from the shared globs2 table, and plain Open already trusts this same database. No sandbox, like
// every other gio call this codebase makes; the deadline is the only guard a child needs here.
fn gio_mime(mime_type: &str) -> Option<String> {
    let child = std::process::Command::new("gio")
        .arg("mime")
        .arg(mime_type)
        // The parse reads only tab-indented lines, so the locale cannot change the answer, but the
        // pin costs nothing and keeps the whole output byte-stable while it is being read.
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .ok()?;
    let pid = child.id() as i32;
    let done = Arc::new(AtomicBool::new(false));
    let watchdog = {
        let done = Arc::clone(&done);
        std::thread::spawn(move || {
            let step = Duration::from_millis(50);
            let mut waited = Duration::ZERO;
            while waited < HANDLERS_TIMEOUT {
                if done.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::sleep(step);
                waited += step;
            }
            if !done.load(Ordering::Relaxed) {
                unsafe { kill(pid, SIGKILL) };
            }
        })
    };
    let output = child.wait_with_output().ok();
    done.store(true, Ordering::Relaxed);
    let _ = watchdog.join();
    match output {
        Some(o) if o.status.success() => Some(String::from_utf8_lossy(&o.stdout).into_owned()),
        _ => None,
    }
}

// Sample output: {"t":"handlers","row":4,"apps":[{"name":"Image Viewer","path":"/usr/share/applications/org.gnome.eog.desktop"}],"ms":1.234}
pub fn handlers_line(row: usize, apps: &[App], ms: f64) -> String {
    let apps: Vec<String> = apps
        .iter()
        .map(|a| format!(r#"{{"name":"{}","path":"{}"}}"#, escape(&a.label), escape(&a.path)))
        .collect();
    format!(r#"{{"t":"handlers","row":{},"apps":[{}],"ms":{:.3}}}"#, row, apps.join(","), ms)
}

// Answers on a thread, the way meta does: the loop waits on nothing, and a gio mime the box
// wedged must not take the listing with it.
pub fn spawn(row: usize, mime_type: String, tx: std::sync::mpsc::Sender<OpMsg>) {
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let apps = list(&mime_type);
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        let _ = tx.send(OpMsg::Handlers { line: handlers_line(row, &apps, ms) });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    // The two shapes LC_ALL=C gio mime answers with on this box, captured from the real tool: a
    // default, and a type with none. The ids are what the tab-indented lines carry, whatever the
    // section headers above them say.
    const WITH_DEFAULT: &str = concat!(
        "Default application for \u{201c}image/png\u{201d}: imv.desktop\n",
        "Registered applications:\n",
        "\timv.desktop\n",
        "\tchromium.desktop\n",
        "\tcom.github.PintaProject.Pinta.desktop\n",
        "Recommended applications:\n",
        "\timv.desktop\n",
        "\tchromium.desktop\n",
    );
    const NO_DEFAULT: &str = "No default applications for \u{201c}application/x-nonexistent\u{201d}\n";

    #[test]
    fn the_registered_list_is_the_tab_indented_ids_in_gios_own_order() {
        assert_eq!(parse_registered(WITH_DEFAULT), ["imv.desktop", "chromium.desktop", "com.github.PintaProject.Pinta.desktop"]);
        assert_eq!(parse_registered(NO_DEFAULT), Vec::<String>::new());
    }

    #[test]
    fn a_recommended_section_repeats_registered_ids_and_dedupe_keeps_the_first_order() {
        assert_eq!(parse_registered("Registered applications:\n\tb.desktop\n\ta.desktop\nRecommended applications:\n\tb.desktop\n"), ["b.desktop", "a.desktop"]);
    }

    #[test]
    fn an_empty_output_and_blank_ids_answer_nothing() {
        assert_eq!(parse_registered(""), Vec::<String>::new());
        assert_eq!(parse_registered("Registered applications:\n\t\n\tb.desktop\n"), ["b.desktop"]);
    }

    #[test]
    fn a_flat_id_resolves_to_the_file_its_name_says() {
        let d = TestDir::new("appsflat");
        d.dir("share/applications");
        std::fs::write(d.join("share/applications/probe.desktop"), "[Desktop Entry]\nName=Probe\nExec=true %f\n").unwrap();
        let (path, label) = from_table("probe.desktop", &table_of(&[d.join("share/applications")])).expect("the flat id resolves");
        assert!(path.ends_with("probe.desktop"), "{}", path.display());
        assert_eq!(label, "Probe");
    }

    #[test]
    fn a_subdirectorys_files_carry_their_dir_as_a_prefix_on_the_id() {
        let d = TestDir::new("appssub");
        d.dir("share/applications/kde4");
        std::fs::write(d.join("share/applications/kde4/konsole.desktop"), "[Desktop Entry]\nName=Konsole\n").unwrap();
        let (path, label) = from_table("kde4-konsole.desktop", &table_of(&[d.join("share/applications")])).expect("the prefixed id resolves");
        assert!(path.ends_with("kde4/konsole.desktop"), "{}", path.display());
        assert_eq!(label, "Konsole");
    }

    #[test]
    fn the_first_dir_carrying_the_id_wins_and_a_missing_file_drops_the_app() {
        let d = TestDir::new("appsorder");
        d.dir("user/applications");
        d.dir("system/applications");
        std::fs::write(d.join("user/applications/both.desktop"), "[Desktop Entry]\nName=User one\n").unwrap();
        std::fs::write(d.join("system/applications/both.desktop"), "[Desktop Entry]\nName=System two\n").unwrap();
        let (_, label) = from_table("both.desktop", &table_of(&[d.join("user/applications"), d.join("system/applications")])).expect("resolves");
        assert_eq!(label, "User one", "the user data dir outranks the system one");
        assert!(from_table("gone.desktop", &table_of(&[d.join("user/applications")])).is_none(), "an id with no file answers nothing");
    }

    #[test]
    fn an_entry_with_no_name_falls_back_to_its_id_stem() {
        let d = TestDir::new("appsnolabel");
        d.dir("share/applications");
        std::fs::write(d.join("share/applications/bare.desktop"), "[Desktop Entry]\nExec=true %f\n").unwrap();
        let (_, label) = from_table("bare.desktop", &table_of(&[d.join("share/applications")])).expect("resolves");
        assert_eq!(label, "bare");
    }

    #[test]
    fn a_desktop_actions_own_name_never_wins_the_entrys() {
        let d = TestDir::new("appsactions");
        d.dir("share/applications");
        std::fs::write(
            d.join("share/applications/acted.desktop"),
            "[Desktop Entry]\nName=The entry\nExec=true %f\n\n[Desktop Action one]\nName=The action\nExec=true\n",
        )
        .unwrap();
        let (_, label) = from_table("acted.desktop", &table_of(&[d.join("share/applications")])).expect("resolves");
        assert_eq!(label, "The entry");
    }

    #[test]
    fn the_line_escapes_a_name_and_a_path_like_every_other_string_on_this_wire() {
        let apps = vec![
            App { label: "say \"hi\"".to_string(), path: "/tmp/say \"hi\".desktop".to_string() },
            App { label: "Viewer".to_string(), path: "/usr/share/applications/v.desktop".to_string() },
        ];
        let line = handlers_line(2, &apps, 1.5);
        assert!(line.starts_with(r#"{"t":"handlers","row":2,"apps":["#), "{}", line);
        assert!(line.contains(r#"{"name":"say \"hi\"","path":"/tmp/say \"hi\".desktop"}"#), "{}", line);
        assert!(line.ends_with(r#"{"name":"Viewer","path":"/usr/share/applications/v.desktop"}],"ms":1.500}"#), "{}", line);
        assert_eq!(handlers_line(0, &[], 0.0), r#"{"t":"handlers","row":0,"apps":[],"ms":0.000}"#);
    }

    #[test]
    fn a_probe_type_with_no_registered_app_answers_an_empty_list_without_dying() {
        let apps = list("application/x-nonexistent-flea-probe");
        assert!(apps.is_empty());
    }
}

