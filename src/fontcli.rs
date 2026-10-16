// One short-lived helper per selected font; directory listing and thumbnail work never load fonts.
use crate::backend::md5;
use crate::json::escape;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MAX_BYTES: u64 = 64 * 1024 * 1024;
const O_NONBLOCK: i32 = 0x800;

pub fn dispatch(args: &[String]) -> Option<i32> {
    if args.get(1).map(String::as_str) != Some("--font") { return None; }
    let result = if args.get(2).map(String::as_str) == Some("axes") && args.len() == 5 {
        read_font(Path::new(&args[3])).and_then(|bytes| {
            let temp = snapshot(&std::env::temp_dir(), &bytes)?;
            crate::fontscan::axes::inspect(&temp.0, &args[4])
        })
    } else if args.get(2).map(String::as_str) == Some("defaults") && args.len() == 3 {
        crate::fontdefaults::run("status", "", "")
    } else if args.get(2).map(String::as_str) == Some("restore") && args.len() == 4 {
        crate::fontdefaults::run("restore", &args[3], "")
    } else if args.get(2).map(String::as_str) == Some("apply") && args.len() == 6 {
        apply_default(Path::new(&args[3]), &args[4], &args[5])
    } else if args.len() == 4 && ["info", "install", "uninstall"].contains(&args[2].as_str()) {
        manage(&args[2], Path::new(&args[3]))
    } else { Err("Use --font info|install|uninstall <path>, defaults, apply <path> <face-index> <destination>, or restore <destination>.".into()) };
    match result {
        Ok(line) => { println!("{line}"); Some(0) }
        Err(e) => { println!("{{\"error\":\"{}\"}}", escape(&e)); Some(2) }
    }
}

fn read_font(path: &Path) -> Result<Vec<u8>, String> {
    let file = OpenOptions::new().read(true).custom_flags(O_NONBLOCK).open(path).map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > MAX_BYTES { return Err("Choose a font file no larger than 64 MiB.".into()); }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES { return Err("The font exceeds 64 MiB.".into()); }
    Ok(bytes)
}

fn user_fonts() -> Result<PathBuf, String> {
    let data = std::env::var_os("XDG_DATA_HOME").filter(|p| Path::new(p).is_absolute())
        .map(PathBuf::from).or_else(|| std::env::var_os("HOME").filter(|p| Path::new(p).is_absolute()).map(|p| PathBuf::from(p).join(".local/share")))
        .ok_or("The user font directory is unavailable.")?;
    Ok(data.join("fonts"))
}

// Explicit user roots only. A symlink or a system file never becomes an uninstall target.
fn user_source(path: &Path, root: &Path) -> bool {
    if fs::symlink_metadata(path).map(|m| !m.is_file()).unwrap_or(true) { return false; }
    let Ok(real) = path.canonicalize() else { return false; };
    let mut roots = vec![root.to_path_buf()];
    if let Some(home) = std::env::var_os("HOME") { roots.push(PathBuf::from(home).join(".fonts")); }
    roots.iter().any(|r| r.canonicalize().map(|r| real.starts_with(r)).unwrap_or(false))
}

// The id names content, not a family: two styles or two versions cannot overwrite each other.
fn same_file(path: &Path, bytes: &[u8]) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.to_string()),
        Ok(m) if !m.is_file() || m.len() != bytes.len() as u64 => Err("A different file occupies the font's install location.".into()),
        Ok(_) => if read_font(path)? == bytes { Ok(true) }
            else { Err("A different file occupies the font's install location.".into()) },
    }
}

struct Temp(PathBuf);
impl Drop for Temp { fn drop(&mut self) { let _ = fs::remove_file(&self.0); } }

// A private snapshot is validated and linked atomically, so a changed source cannot install unvalidated bytes.
fn snapshot(dir: &Path, bytes: &[u8]) -> Result<Temp, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for n in 0..100 {
        let path = dir.join(format!(".flea-font-{}-{n}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path) {
            Ok(mut f) => {
                let temp = Temp(path);
                f.write_all(bytes).and_then(|_| f.sync_all()).map_err(|e| e.to_string())?;
                return Ok(temp);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Could not prepare the font file.".into())
}

fn refresh(dir: &Path) -> bool {
    Command::new("timeout").args(["20", "fc-cache", "--force", "--"]).arg(dir)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

fn manage(verb: &str, path: &Path) -> Result<String, String> {
    let source_is_link = fs::symlink_metadata(path).map(|m| m.file_type().is_symlink()).unwrap_or(false);
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let bytes = read_font(&path)?;
    let root = user_fonts()?;
    let dir = root.join("flea");

    // Inspection leaves the font installation untouched, including its directory and cache.
    let scratch = std::env::temp_dir();
    let temp = snapshot(if verb == "install" { &dir } else { &scratch }, &bytes)?;
    let font = crate::fontscan::inspect(&temp.0, &bytes)?;
    let target = dir.join(format!("{}.{}", md5::hex(&bytes), font.extension));
    let managed = same_file(&target, &bytes)?;
    let source_installed = !source_is_link && user_source(&path, &root);
    let source_is_target = target.canonicalize().ok().as_ref() == Some(&path);
    let mut installed = managed || source_installed;
    let mut cache_ok = true;
    match verb {
        "install" if !installed => {
            let created = match fs::hard_link(&temp.0, &target) {
                Ok(()) => true,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if !same_file(&target, &bytes)? { return Err("The font installation changed; try again.".into()); }
                    false
                },
                Err(e) => return Err(e.to_string()),
            };
            drop(temp);
            installed = true;
            cache_ok = refresh(&root);
            if !crate::fontscan::discoverable(&target) {
                if created {
                    fs::remove_file(&target).map_err(|e| e.to_string())?;
                    refresh(&root);
                    return Err("Linux did not make this font available to desktop applications. The installation was rolled back; your original file is unchanged.".into());
                }
                return Err("Linux does not expose this existing font to desktop applications; the existing file was left unchanged.".into());
            }
        }
        "install" => {
            if !crate::fontscan::discoverable(if source_installed { &path } else { &target }) {
                return Err("Linux does not expose this existing font to desktop applications; the existing file was left unchanged.".into());
            }
        }
        "uninstall" => {
            // Never remove the download. When browsing an installed user font, that file is the explicit subject.
            if managed { fs::remove_file(&target).map_err(|e| e.to_string())?; }
            if source_installed && !source_is_target { fs::remove_file(&path).map_err(|e| e.to_string())?; }
            if !installed { return Err("This font is not installed for this user through Flea or this location.".into()); }
            installed = false;
            cache_ok = refresh(&root);
            if let Some(parent) = path.parent().filter(|_| source_installed && !path.starts_with(&root)) {
                cache_ok = refresh(parent) && cache_ok;
            }
        }
        _ => {},
    }
    let desktop_available = crate::fontscan::discoverable(&path) || (installed && crate::fontscan::discoverable(&target));
    Ok(format!("{{\"family\":\"{}\",\"style\":\"{}\",\"installed\":{},\"cacheOk\":{},\"sourceRemoved\":{},\"location\":\"{}\",\"desktopAvailable\":{},\"format\":\"{}\",\"faces\":{},\"axes\":{}}}",
        escape(&font.faces[0].family), escape(&font.faces[0].style), installed, cache_ok, verb == "uninstall" && source_installed,
        escape(&if source_installed { path } else { target }.to_string_lossy()),
        desktop_available, escape(&font.format), font.faces_json(), font.axes.trim()))
}

fn apply_default(path: &Path, index: &str, target: &str) -> Result<String, String> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let bytes = read_font(&path)?;
    let temp = snapshot(&std::env::temp_dir(), &bytes)?;
    let font = crate::fontscan::inspect(&temp.0, &bytes)?;
    let index: u32 = index.parse().map_err(|_| "Choose a valid font face.")?;
    let face = font.faces.iter().find(|f| f.index == index).ok_or("Choose a valid font face.")?;
    let installed = user_fonts()?.join("flea").join(format!("{}.{}", md5::hex(&bytes), font.extension));
    if !crate::fontscan::discoverable(&path) && !(same_file(&installed, &bytes)? && crate::fontscan::discoverable(&installed)) {
        return Err("Install this font before using it as a default.".into());
    }
    if target != "desktop" && target != "serif" && !face.monospace {
        return Err("Choose a monospaced font for this destination.".into());
    }
    crate::fontdefaults::run("apply", target, &face.family)
}
