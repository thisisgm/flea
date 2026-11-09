// The plugins request's smaller changes: a switch, a setting, a removal and a validation.
use super::pluginfs;
use super::plugins::{check_start, find, read_manifest, record_start, write, Env};
use flea::jsondoc::Json;
use flea::ui_logic_plugin_settings::manifest::{self, Invalid};
use flea::ui_logic_plugin_settings::{builtin, schema, shelf, store};
use std::path::Path;

pub(super) fn switch(env: &Env, plugin: &str, on: bool) -> Result<(String, String), Invalid> {
    let found = find(env, plugin)?;
    let was = found.plugin.enabled;
    write(env, &store::enable_patch(plugin, on))?;
    record_start(plugin, Ok(()));
    let mut shelf_failure = None;
    if found.plugin.manifest.block("shelf").is_some()
        && (was != on || found.plugin.failure.is_some())
    {
        // The ui.json write is the switch; a bar that could not follow is said on stderr, as `flea --ui-state` says it.
        if let Err(e) = super::pluginshelf::switch(env, &found, on) {
            shelf_failure = Some(e);
        }
    }
    if on
        && shelf_failure.is_none()
        && found
            .plugin
            .failure
            .as_ref()
            .map_or(true, |f| f.code == "start")
    {
        super::pluginruntime::stop(env, plugin);
        check_start(env, &found);
    } else if !on {
        super::pluginruntime::stop(env, plugin);
        record_start(plugin, Ok(()));
    }
    if let Some(error) = shelf_failure {
        record_start(plugin, Err(error));
    }
    Ok((plugin.to_string(), found.plugin.manifest.version))
}

pub(super) fn set(
    env: &Env,
    plugin: &str,
    key: &str,
    raw: &Json,
) -> Result<(String, String), Invalid> {
    let found = find(env, plugin)?;
    let refused = || Invalid::new("setting", key);
    let value = if plugin == builtin::SHELF {
        shelf::value(key, raw).ok_or_else(refused)?
    } else {
        let field = found
            .plugin
            .manifest
            .schema
            .iter()
            .find(|f| f.key == key)
            .ok_or_else(refused)?;
        schema::clamp(field, Some(raw))
    };
    write(env, &store::setting_patch(plugin, key, &value))?;
    super::pluginruntime::settings(env, plugin);
    Ok((plugin.to_string(), found.plugin.manifest.version))
}

pub(super) fn remove(env: &Env, plugin: &str) -> Result<(String, String), Invalid> {
    let found = find(env, plugin)?;
    if found.folder.is_some() && found.plugin.enabled {
        super::pluginshelf::switch(env, &found, false)?;
    }
    let folder = found
        .folder
        .ok_or_else(|| Invalid::new("builtin", plugin))?;
    super::pluginruntime::stop(env, plugin);
    pluginfs::remove(&env.dir, &folder).map_err(|e| Invalid::new("write", e))?;
    pluginfs::forget_source(&env.dir, plugin).map_err(|e| Invalid::new("write", e))?;
    record_start(plugin, Ok(()));
    write(env, &store::forget_patch(plugin))?;
    Ok((plugin.to_string(), String::new()))
}

pub(super) fn validate(folder: &Path) -> Result<(String, String), Invalid> {
    let text = read_manifest(folder)
        .ok_or_else(|| Invalid::new("no_manifest", folder.to_string_lossy()))?;
    let m = manifest::parse(&text, false)?;
    pluginfs::entry(folder, &m.run)?;
    Ok((m.id, m.version))
}
