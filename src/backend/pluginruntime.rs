//! One supervised session per installed identity in this host process.
use super::plugins::{self, Env, Installed};
use flea::jsondoc::Json;
use flea::plugin_runtime::{protocol, Config, Reply, Runtime};
use flea::ui_logic_st2::{
    manifest::{Invalid, Manifest},
    wire,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

struct Registry {
    closed: bool,
    live: Option<HashMap<PathBuf, Runtime>>,
}

static RUNNING: Mutex<Registry> = Mutex::new(Registry {
    closed: false,
    live: None,
});
const MAX_PLUGINS: usize = 64;

fn key(env: &Env, id: &str) -> PathBuf {
    env.dir.join(id)
}

pub(super) fn start(env: &Env, found: &Installed) -> Result<(), Invalid> {
    if found.plugin.manifest.block("previewer").is_some() {
        return super::pluginpreview::check(env, found);
    }
    session(env, found)?
        .ensure_ready()
        .map_err(|e| start_failure(&found.plugin.manifest, e))
}

// plugins.invalid.start names the plugin for a hello timeout; every other runtime failure keeps its error as %1.
fn start_failure(manifest: &Manifest, error: String) -> Invalid {
    if error == protocol::HELLO_TIMEOUT {
        Invalid::new("start", manifest.name.clone())
    } else {
        Invalid::new("start", error)
    }
}

fn session(env: &Env, found: &Installed) -> Result<Runtime, Invalid> {
    session_at(&RUNNING, env, found)
}

fn session_at(
    registry: &Mutex<Registry>,
    env: &Env,
    found: &Installed,
) -> Result<Runtime, Invalid> {
    let manifest = &found.plugin.manifest;
    if manifest.kinds.iter().all(|kind| kind == "previewer") {
        return Err(Invalid::new(
            "start",
            "a previewer runs only in the thumbnail sandbox",
        ));
    }
    let folder = found
        .folder
        .clone()
        .ok_or_else(|| Invalid::new("builtin", &manifest.id))?;
    let key = key(env, &manifest.id);
    let mut held = registry.lock().unwrap_or_else(|p| p.into_inner());
    if held.closed {
        return Err(Invalid::new("start", "plugin host stopped"));
    }
    let running = held.live.get_or_insert_with(HashMap::new);
    running.retain(|key, runtime| {
        let present = key.parent().is_some_and(std::path::Path::is_dir);
        if !present {
            runtime.shutdown();
        }
        present
    });
    if let Some(runtime) = running.get(&key) {
        return Ok(runtime.clone());
    }
    if running.len() >= MAX_PLUGINS {
        return Err(Invalid::new("start", "too many running plugins"));
    }
    let hello = flea::jsondoc::parse(&plugins::hello(env, manifest))
        .map_err(|e| Invalid::new("start", e))?;
    let runtime = Runtime::new(Config {
        folder,
        manifest: manifest.clone(),
        hello,
        ready_timeout: env.ready_deadline,
        request_timeout: Duration::from_secs(30),
        idle_timeout: Duration::from_secs(30),
    });
    running.insert(key, runtime.clone());
    Ok(runtime)
}

pub(super) fn failure(env: &Env, manifest: &Manifest) -> Option<Invalid> {
    RUNNING
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .live
        .as_ref()?
        .get(&key(env, &manifest.id))?
        .failure()
        .map(|e| start_failure(manifest, e))
}

pub(super) fn stop(env: &Env, id: &str) {
    let removed = RUNNING
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .live
        .as_mut()
        .and_then(|m| m.remove(&key(env, id)));
    if let Some(runtime) = removed {
        runtime.shutdown();
    }
}

pub(crate) fn shutdown() {
    shutdown_at(&RUNNING);
}

fn shutdown_at(registry: &Mutex<Registry>) {
    let running = {
        let mut held = registry.lock().unwrap_or_else(|p| p.into_inner());
        held.closed = true;
        held.live.take().unwrap_or_default()
    };
    for runtime in running.values() {
        runtime.shutdown();
    }
    drop(running);
}

pub(super) fn reconcile(env: &Env) {
    let installed = plugins::installed(env);
    for found in &installed {
        if let Some(error) = found.plugin.failure.as_ref().filter(|e| e.code == "start") {
            plugins::record_start(&found.plugin.manifest.id, Err(error.clone()));
        }
    }
    let enabled: Vec<_> = installed
        .iter()
        .filter(|p| p.plugin.enabled && p.plugin.failure.is_none())
        .map(|p| key(env, &p.plugin.manifest.id))
        .collect();
    let removed = {
        let mut held = RUNNING.lock().unwrap_or_else(|p| p.into_inner());
        let Some(running) = held.live.as_mut() else {
            return;
        };
        let stale: Vec<_> = running
            .keys()
            .filter(|key| key.parent() == Some(env.dir.as_path()) && !enabled.contains(key))
            .cloned()
            .collect();
        stale
            .into_iter()
            .filter_map(|key| running.remove(&key))
            .collect::<Vec<_>>()
    };
    for runtime in &removed {
        runtime.shutdown();
    }
}

fn request_with_progress(
    env: &Env,
    id: &str,
    message: &Json,
    progress: &mut dyn FnMut(Json),
) -> Result<Reply, Invalid> {
    let found = plugins::find(env, id)?;
    if message.get("op").and_then(Json::as_str) == Some("cancel") {
        stop(env, id);
        return Ok(Reply {
            message: Json::Obj(vec![("op".into(), Json::Str("done".into()))]),
            progress: Vec::new(),
        });
    }
    if !found.plugin.enabled || found.plugin.failure.is_some() {
        return Err(Invalid::new("start", id));
    }
    if found.folder.is_none() {
        return super::pluginshelf::builtin(env, &found, message);
    }
    if message.get("op").and_then(Json::as_str) == Some("ready") {
        start(env, &found)?;
        return Ok(Reply {
            message: Json::Obj(vec![("op".into(), Json::Str("ready".into()))]),
            progress: Vec::new(),
        });
    }
    protocol::operation(&found.plugin.manifest, message).map_err(|e| Invalid::new("request", e))?;
    let result = if message.get("op").and_then(Json::as_str) == Some("preview") {
        super::pluginpreview::preview(env, &found, message)
    } else {
        session(env, &found)?
            .request_stream(message.clone(), progress)
            .map_err(|e| {
                if e == "plugin busy" {
                    Invalid::new("busy", e)
                } else {
                    start_failure(&found.plugin.manifest, e)
                }
            })
    };
    if let Err(error) = &result {
        if !matches!(error.code, "busy" | "request") && error.detail != "plugin stopped" {
            plugins::record_start(id, Err(error.clone()));
        }
    }
    result
}

pub(super) fn settings(env: &Env, id: &str) {
    let runtime = RUNNING
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .live
        .as_ref()
        .and_then(|m| m.get(&key(env, id)).cloned());
    let Some(runtime) = runtime else {
        return;
    };
    let Ok(found) = plugins::find(env, id) else {
        return;
    };
    let settings = Json::Obj(
        found
            .plugin
            .settings
            .iter()
            .map(|(k, v)| (k.clone(), v.to_json()))
            .collect(),
    );
    if let Err(e) = runtime.request(Json::Obj(vec![
        ("op".into(), Json::Str("settings".into())),
        ("settings".into(), settings),
    ])) {
        if e != "plugin busy" {
            plugins::record_start(id, Err(start_failure(&found.plugin.manifest, e)));
        }
    }
    stop(env, id);
}

pub(super) fn reply_stream(env: &Env, request: &Json, progress: &mut dyn FnMut(String)) -> String {
    let plugin = request.get("plugin").and_then(Json::as_str).unwrap_or("");
    let result = request_with_progress(
        env,
        plugin,
        request.get("request").unwrap_or(&Json::Null),
        &mut |message| {
            progress(wire::compact(&Json::Obj(vec![
                ("t".into(), Json::Str("pluginprogress".into())),
                (
                    "id".into(),
                    request.get("id").cloned().unwrap_or(Json::Num("0".into())),
                ),
                ("plugin".into(), Json::Str(plugin.into())),
                ("message".into(), message),
            ])));
        },
    );
    reply_line(request, result)
}

pub(super) fn refusal(request: &Json, error: Invalid) -> String {
    reply_line(request, Err(error))
}

fn reply_line(request: &Json, result: Result<Reply, Invalid>) -> String {
    let plugin = request.get("plugin").and_then(Json::as_str).unwrap_or("");
    let (ok, message, progress, code, error) = match result {
        Ok(reply) => {
            let error = reply.message.get("op").and_then(Json::as_str) == Some("error");
            let detail = if error {
                reply
                    .message
                    .get("message")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_owned()
            } else {
                String::new()
            };
            (
                !error,
                reply.message,
                Json::Arr(reply.progress),
                if error {
                    "operation".into()
                } else {
                    String::new()
                },
                detail,
            )
        }
        Err(error) => (
            false,
            Json::Null,
            Json::Arr(Vec::new()),
            error.code.to_owned(),
            error.detail,
        ),
    };
    wire::compact(&Json::Obj(vec![
        ("t".into(), Json::Str("pluginruntime".into())),
        (
            "id".into(),
            request.get("id").cloned().unwrap_or(Json::Num("0".into())),
        ),
        ("plugin".into(), Json::Str(plugin.into())),
        ("ok".into(), Json::Bool(ok)),
        ("message".into(), message),
        ("progress".into(), progress),
        ("code".into(), Json::Str(code)),
        ("error".into(), Json::Str(error)),
    ]))
}

#[cfg(test)]
#[path = "pluginruntime_shutdown_tests.rs"]
mod shutdown_tests;
