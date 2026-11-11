// Add from a Git URL or a local folder, and Update from Git; both land a whole folder or nothing.
use super::plugins::{self, Env};
use super::{pluginfs, plugingit};
use flea::ui_logic_plugin_settings::manifest::{self, Invalid, Manifest};
use flea::ui_logic_plugin_settings::wire::Source;
use flea::ui_logic_plugin_settings::{builtin, store};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

enum From {
    Git(String),
    Folder(PathBuf),
}

// Sample input: "https://github.com/mtorvik/flea-checksums", "git@host:me/p.git", "/home/me/p" or "~/p".
fn classify(source: &str) -> Result<From, Invalid> {
    let source = source.trim();
    let missing = || Invalid::new("not_found", source);
    if source.contains("://") || source.starts_with("git@") {
        return Ok(From::Git(source.to_string()));
    }
    let path = match source.strip_prefix("~/") {
        Some(rest) => crate::userfile::home()
            .map_err(|_| missing())?
            .join(flea::bytename::view(rest)),
        None => flea::bytename::exact(source),
    };
    if !path.is_absolute() || !path.is_dir() {
        return Err(missing());
    }
    Ok(From::Folder(path))
}

fn staged(env: &Env, source: &str, from: &From, staging: &PathBuf) -> Result<Manifest, Invalid> {
    match from {
        From::Git(url) => {
            plugingit::clone(url, staging, env.git_deadline)
                .map_err(|_| Invalid::new("git", source))?;
            // A clone is held to a folder add's bounds before it can land, its own .git aside.
            pluginfs::measure_clone(staging).map_err(|e| Invalid::new("write", e))?
        }
        From::Folder(path) => {
            pluginfs::copy_tree(path, staging).map_err(|e| Invalid::new("write", e))?
        }
    }
    let text =
        plugins::read_manifest(staging).ok_or_else(|| Invalid::new("no_manifest", source))?;
    let found = manifest::parse(&text, false)?;
    let taken = builtin::is_builtin(&found.id)
        || env.dir.join(&found.id).exists()
        || plugins::find(env, &found.id).is_ok();
    if taken {
        return Err(Invalid::new("exists", found.id));
    }
    let record = match from {
        From::Folder(path) => {
            pluginfs::write_source(&env.dir, &found.id, &path.display().to_string())
        }
        From::Git(_) => pluginfs::forget_source(&env.dir, &found.id),
    };
    record.map_err(|e| Invalid::new("write", e))?;
    if let Err(e) = fs::rename(staging, env.dir.join(&found.id)) {
        let _ = pluginfs::forget_source(&env.dir, &found.id);
        return Err(Invalid::new("write", e.to_string()));
    }
    Ok(found)
}

// An added plugin arrives switched off, as `omarchy plugin add` leaves one without --enable.
pub fn add(env: &Env, source: &str) -> Result<Manifest, Invalid> {
    let from = classify(source)?;
    if let From::Folder(path) = &from {
        pluginfs::measure(path).map_err(|e| Invalid::new("write", e))?;
    }
    fs::create_dir_all(&env.dir).map_err(|e| Invalid::new("write", e.to_string()))?;
    let staging = env.dir.join(format!(
        ".adding-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let landed = staged(env, source.trim(), &from, &staging);
    if staging.exists() {
        let _ = pluginfs::remove(&env.dir, &staging);
    }
    let found = landed?;
    // Settings a removed copy left behind never reach the new one.
    plugins::write(env, &store::forget_patch(&found.id))?;
    plugins::write(env, &store::enable_patch(&found.id, false))?;
    Ok(found)
}

// Update pulls new commits for a plugin added from Git, then asks it to start again if it is on.
pub fn update(env: &Env, plugin: &str) -> Result<(String, String), Invalid> {
    let found = plugins::find(env, plugin)?;
    let folder = found
        .folder
        .clone()
        .ok_or_else(|| Invalid::new("builtin", plugin))?;
    let Source::Git(origin) = &found.plugin.source else {
        return Err(Invalid::new("git", plugin));
    };
    super::pluginruntime::stop(env, plugin);
    plugingit::pull(&folder, env.git_deadline).map_err(|_| Invalid::new("git", origin.as_str()))?;
    plugins::record_start(plugin, Ok(()));
    let fresh = plugins::find(env, plugin)?;
    if let Some(why) = fresh.plugin.failure.clone() {
        return Err(why);
    }
    if fresh.plugin.enabled {
        plugins::check_start(env, &fresh);
    }
    Ok((plugin.to_string(), fresh.plugin.manifest.version))
}
