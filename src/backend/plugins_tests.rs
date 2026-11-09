// The plugins request through its own handler and through a real backend child, every write inside one marked sandbox.
use super::plugins::{handle, Env};
use super::testdir::TestDir;
use crate::uistore::Store;
use flea::ui_logic_st2::wire::{self, Listing, Outcome, Plugin, Reply, Source};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::Duration;

thread_local! {
    static SYNCED: RefCell<Vec<bool>> = const { RefCell::new(Vec::new()) };
}

fn synced(on: bool) -> Result<(), String> {
    SYNCED.with(|s| s.borrow_mut().push(on));
    Ok(())
}

pub(super) fn env(dir: &TestDir) -> Env {
    Env {
        dir: dir.join("config/flea/plugins"),
        store: Store::at(&dir.join("state"), &dir.join("config")),
        pile: dir.join("state/omarchy/flea-shelf/shelf.json"),
        shelf_sync: synced,
        plugin_manager: PathBuf::from("omarchy"),
        ready_deadline: Duration::from_millis(1500),
        git_deadline: Duration::from_secs(30),
    }
}

pub(super) const READY: &str = "#!/bin/sh\nread hello\necho '{\"op\":\"ready\"}'\ncat >/dev/null\n";

// Sample: plugin(&dir, "src", "me.tool", READY) writes src/manifest.json and src/bin/run.
pub(super) fn plugin(dir: &TestDir, folder: &str, id: &str, script: &str) -> PathBuf {
    let root = dir.dir(folder);
    dir.dir(&format!("{folder}/bin"));
    dir.file(&format!("{folder}/manifest.json"), &format!(
        r#"{{"schemaVersion":1,"id":"{id}","name":"Tool","version":"1.0.0","author":"me","description":"A tool.",
"kinds":["menu-action"],"entryPoints":{{"run":"bin/run"}},"menuActions":[{{"id":"go","label":"Go"}}],"access":["selection"],
"schema":[{{"key":"n","type":"integer","label":"N","min":1,"max":10,"step":1,"defaultValue":3}}]}}"#));
    dir.script(&format!("{folder}/bin/run"), script);
    root
}

pub(super) fn ask(env: &Env, line: &str) -> (Option<Outcome>, Listing) {
    let lines = handle(line, env);
    let mut outcome = None;
    for line in &lines {
        match wire::parse(line).unwrap_or_else(|e| panic!("{e}: {line}")) {
            Reply::Outcome(o) => outcome = Some(o),
            Reply::Listing(l) => return (outcome, l),
        }
    }
    panic!("no listing in {lines:?}")
}

pub(super) fn get<'a>(listing: &'a Listing, id: &str) -> &'a Plugin {
    listing.plugins.iter().find(|p| p.manifest.id == id).unwrap_or_else(|| panic!("{id} not listed"))
}

pub(super) fn code(outcome: Option<Outcome>) -> &'static str {
    outcome.expect("an outcome").error.map_or("", |e| e.code)
}

fn stored(env: &Env) -> String {
    crate::jsondoc::render(&env.store.read()).split_whitespace().collect()
}

#[test]
fn st2_backend_a_fresh_list_is_the_five_built_ins_as_they_ship() {
    let dir = TestDir::new("st2-list");
    let (none, listing) = ask(&env(&dir), r#"{"c":"plugins","op":"list","id":4}"#);
    assert!(none.is_none() && listing.id == 4);
    let shipped: Vec<(&str, bool, &Source)> = listing.plugins.iter().map(|p| (p.manifest.id.as_str(), p.enabled, &p.source)).collect();
    assert_eq!(shipped, [("flea.shelf", false, &Source::Builtin), ("flea.localsend", true, &Source::Builtin),
                         ("flea.tailscale", true, &Source::Builtin), ("flea.dropbox", true, &Source::Builtin),
                         ("flea.airdrop", false, &Source::Builtin)]);
    let shelf = get(&listing, "flea.shelf").shelf.clone().expect("the Shelf's page input");
    assert_eq!((shelf.recent, shelf.rail.as_str(), shelf.pins.len()), (3, "off", 0));
}

#[test]
fn st2_backend_the_shelf_checkbox_is_shelf_enabled_and_moves_the_bar() {
    let dir = TestDir::new("st2-shelf");
    let env = env(&dir);
    SYNCED.with(|s| s.borrow_mut().clear());
    let (done, listing) = ask(&env, r#"{"c":"plugins","op":"enable","id":1,"plugin":"flea.shelf"}"#);
    assert_eq!(code(done), "");
    assert!(get(&listing, "flea.shelf").enabled);
    assert!(stored(&env).contains(r#""shelf":{"enabled":true"#), "{}", stored(&env));
    let (_, listing) = ask(&env, r#"{"c":"plugins","op":"enable","id":2,"plugin":"flea.shelf"}"#);
    assert!(get(&listing, "flea.shelf").enabled);
    ask(&env, r#"{"c":"plugins","op":"disable","id":3,"plugin":"flea.shelf"}"#);
    assert_eq!(SYNCED.with(|s| s.borrow().clone()), [true, false], "the bar follows a change of the switch, once each way");
    let (done, _) = ask(&env, r#"{"c":"plugins","op":"set","id":4,"plugin":"flea.shelf","key":"rail","value":"left"}"#);
    assert_eq!(code(done), "");
    let (refused, listing) = ask(&env, r#"{"c":"plugins","op":"set","id":5,"plugin":"flea.shelf","key":"recent","value":4}"#);
    assert_eq!(code(refused), "setting", "a count the row does not offer is refused");
    assert_eq!(get(&listing, "flea.shelf").shelf.as_ref().unwrap().rail, "left");
}

#[test]
fn st2_backend_switching_a_built_in_off_keeps_it_off_without_touching_the_bar() {
    let dir = TestDir::new("st2-builtin");
    let env = env(&dir);
    SYNCED.with(|s| s.borrow_mut().clear());
    let (_, listing) = ask(&env, r#"{"c":"plugins","op":"disable","id":1,"plugin":"flea.tailscale"}"#);
    assert!(!get(&listing, "flea.tailscale").enabled);
    assert!(stored(&env).contains(r#""plugins":{"flea.tailscale":{"enabled":false}}"#));
    assert!(SYNCED.with(|s| s.borrow().is_empty()));
    let (refused, _) = ask(&env, r#"{"c":"plugins","op":"remove","id":2,"plugin":"flea.tailscale"}"#);
    assert_eq!(code(refused), "builtin", "a built-in comes with Flea and has no Remove");
    let (refused, _) = ask(&env, r#"{"c":"plugins","op":"update","id":3,"plugin":"flea.dropbox"}"#);
    assert_eq!(code(refused), "builtin");
}

#[test]
fn st2_backend_a_folder_add_arrives_switched_off_and_remembers_where_it_came_from() {
    let dir = TestDir::new("st2-add");
    let env = env(&dir);
    let source = plugin(&dir, "src", "me.added", READY);
    let line = format!(r#"{{"c":"plugins","op":"add","id":9,"source":"{}"}}"#, source.display());
    let (done, listing) = ask(&env, &line);
    let done = done.unwrap();
    assert_eq!((done.error, done.plugin.as_str(), done.version.as_str()), (None, "me.added", "1.0.0"));
    let added = get(&listing, "me.added");
    assert!(!added.enabled && added.failure.is_none());
    assert_eq!(added.source, Source::Folder(source.display().to_string()));
    let copied = env.dir.join("me.added/bin/run");
    assert!(super::pluginfs::entry(&env.dir.join("me.added"), "bin/run").is_ok(), "the executable bit survives the copy: {}", copied.display());
    let (again, _) = ask(&env, &line);
    assert_eq!(code(again), "exists");
    assert_no_staging(&env.dir);
}

fn assert_no_staging(root: &Path) {
    let left: Vec<String> = std::fs::read_dir(root).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    assert!(left.iter().all(|n| !n.starts_with(".adding-")), "a failed add leaves nothing behind: {left:?}");
}

#[test]
fn st2_backend_an_add_refuses_what_is_not_a_plugin_by_its_own_code() {
    let dir = TestDir::new("st2-refuse");
    let env = env(&dir);
    let add = |source: &str| code(ask(&env, &format!(r#"{{"c":"plugins","op":"add","id":1,"source":"{source}"}}"#)).0);
    assert_eq!(add(""), "not_found");
    assert_eq!(add("relative/path"), "not_found");
    assert_eq!(add(&dir.join("absent").display().to_string()), "not_found");
    assert_eq!(add(&dir.dir("empty").display().to_string()), "no_manifest");
    let reserved = plugin(&dir, "reserved", "flea.mine", READY);
    assert_eq!(add(&reserved.display().to_string()), "reserved");
    let broken = dir.dir("broken");
    dir.file("broken/manifest.json", "{\"schemaVersion\":1");
    assert_eq!(add(&broken.display().to_string()), "json");
    assert_no_staging(&env.dir);
    assert_eq!(ask(&env, r#"{"c":"plugins","op":"add","id":1}"#).1.plugins.len(), 5, "nothing was added");
}

#[test]
fn st2_backend_enabling_asks_the_plugin_to_start_and_a_silent_one_failed_to_start() {
    let dir = TestDir::new("st2-start");
    let mut env = env(&dir);
    env.ready_deadline = Duration::from_millis(400);
    for (folder, id, script) in [("a", "me.ready", READY), ("b", "me.silent", "#!/bin/sh\ncat >/dev/null\n")] {
        let source = plugin(&dir, folder, id, script);
        assert_eq!(code(ask(&env, &format!(r#"{{"c":"plugins","op":"add","id":1,"source":"{}"}}"#, source.display())).0), "");
    }
    let (_, listing) = ask(&env, r#"{"c":"plugins","op":"enable","id":2,"plugin":"me.ready"}"#);
    assert!(get(&listing, "me.ready").enabled && get(&listing, "me.ready").failure.is_none());
    let (done, listing) = ask(&env, r#"{"c":"plugins","op":"enable","id":3,"plugin":"me.silent"}"#);
    assert_eq!(code(done), "", "the switch itself is saved");
    let silent = get(&listing, "me.silent");
    assert!(silent.enabled);
    assert_eq!(silent.failure.as_ref().map(|f| f.code), Some("start"));
    let (_, listing) = ask(&env, r#"{"c":"plugins","op":"disable","id":4,"plugin":"me.silent"}"#);
    assert!(get(&listing, "me.silent").failure.is_none(), "a plugin switched off is not failing");
    std::fs::set_permissions(env.dir.join("me.ready/bin/run"), std::os::unix::fs::PermissionsExt::from_mode(0o644)).unwrap();
    let (_, listing) = ask(&env, r#"{"c":"plugins","op":"list","id":5}"#);
    assert_eq!(get(&listing, "me.ready").failure.as_ref().map(|f| f.code), Some("exec"), "an entry that cannot run cannot start");
}

#[test]
fn st2_backend_a_hello_timeout_names_the_plugin_and_an_early_exit_keeps_its_error() {
    // EY-551: plugins.invalid.start says "%1 did not answer hello", so only a hello timeout puts the plugin's name in %1.
    let dir = TestDir::new("st2-hello");
    let mut env = env(&dir);
    env.ready_deadline = Duration::from_millis(400);
    for (folder, id, script) in [("a", "me.silent", "#!/bin/sh\ncat >/dev/null\n"), ("b", "me.gone", "#!/bin/sh\nexit 4\n")] {
        let source = plugin(&dir, folder, id, script);
        assert_eq!(code(ask(&env, &format!(r#"{{"c":"plugins","op":"add","id":1,"source":"{}"}}"#, source.display())).0), "");
    }
    let (_, listing) = ask(&env, r#"{"c":"plugins","op":"enable","id":2,"plugin":"me.silent"}"#);
    let silent = get(&listing, "me.silent").failure.clone().unwrap();
    assert_eq!((silent.code, silent.detail.as_str()), ("start", "Tool"));
    let i18n = crate::i18n::test_catalogs::library_english();
    assert_eq!(i18n.tr_args(&silent.key(), &[&silent.detail]), "Tool did not answer hello within five seconds");
    let (_, listing) = ask(&env, r#"{"c":"plugins","op":"enable","id":3,"plugin":"me.gone"}"#);
    let gone = get(&listing, "me.gone").failure.clone().unwrap();
    assert_eq!(gone.code, "start");
    assert!(matches!(gone.detail.as_str(), "plugin closed stdout" | "Broken pipe (os error 32)"), "an early exit keeps the error it hit at hello: {gone:?}");
}

#[test]
fn st2_backend_a_setting_is_clamped_before_it_is_stored_and_remove_forgets_it() {
    let dir = TestDir::new("st2-set");
    let env = env(&dir);
    let source = plugin(&dir, "src", "me.set", READY);
    ask(&env, &format!(r#"{{"c":"plugins","op":"add","id":1,"source":"{}"}}"#, source.display()));
    let (done, listing) = ask(&env, r#"{"c":"plugins","op":"set","id":2,"plugin":"me.set","key":"n","value":99}"#);
    assert_eq!(code(done), "");
    assert_eq!(get(&listing, "me.set").settings, [("n".into(), flea::ui_logic_st2::schema::Value::Int(10))]);
    assert!(stored(&env).contains(r#""me.set":{"enabled":false,"settings":{"n":10}}"#), "{}", stored(&env));
    assert_eq!(code(ask(&env, r#"{"c":"plugins","op":"set","id":3,"plugin":"me.set","key":"missing","value":1}"#).0), "setting");
    assert_eq!(code(ask(&env, r#"{"c":"plugins","op":"set","id":3,"plugin":"me.nobody","key":"n","value":1}"#).0), "unknown");
    let (done, listing) = ask(&env, r#"{"c":"plugins","op":"remove","id":4,"plugin":"me.set"}"#);
    assert_eq!(code(done), "");
    assert!(listing.plugins.iter().all(|p| p.manifest.id != "me.set"));
    assert!(!env.dir.join("me.set").exists());
    assert!(!stored(&env).contains("me.set"), "remove takes the plugin's settings with it");
    assert!(source.join("manifest.json").is_file(), "the folder it was added from is untouched");
}

#[test]
fn st2_backend_a_broken_folder_lists_as_failed_under_its_folder_name() {
    let dir = TestDir::new("st2-broken");
    let env = env(&dir);
    dir.dir("config/flea/plugins/half.done");
    dir.file("config/flea/plugins/half.done/manifest.json", "{\"schemaVersion\":");
    dir.dir("config/flea/plugins/.hidden");
    let (_, listing) = ask(&env, r#"{"c":"plugins","op":"list","id":1}"#);
    assert_eq!(listing.plugins.len(), 6, "a dot folder is not a plugin");
    let broken = get(&listing, "half.done");
    assert_eq!((broken.manifest.name.as_str(), broken.failure.as_ref().map(|f| f.code)), ("half.done", Some("json")));
    let (done, listing) = ask(&env, r#"{"c":"plugins","op":"remove","id":2,"plugin":"half.done"}"#);
    assert_eq!(code(done), "", "a failed plugin can still be removed from its page");
    assert_eq!(listing.plugins.len(), 5);
}

#[test]
fn st2_backend_a_request_it_cannot_read_is_refused_with_its_id() {
    let dir = TestDir::new("st2-bad");
    let env = env(&dir);
    let lines = handle("{\"c\":\"plugins\",", &env);
    assert_eq!(lines.len(), 1);
    assert!(matches!(wire::parse(&lines[0]), Ok(Reply::Outcome(Outcome { error: Some(ref e), .. })) if e.code == "request"));
    let lines = handle(r#"{"c":"plugins","op":"format","id":6}"#, &env);
    assert!(matches!(wire::parse(&lines[0]), Ok(Reply::Outcome(Outcome { id: 6, error: Some(ref e), .. })) if e.code == "request" && e.detail == "format"));
    let (done, _) = ask(&env, &format!(r#"{{"c":"plugins","op":"validate","id":7,"source":"{}"}}"#, plugin(&dir, "v", "me.valid", READY).display()));
    assert_eq!(done.map(|o| (o.plugin, o.error)), Some(("me.valid".into(), None)));
}

#[test]
fn plugin_runtime_builtin_shelf_requests_move_the_bar_each_way() {
    let dir = TestDir::new("plugin-runtime-builtin-shelf");
    let env = env(&dir);
    let (done, _) = ask(&env, r#"{"c":"plugins","op":"enable","id":1,"plugin":"flea.shelf"}"#);
    assert_eq!(code(done), "");
    SYNCED.with(|s| s.borrow_mut().clear());
    for (id, action) in [(2, "enable"), (3, "disable")] {
        let line = format!(r#"{{"c":"plugins","op":"runtime","id":{id},"plugin":"flea.shelf","request":{{"op":"shelf","action":"{action}"}}}}"#);
        let reply = handle(&line, &env);
        assert!(reply[0].contains(r#""ok":true"#), "{action}: {reply:?}");
    }
    assert_eq!(SYNCED.with(|s| s.borrow().clone()), [true, false], "the Shelf's own requests switch the bar");
}
