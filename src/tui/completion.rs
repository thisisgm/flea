use crate::jsondoc::Json;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
    Arc,
};

pub struct Completion {
    places: Vec<(String, String)>,
    home: PathBuf,
    pending: Option<(String, PathBuf)>,
    running: Option<(Arc<AtomicBool>, Receiver<String>)>,
}
impl Completion {
    pub fn new(settings: &Json) -> Self {
        let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
        Self {
            places: places(settings, &home),
            home,
            pending: None,
            running: None,
        }
    }
    pub fn request(&mut self, value: &str, current: &Path) -> String {
        if let Some((cancel, _)) = &self.running {
            cancel.store(true, Ordering::Relaxed);
        }
        self.pending = None;
        if value.is_empty() {
            return String::new();
        }
        if value == "~" {
            return "~/".into();
        }
        if value == "." || value == ".." {
            return format!("{}/", value);
        }
        if let Some(path) = place(value, &self.places) {
            return path;
        }
        self.pending = Some((value.into(), current.into()));
        self.start();
        String::new()
    }
    fn start(&mut self) {
        if self.running.is_some() {
            return;
        }
        let Some((value, current)) = self.pending.take() else {
            return;
        };
        let home = self.home.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let stopped = cancel.clone();
        let (tx, result) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(directory(&value, &current, &home, &stopped));
        });
        self.running = Some((cancel, result));
    }
    pub fn poll(&mut self) -> Option<String> {
        let (cancel, receiver) = self.running.as_ref()?;
        let value = match receiver.try_recv() {
            Ok(value) => value,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => String::new(),
        };
        let accepted = !cancel.load(Ordering::Relaxed);
        self.running = None;
        self.start();
        accepted.then_some(value)
    }
}
impl Drop for Completion {
    fn drop(&mut self) {
        if let Some((cancel, _)) = &self.running {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

pub fn expand(value: &str, home: &Path, current: &Path) -> PathBuf {
    if value == "~" {
        home.into()
    } else if let Some(tail) = value.strip_prefix("~/") {
        home.join(tail)
    } else {
        current.join(value)
    }
}

fn places(settings: &Json, home: &Path) -> Vec<(String, String)> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    let dirs = std::fs::read_to_string(config.join("user-dirs.dirs")).unwrap_or_default();
    let mut places = vec![("Home".into(), home.to_string_lossy().into_owned())];
    // Sample input: XDG_DOWNLOAD_DIR="$HOME/Downloads"; this file is data, never shell code.
    for line in dirs.lines() {
        let Some((key, path)) = line.trim().split_once('=') else {
            continue;
        };
        if !key.starts_with("XDG_") || !key.ends_with("_DIR") {
            continue;
        }
        let Some(path) = path
            .trim()
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
        else {
            continue;
        };
        let path = path.replace("$HOME", &home.to_string_lossy());
        if Path::new(&path).is_absolute() {
            places.push((
                Path::new(&path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                path,
            ));
        }
    }
    if let Some(records) = settings
        .get("places")
        .and_then(|p| p.get("favourites"))
        .and_then(Json::as_array)
    {
        for record in records {
            let label = record.get("label").and_then(Json::as_str).unwrap_or("");
            let path = record.get("path").and_then(Json::as_str).unwrap_or("");
            if !label.is_empty() && (path.starts_with('/') || path == "~" || path.starts_with("~/"))
            {
                places.push((label.into(), path.into()));
            }
        }
    }
    places
}

fn place(value: &str, places: &[(String, String)]) -> Option<String> {
    for (label, path) in places {
        if label.to_lowercase().starts_with(&value.to_lowercase()) || path.starts_with(value) {
            return Some(path.clone());
        }
    }
    None
}
fn directory(value: &str, current: &Path, home: &Path, cancel: &AtomicBool) -> String {
    if cancel.load(Ordering::Relaxed) {
        return String::new();
    }
    let expanded = expand(value, home, current);
    let (parent, prefix) = if value.ends_with('/') {
        (expanded.as_path(), "")
    } else {
        (
            expanded.parent().unwrap_or(current),
            expanded.file_name().and_then(|n| n.to_str()).unwrap_or(""),
        )
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return String::new();
    };
    let mut first: Option<String> = None;
    for entry in entries {
        if cancel.load(Ordering::Relaxed) {
            return String::new();
        }
        let Ok(entry) = entry else { continue };
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with(prefix)
            && first.as_ref().map_or(true, |held| name < *held)
            && entry.file_type().is_ok_and(|t| t.is_dir())
        {
            first = Some(name);
        }
    }
    let Some(name) = first else {
        return String::new();
    };
    format!("{}{}/", &value[..value.len() - prefix.len()], name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn completion_follows_prefix_without_importing_or_rewriting_places() {
        let root = TestDir::new("tui-completion");
        std::fs::create_dir(root.join("Documents")).unwrap();
        std::fs::create_dir(root.join("Downloads")).unwrap();
        root.file("Door.txt", "not a directory");
        let cancel = AtomicBool::new(false);
        assert_eq!(
            directory("Do", root.path(), root.path(), &cancel),
            "Documents/"
        );
        assert_eq!(
            directory("Dow", root.path(), root.path(), &cancel),
            "Downloads/"
        );
        assert_eq!(directory("missing/", root.path(), root.path(), &cancel), "");
        assert_eq!(
            expand("~/Documents", root.path(), Path::new("/")),
            root.join("Documents")
        );
        assert_eq!(
            place("w", &[("Work".into(), "/data/work".into())]),
            Some("/data/work".into())
        );
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(directory("Do", root.path(), root.path(), &cancel), "");
    }
    #[test]
    fn a_replaced_completion_cannot_land_on_the_new_query() {
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, receiver) = mpsc::channel();
        let mut completion = Completion {
            places: vec![("Home".into(), "/home/test".into())],
            home: "/home/test".into(),
            pending: None,
            running: Some((cancel.clone(), receiver)),
        };
        assert_eq!(
            completion.request("Home", Path::new("/fixture")),
            "/home/test"
        );
        assert!(cancel.load(Ordering::Relaxed));
        tx.send("old-directory/".into()).unwrap();
        assert!(completion.poll().is_none());
        assert!(completion.running.is_none());
    }
}
