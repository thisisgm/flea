//! The hyprctl chain and Hyprland's event socket, run off the UI thread; the decisions come from glass_tag.
use super::{dispatch_ran, lua_tag, own_addresses, plan, uses_default, Action, Mode, TAG};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

// A request that lands mid-pass waits this long, then asks again, as the QML tagger's rerun timer does.
const RERUN_GAP: Duration = Duration::from_millis(100);
// One warning per process, so a missing hyprctl or socket is never retried into a log storm.
static WARNED: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
pub(crate) struct State {
    pub(crate) busy: bool,
    // One rerun is queued at a time, however many requests land while a pass runs.
    pub(crate) rerun: bool,
    pub(crate) tagged: BTreeSet<String>,
}

// Clears busy when the pass ends, or when its thread never starts, so a later request still runs.
struct Idle(Tagger);

impl Idle {
    fn run(self) {
        self.0.pass();
    }
}

impl Drop for Idle {
    fn drop(&mut self) {
        self.0.lock().busy = false;
    }
}

/// Omarchy's toggle directory and the two shell.toml files, as the Glass reader finds them.
#[derive(Clone)]
pub(crate) struct Sources {
    toggles: PathBuf,
    user_shell: PathBuf,
    theme_shell: PathBuf,
}

impl Sources {
    pub(crate) fn from_env() -> Self {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        let state = std::env::var_os("XDG_STATE_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/state"));
        Self::under(&home, &state)
    }

    /// The same files under a given home and XDG state directory.
    pub(crate) fn under(home: &Path, state: &Path) -> Self {
        Self {
            toggles: state.join("omarchy/toggles/hypr"),
            user_shell: home.join(".config/omarchy/shell.toml"),
            theme_shell: home.join(".local/state/omarchy/current/theme/shell.toml"),
        }
    }

    // Read on every pass, so a Window Transparency switch shows on the next one.
    fn read(&self) -> (Mode, bool) {
        let mode = Mode::from_toggles(
            self.toggles.join("window-glassy.lua").is_file(),
            self.toggles.join("window-opaque.lua").is_file(),
        );
        let shell = |path: &Path| fs::read_to_string(path).unwrap_or_default();
        let uses = uses_default(&shell(&self.user_shell), &shell(&self.theme_shell));
        (mode, uses)
    }
}

/// One process's tagger, with its pid, the hyprctl program, the files it reads and the windows it tagged.
#[derive(Clone)]
pub(crate) struct Tagger {
    pid: u32,
    hyprctl: OsString,
    sources: Sources,
    state: Arc<Mutex<State>>,
}

impl Tagger {
    pub(crate) fn new(pid: u32, hyprctl: OsString, sources: Sources) -> Self {
        Self {
            pid,
            hyprctl,
            sources,
            state: Arc::default(),
        }
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts a pass now, or while one runs asks again after RERUN_GAP.
    pub(crate) fn retag(&self) {
        let mut state = self.lock();
        if state.busy {
            if !std::mem::replace(&mut state.rerun, true) {
                drop(state);
                self.rerun_later();
            }
            return;
        }
        state.busy = true;
        drop(state);
        let idle = Idle(self.clone());
        let _ = std::thread::Builder::new()
            .name("flea-glass-tag".into())
            .spawn(move || idle.run());
    }

    fn rerun_later(&self) {
        let again = self.clone();
        let spawned = std::thread::Builder::new()
            .name("flea-glass-rerun".into())
            .spawn(move || {
                std::thread::sleep(RERUN_GAP);
                again.lock().rerun = false;
                again.retag();
            });
        if spawned.is_err() {
            self.lock().rerun = false;
        }
    }

    /// One pass runs clients, one probe per window of this process, then the dispatches the plan asks for.
    pub(crate) fn pass(&self) {
        let (mode, uses_default) = self.sources.read();
        let Some(clients) = self.query(&["clients", "-j"]) else {
            return;
        };
        let addresses = own_addresses(&clients, self.pid);
        if addresses.is_empty() {
            return;
        }
        let mut probe = String::new();
        for address in &addresses {
            let window = format!("address:{address}");
            let opacity = self.query(&["getprop", window.as_str(), "opacity"]);
            let inactive = self.query(&["getprop", window.as_str(), "opacity_inactive"]);
            let line = format!(
                "{address} {} {}\n",
                opacity.unwrap_or_default().trim(),
                inactive.unwrap_or_default().trim()
            );
            probe.push_str(&line);
        }
        let tagged = self.lock().tagged.clone();
        let mut outcome = plan(&probe, mode, uses_default, &tagged);
        // Only a dispatch Hyprland ran changes the record, so a refused one is planned again on the next pass.
        for (address, action) in outcome.changes {
            let add = action == Action::Add;
            if !self.dispatch(&address, add) {
                if add {
                    outcome.tagged.remove(&address);
                } else {
                    outcome.tagged.insert(address);
                }
            }
        }
        self.lock().tagged = outcome.tagged;
    }

    /// Runs hyprctl for its stdout; a failure logs once and reads as no answer.
    fn query(&self, args: &[&str]) -> Option<String> {
        match Command::new(&self.hyprctl)
            .args(args)
            .stderr(Stdio::null())
            .output()
        {
            Ok(Output { status, stdout, .. }) if status.success() => {
                Some(String::from_utf8_lossy(&stdout).into_owned())
            }
            Ok(output) => {
                warn_once(&format!(
                    "hyprctl {} exited with {}",
                    args.join(" "),
                    output.status
                ));
                None
            }
            Err(error) => {
                warn_once(&format!("hyprctl {} did not run: {error}", args.join(" ")));
                None
            }
        }
    }

    /// Adds or removes Flea's tag, trying the Lua dispatcher first and the classic tagwindow form after; true when one ran.
    fn dispatch(&self, address: &str, add: bool) -> bool {
        let tag = format!("{}{TAG}", if add { '+' } else { '-' });
        let window = format!("address:{address}");
        let lua = lua_tag(&window, &tag);
        // The Lua form is refused on Hyprland builds that only know tagwindow, so only a second refusal warns.
        if self.dispatched(&["dispatch", lua.as_str()]).is_ok() {
            return true;
        }
        let classic = ["dispatch", "tagwindow", tag.as_str(), window.as_str()];
        match self.dispatched(&classic) {
            Ok(()) => true,
            Err(reply) => {
                warn_once(&format!("hyprctl did not tag {window} {tag}: {reply}"));
                false
            }
        }
    }

    // The reply decides, never the exit status: hyprctl exits 0 when a dispatcher refuses.
    fn dispatched(&self, args: &[&str]) -> Result<(), String> {
        let output = Command::new(&self.hyprctl)
            .args(args)
            .stderr(Stdio::null())
            .output()
            .map_err(|error| error.to_string())?;
        let reply = String::from_utf8_lossy(&output.stdout);
        if dispatch_ran(&reply) {
            Ok(())
        } else {
            Err(reply.trim().to_owned())
        }
    }
}

pub(crate) fn warn_once(message: &str) {
    if !WARNED.swap(true, Ordering::Relaxed) {
        eprintln!("flea-gui: window transparency tag skipped: {message}");
    }
}
