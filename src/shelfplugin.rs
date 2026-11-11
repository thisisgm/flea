// B1, as the design states it: "Enable shelf checked in Settings = shelf in bar automatically,
// unchecked = no shelf in bar". The shelf is a separate Omarchy plugin, so that switch is what
// installs it. The package ships the plugin's files as data beside the UI; turning the switch on
// copies them into the user's own plugin directory and asks Omarchy to enable it, turning it off
// asks Omarchy to disable it. Every bar action is a stock omarchy command, hard rule 2.

use crate::json;
use crate::userfile;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const ID: &str = "io.github.thisisgm.flea-shelf";
// A plugin manifest is a few hundred bytes, so a larger one is not the shelf's.
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

// An empty FLEA_SHELF would resolve manifest.json against the working directory, so it is no
// candidate, the same rule paths.rs applies to FLEA_UI.
fn env_source() -> Option<PathBuf> {
    let value = std::env::var("FLEA_SHELF").ok()?;
    if value.is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

// The plugin ships as data, so it is found the way paths::ui_dir finds the UI.
pub fn source() -> Option<PathBuf> {
    if let Some(p) = env_source() {
        if p.join("manifest.json").is_file() {
            return Some(p);
        }
    }
    let packaged = PathBuf::from("/usr/share/flea/shelf");
    if packaged.join("manifest.json").is_file() {
        return Some(packaged);
    }
    let exe = std::env::current_exe().ok()?;
    let dev = exe.parent()?.parent()?.parent()?.join("shelf");
    if dev.join("manifest.json").is_file() {
        return Some(dev);
    }
    None
}

// Where Omarchy looks for a user's own plugins, one folder per id.
pub fn installed() -> Result<PathBuf, String> {
    Ok(userfile::config_home()?.join("omarchy").join("plugins").join(ID))
}

// Sample manifest, the two keys read here being the only ones this file cares about:
// {"schemaVersion": 1, "id": "io.github.thisisgm.flea-shelf", "version": "0.3.0", ...}
fn version(dir: &Path) -> Option<String> {
    // Read before the window, so a fifo in its place must not block the launch.
    let text = crate::backend::regfile::read_text(&dir.join("manifest.json"), MAX_MANIFEST_BYTES).ok()?;
    json::field_str(&text, "version")
}

// Only the two manifests are read before the window; each PKGBUILD stamps $pkgver-$pkgrel, so every build replaces the copy.
fn current(dest: &Path, source: &Path) -> bool {
    crate::persistdoc::read(&dest.join("manifest.json"), manifest_shape).is_ok()
        && version(dest) == version(source)
}

// Files and never a symlink: a link into /usr/share would leave the plugin dead after an upgrade
// replaced it, and `omarchy plugin remove` expects a folder it can delete.
fn install(source: &Path, dest: &Path) -> Result<(), String> {
    crate::persistdoc::read(&dest.join("manifest.json"), manifest_shape)?;
    std::fs::create_dir_all(dest).map_err(|e| format!("{} could not be created ({})", dest.display(), e))?;
    for entry in std::fs::read_dir(source).map_err(|e| format!("{} could not be read ({})", source.display(), e))? {
        let entry = entry.map_err(|e| format!("{} could not be read ({})", source.display(), e))?;
        let from = entry.path();
        if from.is_dir() {
            return Err(format!("the shelf plugin holds a folder, {}, and this installer copies a flat one", from.display()));
        }
        let to = dest.join(entry.file_name());
        std::fs::copy(&from, &to).map_err(|e| format!("{} could not be copied ({})", from.display(), e))?;
    }
    // A file this Flea no longer ships would otherwise outlive the upgrade that dropped it.
    for entry in std::fs::read_dir(dest).map_err(|e| format!("{} could not be read ({})", dest.display(), e))? {
        let entry = entry.map_err(|e| format!("{} could not be read ({})", dest.display(), e))?;
        if !source.join(entry.file_name()).exists() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    Ok(())
}

fn manifest_shape(doc: &crate::jsondoc::Json) -> bool {
    use crate::jsondoc::Json;
    doc.get("schemaVersion").and_then(Json::as_f64) == Some(1.0)
        && doc.get("id").and_then(Json::as_str) == Some(ID)
        && doc.get("version").and_then(Json::as_str).is_some()
}

// omarchy's own subcommands resolve their helpers through OMARCHY_PATH, which a session that did not
// come from a login shell does not carry.
fn omarchy(args: &[&str]) -> Result<(), String> {
    let mut command = Command::new("omarchy");
    command.args(args);
    if std::env::var("OMARCHY_PATH").map(|v| v.is_empty()).unwrap_or(true) {
        command.env("OMARCHY_PATH", "/usr/share/omarchy");
    }
    let out = command.output().map_err(|e| format!("omarchy {} could not run ({})", args.join(" "), e))?;
    if out.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&out.stderr);
    let first = said.lines().find(|l| !l.trim().is_empty()).unwrap_or("it said nothing").trim().to_string();
    Err(format!("omarchy {} refused: {}", args.join(" "), first))
}

// The catalog knows the folders it has scanned, so a plugin copied in this second is unknown to
// `omarchy plugin enable` until the shell has been asked to look again. This is the stock fallback
// every omarchy-plugin-* command names in its own failure message.
fn rescan() {
    let _ = Command::new("omarchy-shell").args(["shell", "rescanPlugins"]).output();
}

// The switch itself, both ways.
pub fn sync(enabled: bool) -> Result<(), String> {
    let dest = installed()?;
    if !enabled {
        // Never installed here means nothing to disable, and asking would only print a refusal.
        if !dest.join("manifest.json").is_file() {
            return Ok(());
        }
        return omarchy(&["plugin", "disable", ID]);
    }
    let source = source().ok_or_else(|| "the shelf plugin is not installed, look for /usr/share/flea/shelf".to_string())?;
    if !current(&dest, &source) {
        install(&source, &dest)?;
    }
    rescan();
    omarchy(&["plugin", "enable", ID])
}

// Startup, where the copy is refreshed after an upgrade shipped a new plugin under an old switch.
// A switch that is off owns no copy, so there is nothing to carry forward.
pub fn refresh(enabled: bool) -> Result<(), String> {
    if !enabled {
        return Ok(());
    }
    let (dest, source) = (installed()?, source());
    let source = match source {
        Some(source) => source,
        None => return Ok(()),
    };
    if current(&dest, &source) {
        return Ok(());
    }
    sync(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str, version: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("flea-shelfplugin-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the fixture root");
        std::fs::write(dir.join("manifest.json"),
                       format!("{{\n  \"schemaVersion\": 1,\n  \"id\": \"{}\",\n  \"version\": \"{}\"\n}}\n", ID, version)).expect("a manifest");
        std::fs::write(dir.join("Panel.qml"), "Item {}\n").expect("a panel");
        dir
    }

    #[test]
    fn the_manifest_names_the_version_and_not_the_schema_s() {
        let dir = fixture("version", "0.3.0");
        assert_eq!(version(&dir).as_deref(), Some("0.3.0"));
        assert_eq!(version(&dir.join("nowhere")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A release bump that forgets the shelf, or reshapes the line every PKGBUILD stamps, fails here.
    #[test]
    fn the_shelf_ships_under_the_flea_version_on_the_line_the_packages_stamp() {
        let manifest = include_str!("../shelf/manifest.json");
        assert_eq!(json::field_str(manifest, "version").as_deref(), Some(env!("CARGO_PKG_VERSION")),
                   "shelf/manifest.json must say Cargo.toml's version");
        let line = format!("\n  \"version\": \"{}\",\n", env!("CARGO_PKG_VERSION"));
        assert_eq!(manifest.matches(line.as_str()).count(), 1, "the PKGBUILD stamp rewrites one top-level version line");
    }

    // Flea through 0.3.8 shipped a changing shelf as 0.3.1, so the copy one of them left must not read as current.
    #[test]
    fn a_copy_an_older_flea_left_is_replaced_by_this_tree_s_shelf() {
        let tree = Path::new(env!("CARGO_MANIFEST_DIR")).join("shelf");
        let dest = fixture("older", "0.3.1");
        assert!(!current(&dest, &tree), "the copy an older Flea left reads as current");
        install(&tree, &dest).expect("the copy");
        assert!(current(&dest, &tree));
        assert_eq!(std::fs::read(dest.join("Panel.qml")).ok(), std::fs::read(tree.join("Panel.qml")).ok());
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn an_install_leaves_the_copy_and_drops_what_this_flea_no_longer_ships() {
        let source = fixture("source", "0.3.0");
        let dest = fixture("dest", "0.2.9");
        std::fs::write(dest.join("Gone.qml"), "Item {}\n").expect("a file from the older plugin");
        install(&source, &dest).expect("the copy");
        assert_eq!(version(&dest).as_deref(), Some("0.3.0"));
        assert!(dest.join("Panel.qml").is_file());
        assert!(!dest.join("Gone.qml").exists(), "the older plugin's file outlived the upgrade");
        assert!(!dest.join("Panel.qml").symlink_metadata().expect("the copy").file_type().is_symlink());
        let _ = std::fs::remove_dir_all(&source);
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn a_folder_inside_the_plugin_is_refused_rather_than_silently_dropped() {
        let source = fixture("nested", "0.3.0");
        std::fs::create_dir_all(source.join("inner")).expect("a folder");
        let dest = fixture("nested-dest", "0.2.9");
        let said = install(&source, &dest).expect_err("a flat installer must say so");
        assert!(said.contains("flat one"), "{}", said);
        let _ = std::fs::remove_dir_all(&source);
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn the_plugin_lands_where_omarchy_looks_for_a_user_s_own() {
        let path = installed().expect("a config directory");
        assert!(path.ends_with("omarchy/plugins/io.github.thisisgm.flea-shelf"), "{}", path.display());
    }
}
