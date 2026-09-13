// Application discovery uses the same GIO registry that launches the selected desktop entry.
use super::trashmanifest::Cancellation;
use std::ffi::OsStr;
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};

// Registry output is metadata; an oversized response is refused instead of allocating without a bound.
const MAX_REGISTRY_BYTES: u64 = 1024 * 1024;
const SIGKILL: i32 = 9;
const ESRCH: i32 = 3;
const EINTR: i32 = 4;
const POLLIN: i16 = 1;
#[repr(C)]
struct PollFd { fd: i32, events: i16, revents: i16 }
extern "C" {
    fn pidfd_open(pid: i32, flags: u32) -> i32;
    fn kill(pid: i32, signal: i32) -> i32;
    fn poll(fds: *mut PollFd, count: usize, timeout: i32) -> i32;
}
struct Running { pid: i32, pidfd: OwnedFd, group: bool }

#[derive(Clone, Default)]
pub(crate) struct Registry { running: Arc<Mutex<Option<Running>>>,
                             icons: Arc<Mutex<Option<Arc<HashMap<String, PathBuf>>>>> }

impl Registry {
    pub fn cancel(&self) -> Result<(), String> {
        let running = self.running.lock().map_err(|_| "The application query service stopped.")?;
        if let Some(child) = running.as_ref() {
            // Reaping also holds this lock, so cancellation cannot target a reused PID or process group.
            terminate(child.pid, child.group)?;
        }
        Ok(())
    }

    fn query(&self, args: &[&OsStr], cancel: &Cancellation) -> Result<String, String> {
        let mut command = Command::new("gio");
        command.args(args).env("LC_ALL", "C");
        self.capture(&mut command, cancel)
    }

    fn capture(&self, command: &mut Command, cancel: &Cancellation) -> Result<String, String> {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut owned = OwnedChild::start(command, self, cancel, true)?;
        let child = owned.child.as_mut().unwrap();
        let output = reader(child.stdout.take().unwrap(), self.clone())?;
        let errors = match reader(child.stderr.take().unwrap(), self.clone()) {
            Ok(reader) => reader,
            Err(error) => { drop(owned); let _ = output.join(); return Err(error); }
        };
        let output = match output.join() {
            Ok(result) => result,
            Err(_) => { drop(owned); let _ = errors.join(); return Err("GIO output reader stopped.".into()); }
        };
        let errors = errors.join().map_err(|_| "GIO error reader stopped.".to_string())?;
        let status = owned.wait()?;
        cancelled(cancel)?;
        let stdout = output?;
        let stderr = errors?;
        if !status.success() {
            let detail = String::from_utf8_lossy(&stderr);
            return Err(format!("GIO application query failed: {}.", detail.lines().last().unwrap_or("no diagnostic")));
        }
        String::from_utf8(stdout).map_err(|_| "GIO returned invalid application registry text.".into())
    }

    // The name an entry's Icon= resolves to, following /usr/share/omarchy/shell/services/AppLibrary.qml:
    // the app and device icon directories answer first, because an unconstrained themed lookup can
    // resolve a name such as "zoom" to an action icon instead. A name the index cannot place is
    // returned as it came, for the themed lookup ui/MenuRow.qml does with it.
    fn icon(&self, name: &str, cancel: &Cancellation) -> Result<String, String> {
        if name.is_empty() || name.starts_with('/') { return Ok(name.into()); }
        let index = self.icon_index(cancel)?;
        Ok(index.get(name).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| name.into()))
    }

    // Scanned once per process, the way that service scans once per shell start.
    fn icon_index(&self, cancel: &Cancellation) -> Result<Arc<HashMap<String, PathBuf>>, String> {
        let mut held = self.icons.lock().map_err(|_| "The icon index service stopped.")?;
        if let Some(index) = held.as_ref() { return Ok(index.clone()); }
        let index = Arc::new(icon_index(cancel)?);
        *held = Some(index.clone());
        Ok(index)
    }

    pub fn launch(&self, desktop: &Path, path: &Path, cancel: &Cancellation) -> Result<(), String> {
        const PR_SET_THP_DISABLE: i32 = 41;
        extern "C" { fn prctl(option: i32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> i32; }
        let mut command = Command::new("gio");
        command.arg("launch").arg(desktop).arg(path);
        // Foreign applications retain the released THP policy; this child hook cannot allocate.
        unsafe { command.pre_exec(|| if prctl(PR_SET_THP_DISABLE, 0, 0, 0, 0) == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }); }
        self.launch_command(&mut command, cancel)
            .map_err(|error| format!("Could not open {} with {}: {}", path.display(), desktop.display(), error))
    }

    fn launch_command(&self, command: &mut Command, cancel: &Cancellation) -> Result<(), String> {
        // Applications can inherit launcher descriptors and its group, so neither pipes nor group cancellation are appropriate here.
        command.stdout(Stdio::null()).stderr(Stdio::null());
        let status = OwnedChild::start(command, self, cancel, false)?.wait()?;
        cancelled(cancel)?;
        if status.success() { Ok(()) } else { Err(format!("GIO application launcher exited with {}.", status)) }
    }
}

struct OwnedChild { child: Option<Child>, registry: Registry }

impl OwnedChild {
    fn start(command: &mut Command, registry: &Registry, cancel: &Cancellation, group: bool) -> Result<Self, String> {
        cancelled(cancel)?;
        let mut child = command.stdin(Stdio::null()).process_group(0).spawn()
            .map_err(|e| format!("Could not start GIO application query or launcher: {}.", e))?;
        let raw = unsafe { pidfd_open(child.id() as i32, 0) };
        if raw < 0 {
            let mut error = format!("Could not observe GIO application child: {}.", std::io::Error::last_os_error());
            if let Err(cleanup) = terminate(child.id() as i32, group) { error.push_str(&format!(" {}", cleanup)); }
            if let Err(cleanup) = child.wait() { error.push_str(&format!(" Could not reap it: {}.", cleanup)); }
            return Err(error);
        }
        *registry.running.lock().unwrap() = Some(Running { pid: child.id() as i32, pidfd: unsafe { OwnedFd::from_raw_fd(raw) }, group });
        let owned = Self { child: Some(child), registry: registry.clone() };
        cancelled(cancel)?;
        Ok(owned)
    }

    // One Registry runs one child at a time: MenuActions drives a single worker thread and every
    // query and launch goes through it, so the slot this reads is always this child's own.
    fn wait(mut self) -> Result<ExitStatus, String> {
        let descriptor = self.registry.running.lock().unwrap().as_ref().unwrap().pidfd.as_raw_fd();
        wait_exit(descriptor)?;
        self.reap()
    }

    fn reap(&mut self) -> Result<ExitStatus, String> {
        let mut running = self.registry.running.lock().unwrap();
        let status = self.child.take().unwrap().wait();
        running.take();
        status.map_err(|e| format!("Could not reap GIO application child: {}.", e))
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.child.is_none() { return; }
        if let Err(error) = self.registry.cancel() { eprintln!("flea: {}", error); }
        if let Err(error) = self.reap() { eprintln!("flea: {}", error); }
    }
}

fn terminate(pid: i32, group: bool) -> Result<(), String> {
    if unsafe { kill(if group { -pid } else { pid }, SIGKILL) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(ESRCH) { return Err(format!("Could not cancel owned GIO child: {}.", error)); }
    }
    Ok(())
}

fn cancelled(cancel: &Cancellation) -> Result<(), String> {
    cancel.check().map_err(|_| "Application query cancelled.".into())
}

fn reader(pipe: impl Read + Send + 'static, registry: Registry) -> Result<std::thread::JoinHandle<Result<Vec<u8>, String>>, String> {
    std::thread::Builder::new().spawn(move || {
        let mut bytes = Vec::new();
        if let Err(error) = pipe.take(MAX_REGISTRY_BYTES + 1).read_to_end(&mut bytes) {
            registry.cancel()?;
            return Err(format!("Could not read GIO application query: {}.", error));
        }
        if bytes.len() as u64 > MAX_REGISTRY_BYTES {
            registry.cancel()?;
            return Err(format!("GIO application registry response exceeds {} bytes.", MAX_REGISTRY_BYTES));
        }
        Ok(bytes)
    }).map_err(|e| format!("Could not start GIO output reader: {}.", e))
}

fn wait_exit(fd: i32) -> Result<(), String> {
    loop {
        let mut descriptor = PollFd { fd, events: POLLIN, revents: 0 };
        let result = unsafe { poll(&mut descriptor, 1, -1) };
        if result > 0 && descriptor.revents & POLLIN != 0 { return Ok(()); }
        let error = std::io::Error::last_os_error();
        if result < 0 && error.raw_os_error() == Some(EINTR) { continue; }
        return Err(format!("Could not wait for GIO application query: {}.", error));
    }
}

pub(crate) struct Application { pub id: String, pub label: String, pub path: PathBuf,
                               pub icon: String, pub default: bool }

// The flyout's registry, plus the whole installed set and the kind's own name that the dialog groups by.
pub(crate) struct Catalogue { pub mime: String, pub kind: String,
                              pub handlers: Vec<Application>, pub installed: Vec<Application> }

pub(crate) fn catalogue(registry: &Registry, path: &Path, whole: bool, cancel: &Cancellation) -> Result<Catalogue, String> {
    let mime = content_type(registry, path, cancel)?;
    // The flyout draws the registry alone, and it is asked for on every row menu that opens. Walking
    // every applications directory for a dialog nobody opened cost that menu a scan it never drew.
    let installed = if whole { installed(registry, cancel)? } else { Vec::new() };
    Ok(Catalogue { kind: describe(&mime, cancel)?, handlers: handlers(registry, &mime, cancel)?, installed, mime })
}

pub(crate) fn content_type(registry: &Registry, path: &Path, cancel: &Cancellation) -> Result<String, String> {
    // Symlinks are followed here, unlike the listing's own lookup: Open with acts on the file the
    // user opens, and typing the link instead reports inode/symlink, which nothing can open.
    let info = registry.query(&["info".as_ref(), "--attributes=standard::content-type".as_ref(), path.as_os_str()], cancel)?;
    // Sample GIO info attribute: "  standard::content-type: text/plain".
    info.lines().find_map(|line| line.trim().strip_prefix("standard::content-type: "))
        .filter(|m| !m.is_empty()).map(str::to_string)
        .ok_or_else(|| "GIO did not report the selected item's content type.".into())
}

// The desktop's own default handler, written the one way the desktop reads it back.
pub(crate) fn set_default(registry: &Registry, mime: &str, id: &str, cancel: &Cancellation) -> Result<(), String> {
    registry.query(&["mime".as_ref(), mime.as_ref(), id.as_ref()], cancel)
        .map(|_| ())
        .map_err(|error| format!("Could not make {} the default for {}: {}", id, mime, error))
}

// The applications the registry names for one type, the desktop's own default at the head.
pub(crate) fn handlers(registry: &Registry, mime: &str, cancel: &Cancellation) -> Result<Vec<Application>, String> {
    let output = registry.query(&["mime".as_ref(), mime.as_ref()], cancel)?;
    // Sample GIO mime output: the default is named on its own line before the indented registry.
    // "Default application for \u{201c}text/plain\u{201d}: org.gnome.TextEditor.desktop".
    let default = output
        .lines()
        .find(|line| line.starts_with("Default application"))
        .and_then(|line| line.rsplit(':').next())
        .map(str::trim)
        .filter(|id| id.ends_with(".desktop"))
        .unwrap_or_default()
        .to_string();
    let mut apps: Vec<Application> = Vec::new();
    // Sample GIO mime registry row: "\torg.gnome.TextEditor.desktop".
    for line in output.lines().filter(|line| line.starts_with('\t') || line.starts_with("  ")) {
        cancelled(cancel)?;
        let id = line.trim();
        if !id.ends_with(".desktop") || id.contains('/') || id.contains('\0') || apps.iter().any(|a: &Application| a.id == id) { continue; }
        if let Some(path) = desktop_file(id, cancel)? {
            if let Some(mut app) = launchable(id, &path)? {
                app.icon = registry.icon(&app.icon, cancel)?;
                app.default = id == default;
                apps.push(app);
            }
        }
    }
    // OpenWith.html: the desktop's current default is first, and the registry order follows it.
    if let Some(at) = apps.iter().position(|a| a.default) {
        let head = apps.remove(at);
        apps.insert(0, head);
    }
    Ok(apps)
}

// The dialog can open with an application the registry never named for this type, so an id resolves
// through the data roots the desktop itself searches rather than through one type's handler list.
pub(crate) fn resolve(id: &str, cancel: &Cancellation) -> Result<Application, String> {
    if !id.ends_with(".desktop") || id.contains('/') || id.contains('\0') {
        return Err("That application id is not a desktop entry.".into());
    }
    let path = desktop_file(id, cancel)?.ok_or("That application is no longer installed.")?;
    launchable(id, &path)?.ok_or_else(|| "That application cannot open a file.".into())
}

// The XDG data roots in search order, the user's own first, which every lookup below walks.
fn data_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("XDG_DATA_HOME").filter(|p| !p.is_empty()) {
        roots.push(PathBuf::from(home));
    } else if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".local/share"));
    }
    roots.extend(std::env::split_paths(&std::env::var_os("XDG_DATA_DIRS").filter(|p| !p.is_empty()).unwrap_or_else(|| "/usr/local/share:/usr/share".into())));
    roots.retain(|root| root.is_absolute());
    roots
}

// The same roots and the same shape AppLibrary.qml's own scan walks: every apps/ and devices/ icon
// under the XDG icon directories, plus /usr/share/pixmaps' own top level. That scan runs an SVG pass
// over every root before its PNG pass, so a scalable icon in any root beats a raster one in an
// earlier root, and within a pass the filesystem's own order decides. Both are copied deliberately:
// this index exists to answer the way the Omarchy menu answers, not to improve on it.
fn icon_index(cancel: &Cancellation) -> Result<HashMap<String, PathBuf>, String> {
    let mut svg: HashMap<String, PathBuf> = HashMap::new();
    let mut png: HashMap<String, PathBuf> = HashMap::new();
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME") { roots.push(PathBuf::from(home).join(".icons")); }
    roots.extend(data_roots().into_iter().map(|root| root.join("icons")));
    for root in roots {
        cancelled(cancel)?;
        let mut pending = vec![root];
        while let Some(at) = pending.pop() {
            cancelled(cancel)?;
            let Ok(entries) = std::fs::read_dir(at) else { continue; };
            for entry in entries.flatten() {
                let path = entry.path();
                match entry.file_type() {
                    Ok(kind) if kind.is_dir() => pending.push(path),
                    // A device icon is what entries such as Print Settings name instead of an app icon.
                    Ok(_) if in_group(&path, "apps") || in_group(&path, "devices") => index_icon(&path, &mut svg, &mut png),
                    _ => (),
                }
            }
        }
    }
    // Pixmaps has no theme layout, so only its own level is an icon directory.
    if let Ok(entries) = std::fs::read_dir("/usr/share/pixmaps") {
        for entry in entries.flatten() {
            cancelled(cancel)?;
            if entry.file_type().map(|kind| kind.is_file()).unwrap_or(false) { index_icon(&entry.path(), &mut svg, &mut png); }
        }
    }
    for (name, path) in png { svg.entry(name).or_insert(path); }
    Ok(svg)
}

fn index_icon(path: &Path, svg: &mut HashMap<String, PathBuf>, png: &mut HashMap<String, PathBuf>) {
    let Some(name) = path.file_stem().map(|stem| stem.to_string_lossy().to_string()).filter(|name| !name.is_empty()) else { return; };
    let target = match path.extension().and_then(|ext| ext.to_str()) {
        Some("svg") => svg,
        Some("png") => png,
        _ => return,
    };
    target.entry(name).or_insert_with(|| path.to_path_buf());
}

// The icon's own directory, not any ancestor: /opt/apps/share/icons/.../mimetypes matched "apps"
// through its prefix and let a whole non-application category win names in the index.
fn in_group(path: &Path, group: &str) -> bool {
    path.parent().and_then(Path::file_name).map(|dir| dir == group).unwrap_or(false)
}

// OpenWith.html's ALL APPLICATIONS group, alphabetical: every entry that declares itself an
// application, is not hidden, and names a command. OnlyShowIn, NotShowIn and TryExec are not read,
// so an entry another desktop scopes to itself is listed here.
fn installed(registry: &Registry, cancel: &Cancellation) -> Result<Vec<Application>, String> {
    let mut apps: Vec<Application> = Vec::new();
    for root in data_roots() {
        let dir = root.join("applications");
        let mut pending = vec![dir.clone()];
        while let Some(at) = pending.pop() {
            cancelled(cancel)?;
            let Ok(entries) = std::fs::read_dir(at) else { continue; };
            for entry in entries.flatten() {
                cancelled(cancel)?;
                let path = entry.path();
                let Ok(kind) = entry.file_type() else { continue; };
                if kind.is_dir() { pending.push(path); continue; }
                if path.extension() != Some(OsStr::new("desktop")) { continue; }
                let Ok(id) = path.strip_prefix(&dir) else { continue; };
                let id = id.to_string_lossy().replace('/', "-");
                // The first root that names an id owns it, the way the desktop resolves one itself.
                if apps.iter().any(|a| a.id == id) { continue; }
                // A dangling symlink or an unreadable entry is skipped, not fatal: one of them in
                // any applications directory used to take Open with away from every file on the box.
                let Ok(Some(mut app)) = launchable(&id, &path) else { continue; };
                app.icon = registry.icon(&app.icon, cancel)?;
                apps.push(app);
            }
        }
    }
    apps.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    Ok(apps)
}

// A desktop entry the user can open something with: an application, shown, and with a command to run.
fn launchable(id: &str, path: &Path) -> Result<Option<Application>, String> {
    if desktop_key(path, "Type=")?.as_deref() != Some("Application") { return Ok(None); }
    if desktop_key(path, "NoDisplay=")?.as_deref() == Some("true") { return Ok(None); }
    if desktop_key(path, "Hidden=")?.as_deref() == Some("true") { return Ok(None); }
    if desktop_key(path, "Exec=")?.is_none() { return Ok(None); }
    let label = desktop_label(path)?.unwrap_or_else(|| id.trim_end_matches(".desktop").into());
    Ok(Some(Application { id: id.into(), label, path: path.to_path_buf(),
                          icon: desktop_key(path, "Icon=")?.unwrap_or_default(), default: false }))
}

// The kind's own name, from the shared mime database the desktop describes a type with.
// Sample row: "  <comment>PNG image</comment>"; the translated siblings carry an xml:lang attribute.
fn describe(mime: &str, cancel: &Cancellation) -> Result<String, String> {
    if mime.contains("..") || mime.matches('/').count() != 1 { return Ok(mime.into()); }
    for root in data_roots() {
        cancelled(cancel)?;
        let path = root.join("mime").join(format!("{}.xml", mime));
        let Ok(text) = std::fs::read_to_string(&path) else { continue; };
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("<comment>") {
                if let Some(name) = rest.strip_suffix("</comment>") { return Ok(name.into()); }
            }
        }
    }
    Ok(mime.into())
}

fn desktop_file(id: &str, cancel: &Cancellation) -> Result<Option<PathBuf>, String> {
    for root in data_roots() {
        cancelled(cancel)?;
        let apps = root.join("applications");
        let direct = apps.join(id);
        if direct.is_file() { return Ok(Some(direct)); }
        let mut pending = vec![apps.clone()];
        while let Some(dir) = pending.pop() {
            cancelled(cancel)?;
            let Ok(entries) = std::fs::read_dir(dir) else { continue; };
            for entry in entries.flatten() {
                cancelled(cancel)?;
                let Ok(kind) = entry.file_type() else { continue; };
                let path = entry.path();
                if kind.is_dir() { pending.push(path); }
                else if path.strip_prefix(&apps).map(|relative| relative.to_string_lossy().replace('/', "-") == id).unwrap_or(false) && path.is_file() { return Ok(Some(path)); }
            }
        }
    }
    Ok(None)
}

fn desktop_label(path: &Path) -> Result<Option<String>, String> { desktop_key(path, "Name=") }

// Sample Desktop Entry: "[Desktop Entry]\nName=Text Editor\nIcon=accessories-text-editor\nExec=editor %U";
// GIO alone interprets Exec. Only the [Desktop Entry] group is read, so an action group cannot answer.
fn desktop_key(path: &Path, key: &str) -> Result<Option<String>, String> {
    let file = super::regfile::open_if_regular(path, 0).map_err(|e| format!("Could not read application {}: {}.", path.display(), e))?;
    let mut bytes = Vec::new();
    file.take(MAX_REGISTRY_BYTES + 1).read_to_end(&mut bytes).map_err(|e| format!("Could not read application {}: {}.", path.display(), e))?;
    if bytes.len() as u64 > MAX_REGISTRY_BYTES { return Err(format!("Application {} exceeds the {} byte metadata limit.", path.display(), MAX_REGISTRY_BYTES)); }
    let text = std::str::from_utf8(&bytes).map_err(|_| format!("Application {} is not valid UTF-8.", path.display()))?;
    let mut entry = false;
    for line in text.lines() {
        // Trimmed: a file written with CRLF or a trailing space is still a desktop file, and an
        // exact compare dropped it from the list with no diagnostic at all.
        let line = line.trim_end();
        if line.starts_with('[') { entry = line == "[Desktop Entry]"; }
        if entry {
            if let Some(name) = line.strip_prefix(key) { return Ok(Some(name.trim().replace("\\s", " ").replace("\\n", " "))); }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::{Duration, Instant};

    fn started(registry: &Registry) -> i32 {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Some(child) = registry.running.lock().unwrap().as_ref() { return child.pid; }
            std::thread::yield_now();
        }
        panic!("the owned GIO child did not start");
    }

    struct TestApplication(Child);
    impl Drop for TestApplication {
        fn drop(&mut self) {
            if matches!(self.0.try_wait(), Ok(None)) {
                if let Err(error) = self.0.kill() { eprintln!("test application cleanup: {}", error); }
            }
            if let Err(error) = self.0.wait() { eprintln!("test application reap: {}", error); }
        }
    }

    #[test]
    fn closing_a_query_kills_its_process_group_and_releases_the_service() {
        let registry = Registry::default();
        let cancel = Cancellation::default();
        let (sent, received) = channel();
        let running = registry.clone();
        let generation = cancel.clone();
        let worker = std::thread::spawn(move || {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", "sleep 600 & wait"]);
            sent.send(running.capture(&mut command, &generation)).unwrap();
        });
        started(&registry);
        cancel.next();
        registry.cancel().unwrap();
        assert!(received.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err().contains("cancelled"));
        worker.join().unwrap();
        assert!(registry.running.lock().unwrap().is_none());
        let mut next = Command::new("/usr/bin/printf");
        next.arg("ready");
        assert_eq!(registry.capture(&mut next, &Cancellation::default()).unwrap(), "ready");
    }

    #[test]
    fn cancelling_a_launcher_preserves_an_application_in_its_process_group() {
        let registry = Registry::default();
        let cancel = Cancellation::default();
        let (sent, received) = channel();
        let running = registry.clone();
        let generation = cancel.clone();
        let worker = std::thread::spawn(move || {
            let mut command = Command::new("/usr/bin/sleep");
            command.arg("600");
            sent.send(running.launch_command(&mut command, &generation)).unwrap();
        });
        let group = started(&registry);
        let mut application = TestApplication(Command::new("/usr/bin/sleep").arg("600")
            .process_group(group).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
        assert!(!registry.running.lock().unwrap().as_ref().unwrap().group);
        cancel.next();
        registry.cancel().unwrap();
        assert!(received.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err().contains("cancelled"));
        worker.join().unwrap();
        assert!(application.0.try_wait().unwrap().is_none(), "cancelling the chooser must not kill an application sharing the launcher's group");
        assert!(registry.running.lock().unwrap().is_none());
        assert!(registry.launch_command(&mut Command::new("/usr/bin/true"), &Cancellation::default()).is_ok());
    }

    #[test]
    fn early_failure_reaps_the_owned_query_before_the_next_request() {
        let registry = Registry::default();
        let mut command = Command::new("/usr/bin/sleep");
        command.arg("600").stdout(Stdio::null()).stderr(Stdio::null());
        let owned = OwnedChild::start(&mut command, &registry, &Cancellation::default(), true).unwrap();
        let descriptor = registry.running.lock().unwrap().as_ref().unwrap().pidfd.try_clone().unwrap();
        drop(owned);
        assert!(registry.running.lock().unwrap().is_none());
        assert!(wait_exit(descriptor.as_raw_fd()).is_ok());
        assert!(registry.launch_command(&mut Command::new("/usr/bin/true"), &Cancellation::default()).is_ok());
    }

    #[test]
    fn desktop_metadata_is_bounded_and_missing_files_are_named() {
        let d = crate::backend::testdir::TestDir::new("menu-desktop");
        let valid = d.file("valid.desktop", "[Desktop Entry]\nName=Text\\sEditor\nExec=editor %U\n");
        assert_eq!(desktop_label(&valid).unwrap().as_deref(), Some("Text Editor"));
        let link = d.join("linked.desktop");
        std::os::unix::fs::symlink(&valid, &link).unwrap();
        assert_eq!(desktop_label(&link).unwrap().as_deref(), Some("Text Editor"));
        let fifo = d.join("fifo.desktop");
        crate::backend::fifotest::mkfifo(&fifo);
        assert!(crate::backend::fifotest::within("desktop_label", move || desktop_label(&fifo)).unwrap_err().contains("not a regular file"));
        let missing = d.join("missing.desktop");
        assert!(desktop_label(&missing).unwrap_err().contains("missing.desktop"));
        let large = d.join("large.desktop");
        std::fs::write(&large, vec![b'x'; MAX_REGISTRY_BYTES as usize + 1]).unwrap();
        assert!(desktop_label(&large).unwrap_err().contains("metadata limit"));
        let cancel = Cancellation::default();
        cancel.next();
        assert!(desktop_file("valid.desktop", &cancel).unwrap_err().contains("cancelled"));
    }

    #[test]
    fn unavailable_failed_and_oversized_queries_are_named_errors() {
        let registry = Registry::default();
        let cancel = Cancellation::default();
        assert!(registry.capture(&mut Command::new("/definitely-missing-gio-test-helper"), &cancel).unwrap_err().contains("Could not start"));
        assert!(registry.capture(&mut Command::new("/usr/bin/false"), &cancel).unwrap_err().contains("query failed"));
        let mut large = Command::new("/usr/bin/head");
        large.args(["-c", &(MAX_REGISTRY_BYTES + 1).to_string(), "/dev/zero"]);
        assert!(registry.capture(&mut large, &cancel).unwrap_err().contains("exceeds"));
        assert!(registry.running.lock().unwrap().is_none());
    }
}
