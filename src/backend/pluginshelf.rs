//! Shelf kinds use Omarchy's own plugin switch; the packaged pile retains its existing adapter.
use super::plugins::{Env, Installed};
use flea::jsondoc::Json;
use flea::plugin_runtime::{protocol, Reply};
use flea::ui_logic_plugin_settings::{manifest::Invalid, wire::Source};
use std::process::{Command, Stdio};
use std::time::Duration;

pub(super) fn switch(env: &Env, found: &Installed, on: bool) -> Result<(), Invalid> {
    let Some(block) = found.plugin.manifest.block("shelf") else {
        return Ok(());
    };
    let name = block
        .get("omarchyPlugin")
        .and_then(Json::as_str)
        .ok_or_else(|| Invalid::new("block", "omarchyPlugin"))?;
    if found.plugin.source == Source::Builtin {
        return (env.shelf_sync)(on).map_err(|e| Invalid::new("start", e));
    }
    omarchy(env, name, on).map_err(|e| Invalid::new("start", e))
}

fn omarchy(env: &Env, name: &str, on: bool) -> Result<(), String> {
    let mut command = Command::new(&env.plugin_manager);
    command.args(["plugin", if on { "enable" } else { "disable" }, name]);
    if std::env::var_os("OMARCHY_PATH").map_or(true, |p| p.is_empty()) {
        command.env("OMARCHY_PATH", "/usr/share/omarchy");
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = command
        .spawn()
        .map_err(|e| format!("Omarchy plugin {name}: {e}"))?;
    super::pluginfs::finish(child, Duration::from_secs(5))
        .map_err(|e| format!("Omarchy plugin {name}: {e}"))
}

pub(super) fn builtin(env: &Env, found: &Installed, message: &Json) -> Result<Reply, Invalid> {
    protocol::operation(&found.plugin.manifest, message).map_err(|e| Invalid::new("request", e))?;
    if message.get("op").and_then(Json::as_str) != Some("shelf") {
        return Err(Invalid::new(
            "request",
            "builtin adapter does not own this operation",
        ));
    }
    let action = message.get("action").and_then(Json::as_str).unwrap_or("");
    match action {
        "enable" => switch(env, found, true)?,
        "disable" => switch(env, found, false)?,
        "add" => {
            let paths = message
                .get("paths")
                .and_then(Json::as_array)
                .filter(|paths| !paths.is_empty())
                .ok_or_else(|| Invalid::new("request", "empty shelf selection"))?;
            let binary =
                std::env::current_exe().map_err(|e| Invalid::new("start", e.to_string()))?;
            let mut command = Command::new(binary);
            command.args(["shelf", "add"]);
            for path in paths {
                command.arg(
                    path.as_str()
                        .ok_or_else(|| Invalid::new("request", "shelf path"))?,
                );
            }
            let child = command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| Invalid::new("start", e.to_string()))?;
            super::pluginfs::finish(child, Duration::from_secs(5))
                .map_err(|e| Invalid::new("start", e))?;
        }
        _ => return Err(Invalid::new("request", "unknown shelf action")),
    }
    Ok(Reply {
        message: Json::Obj(vec![("op".into(), Json::Str("done".into()))]),
        progress: Vec::new(),
    })
}
