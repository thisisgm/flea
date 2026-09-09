// The desktop's own applications database, read through gio the way the plain Open handoff reaches
// it. Flea decides nothing about which programs open a type: that judgement is gio's, and this
// module only asks it and then resolves the ids it names to launchable desktop entry files. Like
// thumb and meta, a type is looked at only when a client asked for one row. The installed-apps
// table this module consumes is appscan.rs's; here only the per-type question gio answers lives.
use crate::backend::appscan::{self, entry_name, table_of};
use crate::backend::opsreq::OpMsg;
use crate::backend::proto::error_line;
use crate::error::FleaError;
use crate::json::escape;
use std::collections::HashMap;
use std::io::Write;
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

// One scan serves every id, so a type with eleven handlers costs the scan once, not eleven times.
fn from_table(id: &str, table: &HashMap<String, PathBuf>) -> Option<(PathBuf, String)> {
    let path = table.get(id)?;
    let name = std::fs::read_to_string(path).ok().and_then(|text| entry_name(&text));
    Some((path.clone(), name.unwrap_or_else(|| id.trim_end_matches(".desktop").to_string())))
}

// One launchable application per id gio named, in gio's order; an id whose desktop file has
// vanished since the listing is dropped, which is what gio's own resolution does too.
pub fn list(mime_type: &str) -> Vec<App> {
    let output = match gio_mime(mime_type) {
        Some(output) => output,
        None => return Vec::new(),
    };
    let table = table_of(&appscan::applications_dirs());
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
fn gio_run(args: &[&str]) -> Option<std::process::Output> {
    let mut cmd = std::process::Command::new("gio");
    cmd.args(args);
    // The parse reads only tab-indented lines, so the locale cannot change the answer, but the
    // pin costs nothing and keeps the whole output byte-stable while it is being read.
    cmd.env("LC_ALL", "C");
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.process_group(0);
    let child = cmd.spawn().ok()?;
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
    output
}

fn gio_mime(mime_type: &str) -> Option<String> {
    let output = gio_run(&["mime", mime_type])?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        None
    }
}

// The "always" write: one gio mime spawn names the handler the default for the type. gio validates
// the id against the desktop database itself and writes the user's own mimeapps.list through GLib,
// so no file of Flea's is touched, no default is read back to be second-guessed, and a refused
// handler is the tool's own refusal carried to the client in the error line.
pub fn set_default(mime_type: &str, id: &str) -> Result<(), FleaError> {
    let refusal = "gio could not be asked to set the default";
    match gio_run(&["mime", mime_type, id]) {
        Some(o) if o.status.success() => Ok(()),
        Some(o) => {
            let msg = String::from_utf8_lossy(&o.stderr).trim().to_string();
            Err(FleaError { where_: "setdefault".to_string(), path: String::new(), msg: if msg.is_empty() { refusal.to_string() } else { msg } })
        }
        None => Err(FleaError { where_: "setdefault".to_string(), path: String::new(), msg: refusal.to_string() }),
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

// The write answers the same way: on a thread, with the gio refusal carried inside the line when
// there is one. A defaulted line is success only; a failure is an error line whose where is the
// request's own, so ui/js/Errors.js words it in one place.
pub fn spawn_set_default(mime_type: String, id: String, tx: std::sync::mpsc::Sender<OpMsg>) {
    std::thread::spawn(move || {
        let line = match set_default(&mime_type, &id) {
            Ok(()) => r#"{"t":"defaulted","ok":true}"#.to_string(),
            Err(e) => error_line(&e),
        };
        let _ = tx.send(OpMsg::Defaulted { line });
    });
}

// The loop's own dispatch for the open-with family, moved here from run.rs because the module owns
// the whole subject and run.rs was at its cap: handlers resolves one row's type and answers the
// ask, applications lists the desktop's installed applications, and setdefault resolves the type
// from the path's own name — a write names paths, not rows, so a listing change cannot retarget it.
pub fn run_handlers(
    out: &mut impl Write,
    tx: &std::sync::mpsc::Sender<OpMsg>,
    row: usize,
    listing: &crate::backend::listing::Listing,
    mime: &crate::backend::mime::Db,
    aliases: &crate::backend::aliases::Aliases,
) {
    let t = std::time::Instant::now();
    if row < listing.len() {
        let name = listing.name(row);
        let mime = if listing.is_dir(row) { None } else { mime.lookup(name).map(|m| aliases.canonical(m).to_string()) };
        // A directory navigates instead of opening, so it answers an empty list on the spot, and
        // so does a name no glob matched; both cost the client its one line, so a slot asked and
        // never answered cannot strand the row.
        match mime {
            Some(mime) => spawn(row, mime, tx.clone()),
            None => say(out, &handlers_line(row, &[], since(t))),
        }
    }
}

pub fn run_applications(tx: std::sync::mpsc::Sender<OpMsg>) {
    appscan::spawn_all(tx);
}

pub fn run_set_default(
    out: &mut impl Write,
    tx: &std::sync::mpsc::Sender<OpMsg>,
    path: &str,
    id: &str,
    mime: &crate::backend::mime::Db,
    aliases: &crate::backend::aliases::Aliases,
) {
    let name = Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    // No glob matched, or the name is empty: nothing about that file is a type, so there is no
    // default to set and the client is told rather than left waiting for a reply that is not coming.
    match mime.lookup(&name).map(|m| aliases.canonical(m).to_string()) {
        Some(mime_type) => spawn_set_default(mime_type, id.to_string(), tx.clone()),
        None => say(out, &error_line(&FleaError { where_: "setdefault".to_string(), path: path.to_string(), msg: "that file has no file type this desktop names".to_string() })),
    }
}

fn say(out: &mut impl Write, line: &str) {
    writeln!(out, "{}", line).ok();
}

fn since(t: std::time::Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
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

