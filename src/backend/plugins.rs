// The plugins request: list, enable, disable, set, add, update, remove and validate; see docs/protocol.md "plugins".
use super::{pluginfs, plugingit, pluginops};
use flea::jsondoc::{self, Json};
use flea::ui_logic_plugin_settings::manifest::{self, Invalid, Manifest};
use flea::ui_logic_plugin_settings::wire::{self, Listing, Outcome, Plugin, Source};
use flea::ui_logic_plugin_settings::{builtin, schema, shelf, store};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

// The protocol's own deadline for an answer to hello.
pub const READY_DEADLINE: Duration = Duration::from_secs(5);
// Far past any pile the bar plugin writes, so a larger file reads as an empty pile.
const MAX_PILE_BYTES: u64 = 4 << 20;

pub struct Env {
    pub dir: PathBuf,
    pub store: crate::uistore::Store,
    pub pile: PathBuf,
    pub shelf_sync: fn(bool) -> Result<(), String>,
    pub plugin_manager: PathBuf,
    pub ready_deadline: Duration,
    pub git_deadline: Duration,
}

impl Env {
    pub fn user() -> Result<Env, String> {
        Ok(Env {
            dir: crate::userfile::config_home()?.join("flea/plugins"),
            store: crate::uistore::Store::user()?,
            pile: crate::shelf::Shelf::user()?.pile_file().to_path_buf(),
            shelf_sync: crate::shelfplugin::sync,
            plugin_manager: PathBuf::from("omarchy"),
            ready_deadline: READY_DEADLINE,
            git_deadline: plugingit::GIT_DEADLINE,
        })
    }
}

// Every change is one at a time, so two windows adding or removing never race on one folder.
static CHANGING: Mutex<()> = Mutex::new(());
// A start that failed this session, kept until the plugin is switched off, updated or starts.
static FAILED: Mutex<Option<HashMap<String, Invalid>>> = Mutex::new(None);

pub fn record_start(id: &str, result: Result<(), Invalid>) {
    let mut held = FAILED.lock().unwrap_or_else(|p| p.into_inner());
    let map = held.get_or_insert_with(HashMap::new);
    match result {
        Ok(()) => map.remove(id),
        Err(why) => map.insert(id.to_string(), why),
    };
}

fn start_failure(id: &str) -> Option<Invalid> {
    FAILED
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
        .and_then(|m| m.get(id).cloned())
}

// Answers on its own thread: a clone, a pull or a start check takes seconds and the loop keeps listing.
pub fn request(line: String, replies: std::sync::mpsc::Sender<super::opsreq::OpMsg>) {
    std::thread::spawn(move || {
        let lines = match Env::user() {
            Ok(env) => handle_stream(&line, &env, &mut |line| {
                let _ = replies.send(super::opsreq::OpMsg::Meta { line });
            }),
            Err(e) => vec![refusal(&line, Invalid::new("write", e))],
        };
        for line in lines {
            let _ = replies.send(super::opsreq::OpMsg::Meta { line });
        }
    });
}

fn state(env: &Env) -> Json {
    jsondoc::parse(&crate::jsondoc::render(&env.store.read())).unwrap_or(Json::Null)
}

pub fn write(env: &Env, patch: &Json) -> Result<(), Invalid> {
    let patch =
        crate::jsondoc::parse(&wire::compact(patch)).map_err(|e| Invalid::new("setting", e))?;
    env.store
        .update(&patch)
        .map(|_| ())
        .map_err(|e| Invalid::new("setting", e))
}

// The installed folders, each with its manifest or the reason it has none.
fn discovered(env: &Env) -> Vec<(PathBuf, Result<Manifest, Invalid>)> {
    let mut found: Vec<(String, PathBuf)> = fs::read_dir(&env.dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.file_name().into_string().ok().map(|n| (n, e.path())))
                .filter(|(name, path)| !name.starts_with('.') && path.is_dir())
                .take(256)
                .collect()
        })
        .unwrap_or_default();
    found.sort_by_key(|(name, _)| name.to_lowercase());
    found
        .into_iter()
        .map(|(name, path)| {
            let parsed = match read_manifest(&path) {
                Some(text) => manifest::parse(&text, false),
                None => Err(Invalid::new("manifest", name)),
            };
            (path, parsed)
        })
        .collect()
}

// A manifest is a small regular file; a fifo, a folder or anything past the bound reads as none.
pub fn read_manifest(folder: &Path) -> Option<String> {
    use std::io::Read;
    let mut text = String::new();
    let file = super::regfile::open_regular(&folder.join("manifest.json"))?;
    file.take(manifest::MAX_BYTES as u64 + 1)
        .read_to_string(&mut text)
        .ok()?;
    (text.len() <= manifest::MAX_BYTES).then_some(text)
}

fn named_after(folder: &Path) -> Manifest {
    let name = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Manifest {
        id: name.clone(),
        name,
        version: String::new(),
        author: String::new(),
        license: String::new(),
        description: String::new(),
        kinds: Vec::new(),
        run: String::new(),
        mark: String::new(),
        glyph: String::new(),
        access: Vec::new(),
        schema: Vec::new(),
        blocks: Vec::new(),
    }
}

// A clone names its origin in its own .git folder; a folder add's record lives outside every plugin folder.
fn source_of(folder: &Path) -> Source {
    if let Some(origin) = plugingit::git_origin(folder) {
        return Source::Git(origin);
    }
    let (Some(store), Some(id)) = (folder.parent(), folder.file_name().and_then(|n| n.to_str()))
    else {
        return Source::Unknown;
    };
    pluginfs::read_source(store, id).map_or(Source::Unknown, Source::Folder)
}

pub struct Installed {
    pub plugin: Plugin,
    pub folder: Option<PathBuf>,
}

fn shelf_input(state: &Json, env: &Env) -> shelf::Input {
    // The bar plugin owns this file, so a fifo or an endless file in its place reads as an empty pile.
    let pile = super::regfile::read_text(&env.pile, MAX_PILE_BYTES).unwrap_or_default();
    shelf::input(state, shelf::pinned(&pile))
}

pub fn installed(env: &Env) -> Vec<Installed> {
    let state = state(env);
    let mut out: Vec<Installed> = builtin::manifests()
        .into_iter()
        .map(|(m, on)| {
            let enabled = store::enabled(&state, &m.id, on);
            let shelf = (m.id == builtin::SHELF).then(|| shelf_input(&state, env));
            Installed {
                plugin: Plugin {
                    source: Source::Builtin,
                    enabled,
                    failure: enabled.then(|| start_failure(&m.id)).flatten(),
                    mark_path: String::new(),
                    settings: Vec::new(),
                    shelf,
                    manifest: m,
                },
                folder: None,
            }
        })
        .collect();
    for (folder, parsed) in discovered(env) {
        let (manifest, mut failure) = match parsed {
            Ok(m) => (m, None),
            Err(why) => (named_after(&folder), Some(why)),
        };
        if failure.is_none() && out.iter().any(|i| i.plugin.manifest.id == manifest.id) {
            failure = Some(Invalid::new("duplicate", manifest.id.clone()));
        }
        let enabled = store::enabled(&state, &manifest.id, false);
        if enabled && failure.is_none() {
            failure = pluginfs::entry(&folder, &manifest.run)
                .err()
                .or_else(|| super::pluginruntime::failure(env, &manifest))
                .or_else(|| start_failure(&manifest.id));
        }
        let settings = schema::settings(
            &manifest.schema,
            store::stored_settings(&state, &manifest.id),
        );
        let mark_path = Some(manifest.mark.as_str())
            .filter(|m| !m.is_empty())
            .map(|m| folder.join(m))
            .filter(|p| p.is_file())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let plugin = Plugin {
            source: source_of(&folder),
            enabled,
            failure,
            mark_path,
            settings,
            shelf: None,
            manifest,
        };
        out.push(Installed {
            plugin,
            folder: Some(folder),
        });
    }
    out
}

pub(super) fn hello(env: &Env, m: &Manifest) -> String {
    let i18n = flea::i18n::I18n::default();
    let settings = schema::settings(&m.schema, store::stored_settings(&state(env), &m.id));
    wire::compact(&Json::Obj(vec![
        ("op".into(), Json::Str("hello".into())),
        (
            "protocolVersion".into(),
            Json::Num(flea::plugin_runtime::protocol::VERSION.to_string()),
        ),
        ("flea".into(), Json::Str(env!("CARGO_PKG_VERSION").into())),
        ("language".into(), Json::Str(i18n.language().into())),
        ("decimal".into(), Json::Str(i18n.locale().decimal.clone())),
        (
            "settings".into(),
            Json::Obj(
                settings
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_json()))
                    .collect(),
            ),
        ),
        (
            "kinds".into(),
            Json::Arr(m.kinds.iter().map(|k| Json::Str(k.clone())).collect()),
        ),
    ]))
}

// Built-ins run inside Flea, so only a plugin with a folder is asked to start.
pub fn check_start(env: &Env, found: &Installed) {
    if found.folder.is_some() {
        let m = &found.plugin.manifest;
        record_start(&m.id, super::pluginruntime::start(env, found));
    }
}

pub(super) fn refusal(line: &str, error: Invalid) -> String {
    let doc = jsondoc::parse(line).unwrap_or(Json::Null);
    if doc.get("op").and_then(Json::as_str) == Some("runtime") {
        return super::pluginruntime::refusal(&doc, error);
    }
    let text = |k: &str| doc.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    let id = doc
        .get("id")
        .and_then(Json::as_f64)
        .filter(|n| *n >= 0.0 && n.fract() == 0.0)
        .unwrap_or(0.0) as u64;
    wire::outcome_line(&Outcome {
        id,
        op: text("op"),
        plugin: text("plugin"),
        error: Some(error),
        version: String::new(),
    })
}

// One request line in, its answer lines out: an outcome for a change, then the listing every change ends in.
pub fn handle(line: &str, env: &Env) -> Vec<String> {
    handle_stream(line, env, &mut |_| {})
}

pub(super) fn handle_stream(
    line: &str,
    env: &Env,
    progress: &mut dyn FnMut(String),
) -> Vec<String> {
    let doc = match jsondoc::parse(line) {
        Ok(doc) => doc,
        Err(e) => return vec![refusal(line, Invalid::new("request", e))],
    };
    let text = |k: &str| doc.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    let id = doc
        .get("id")
        .and_then(Json::as_f64)
        .filter(|n| *n >= 0.0 && n.fract() == 0.0)
        .unwrap_or(0.0) as u64;
    let op = text("op");
    if op == "runtime" {
        return vec![super::pluginruntime::reply_stream(env, &doc, progress)];
    }
    let listing = |env: &Env| {
        wire::listing_line(&Listing {
            id,
            plugins: installed(env).into_iter().map(|i| i.plugin).collect(),
        })
    };
    if op == "list" {
        super::pluginruntime::reconcile(env);
        return vec![listing(env)];
    }
    if !wire::OPS.contains(&op.as_str()) {
        return vec![refusal(line, Invalid::new("request", op))];
    }
    let _one = CHANGING.lock().unwrap_or_else(|p| p.into_inner());
    let plugin = text("plugin");
    let result = match op.as_str() {
        "enable" | "disable" => pluginops::switch(env, &plugin, op == "enable"),
        "set" => pluginops::set(
            env,
            &plugin,
            &text("key"),
            doc.get("value").unwrap_or(&Json::Null),
        ),
        "add" => super::pluginadd::add(env, &text("source")).map(|m| (m.id, m.version)),
        "update" => super::pluginadd::update(env, &plugin),
        "remove" => pluginops::remove(env, &plugin),
        _ => pluginops::validate(&flea::bytename::view(&text("source"))),
    };
    let (named, error, version) = match result {
        Ok((named, version)) => (named, None, version),
        Err(why) => (plugin, Some(why), String::new()),
    };
    vec![
        wire::outcome_line(&Outcome {
            id,
            op,
            plugin: named,
            error,
            version,
        }),
        listing(env),
    ]
}

pub fn find(env: &Env, plugin: &str) -> Result<Installed, Invalid> {
    installed(env)
        .into_iter()
        .find(|i| i.plugin.manifest.id == plugin)
        .ok_or_else(|| Invalid::new("unknown", plugin))
}
