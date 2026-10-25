// A locked Quickshell registry entry identifies a Flea shell without matching process names.
use std::ffi::OsString;
use std::fs;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[repr(C)]
struct Flock {
    kind: i16,
    whence: i16,
    start: i64,
    len: i64,
    pid: i32,
}

extern "C" {
    fn fcntl(fd: i32, command: i32, ...) -> i32;
}

pub(super) struct Instance {
    pub id: String,
    pub pid: i32,
    birth: String,
    prefix: Vec<OsString>,
    lock: fs::File,
}

fn birth(pid: i32) -> Option<String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let tail = stat.rsplit_once(") ")?.1;
    if matches!(tail.split_whitespace().next(), Some("Z" | "X")) {
        return None;
    }
    tail.split_whitespace().nth(19).map(str::to_string)
}

const F_GETLK: i32 = 5;

fn holder(file: &fs::File) -> Option<i32> {
    let mut lock = Flock {
        kind: 1,
        whence: 0,
        start: 0,
        len: 0,
        pid: 0,
    };
    if unsafe { fcntl(file.as_raw_fd(), F_GETLK, &mut lock as *mut Flock) } != 0 {
        return None;
    }
    (lock.kind != 2 && lock.pid > 1).then_some(lock.pid)
}

impl Instance {
    pub fn command(&self) -> Command {
        let mut command = Command::new(format!("/proc/{}/exe", self.pid));
        command.args(&self.prefix);
        command
    }

    pub fn alive(&self) -> bool {
        birth(self.pid).as_ref() == Some(&self.birth)
    }

    pub fn verified(&self) -> bool {
        self.alive() && holder(&self.lock) == Some(self.pid)
    }
}

fn owned(path: &Path, uid: u32) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && m.uid() == uid)
}

pub(super) fn find(runtime: &Path) -> Result<Vec<Instance>, String> {
    let base = runtime.join("quickshell");
    let uid = crate::backend::manifestdir::current_uid();
    let shell = base.join("by-shell/flea");
    let entries = match fs::read_dir(&shell) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("the old window registry could not be read".into()),
    };
    if !owned(runtime, uid)
        || !runtime.metadata().is_ok_and(|m| m.mode() & 0o077 == 0)
        || !owned(&base, uid)
        || !owned(&base.join("by-id"), uid)
    {
        return Err("the old window registry is not private to this user".into());
    }
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| "the old window registry could not be read")?;
        let Some(id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let dir = base.join("by-id").join(&id);
        if fs::read_link(entry.path()).ok().as_deref() != Some(dir.as_path()) || !owned(&dir, uid) {
            continue;
        }
        let lock_path = dir.join("instance.lock");
        let Ok(lock) =
            crate::backend::regfile::open_if_regular(&lock_path, crate::oflags::O_NOFOLLOW)
        else {
            continue;
        };
        if lock
            .metadata()
            .is_ok_and(|m| m.uid() != uid || m.len() == 0)
        {
            continue;
        }
        let Some(pid) = holder(&lock) else { continue };
        if fs::read_link(base.join("by-pid").join(pid.to_string()))
            .ok()
            .as_deref()
            != Some(dir.as_path())
        {
            continue;
        }
        let Some(birth) = birth(pid) else { continue };
        let proc = PathBuf::from(format!("/proc/{pid}"));
        if !proc.metadata().is_ok_and(|m| m.uid() == uid) {
            continue;
        }
        let args = fs::read(proc.join("cmdline"))
            .map_err(|_| "the old window command line could not be read")?;
        let args: Vec<&[u8]> = args.split(|b| *b == 0).filter(|s| !s.is_empty()).collect();
        let env = fs::read(proc.join("environ"))
            .map_err(|_| "the old window environment could not be read")?;
        let env: Vec<&[u8]> = env.split(|b| *b == 0).collect();
        let ui = env
            .iter()
            .find_map(|s| s.strip_prefix(b"FLEA_UI=" as &[u8]))
            .filter(|s| !s.is_empty())
            .unwrap_or(b"/usr/share/flea/ui");
        let mut entry = ui.to_vec();
        entry.extend_from_slice(b"/boot/shell.qml");
        let backend = env
            .iter()
            .any(|s| s.starts_with(b"FLEA_BIN=") && s.len() > 9);
        let position = args.windows(2).position(|a| a[0] == b"-p" && a[1] == entry);
        if !backend || position.is_none() || position == Some(0) {
            return Err("a live Flea registry entry has an unsupported process identity".into());
        }
        let instance = Instance {
            id,
            pid,
            birth,
            prefix: args[1..position.unwrap()]
                .iter()
                .map(|a| OsString::from_vec(a.to_vec()))
                .collect(),
            lock,
        };
        if instance.verified() {
            found.push(instance);
        }
        if found.len() > 16 {
            return Err("too many old windows are open to replace safely".into());
        }
    }
    found.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(found)
}
