//! The hyprctl chain and Hyprland's event socket, run off the UI thread; the decisions come from glass_tag.
use super::{
    dispatch_ran, lua_exit_rule, lua_owned_rule, lua_tag, nautilus_opacity, own_addresses,
    parse_props, plan, rule_step, uses_default, Action, Mode, RULE, TAG,
};
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
    // Shutdown blocks queued passes from enabling the rule after cleanup.
    pub(crate) stopped: bool,
    pub(crate) tagged: BTreeSet<String>,
    // Whether this process last enabled (Some(true)) or disabled its Glassy rule; None until its first eval.
    pub(crate) ruled: Option<bool>,
    // The glass inputs the window last applied, so only a change among them asks for a pass.
    pub(crate) followed: Option<crate::window::glass::Inputs>,
    // Told under the lock that clears busy, so a test can tell a pass that ended from one never started.
    #[cfg(test)]
    pub(crate) ended: Option<std::sync::mpsc::Sender<()>>,
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
        let mut state = self.0.lock();
        state.busy = false;
        #[cfg(test)]
        if let Some(ended) = &state.ended {
            let _ = ended.send(());
        }
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

    // Read on every pass, so a Window Transparency switch shows on the next one; Nautilus's Glassy opacity is wanted only at the default alpha.
    fn read(&self) -> (Mode, bool, Option<(f64, f64)>) {
        let glassy = fs::read_to_string(self.toggles.join("window-glassy.lua")).ok();
        let mode = Mode::from_toggles(
            glassy.is_some(),
            self.toggles.join("window-opaque.lua").is_file(),
        );
        let shell = |path: &Path| fs::read_to_string(path).unwrap_or_default();
        let uses = uses_default(&shell(&self.user_shell), &shell(&self.theme_shell));
        let wanted = glassy.filter(|_| uses).and_then(|text| {
            let opacity = nautilus_opacity(&text);
            if opacity.is_none() {
                warn_once("Omarchy's Glassy toggle gives Nautilus no opacity Flea can follow");
            }
            opacity
        });
        (mode, uses, wanted)
    }
}

/// One process's tagger, with its pid, the hyprctl program, the files it reads and the windows it tagged.
#[derive(Clone)]
pub(crate) struct Tagger {
    pid: u32,
    hyprctl: OsString,
    sources: Sources,
    state: Arc<Mutex<State>>,
    pass_gate: Arc<Mutex<()>>,
}

impl Tagger {
    pub(crate) fn new(pid: u32, hyprctl: OsString, sources: Sources) -> Self {
        Self {
            pid,
            hyprctl,
            sources,
            state: Arc::default(),
            pass_gate: Arc::default(),
        }
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts a pass now, or while one runs asks again after RERUN_GAP.
    pub(crate) fn retag(&self) {
        let mut state = self.lock();
        if state.stopped {
            return;
        }
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

    /// Asks for a pass when the window's glass inputs changed, as a live shell.toml alpha edit does; Hyprland sends no event for it.
    pub(crate) fn follow(&self, inputs: crate::window::glass::Inputs) {
        if self.lock().followed.replace(inputs) != Some(inputs) {
            self.retag();
        }
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
        let _pass = self
            .pass_gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.lock().stopped {
            return;
        }
        let (mode, uses_default, wanted) = self.sources.read();
        // The app paints the original translucent fills; fading the whole surface would dim text twice.
        let wanted = if mode == Mode::Glassy
            && uses_default
            && wanted.is_some()
            && self
                .lock()
                .followed
                .is_some_and(|input| input.chrome_inks.is_some() && input.wanted_alpha < 255)
        {
            Some((1., 1.))
        } else {
            wanted
        };
        // Clear a stale global rule even before a client has mapped or its properties can be read.
        if wanted.is_none() && rule_step(wanted, &[], self.lock().ruled).is_some() {
            self.set_rule(None);
        }
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
        if wanted.is_none() {
            return;
        }
        let windows: Vec<_> = probe.lines().filter_map(parse_props).collect();
        let ruled = self.lock().ruled;
        if let Some(opacity) = rule_step(wanted, &windows, ruled) {
            self.set_rule(opacity);
        }
    }

    /// Waits for an in-flight pass, then lets Hyprland atomically retire this process's ownership.
    pub(crate) fn stop(&self) {
        let _pass = self
            .pass_gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.lock().stopped = true;
        if self.lock().ruled != Some(true) {
            return;
        }
        self.eval_rule(&lua_exit_rule(self.pid), false);
    }

    fn set_rule(&self, opacity: Option<(f64, f64)>) {
        self.eval_rule(&lua_owned_rule(self.pid, opacity), opacity.is_some());
    }

    fn eval_rule(&self, lua: &str, enabled: bool) {
        // Ownership chunks start with a Lua comment, so stop hyprctl parsing that payload as a flag.
        match self.dispatched(&["eval", "--", lua]) {
            Ok(()) => self.lock().ruled = Some(enabled),
            // A host without eval has no Lua rule to disable; Glassy enablement still needs to succeed.
            Err(reply) if !enabled && eval_unavailable(&reply) => {}
            Err(reply) => warn_once(&format!("hyprctl eval did not set {RULE}: {reply}")),
        }
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

// Sample input: "usage: hyprctl [flags] <command>" or "unknown request".
fn eval_unavailable(reply: &str) -> bool {
    reply.starts_with("usage: hyprctl ")
        || matches!(
            reply,
            "unknown request" | "unknown command" | "unknown command: eval"
        )
}

pub(crate) fn warn_once(message: &str) {
    if !WARNED.swap(true, Ordering::Relaxed) {
        eprintln!("flea-gui: window transparency tag skipped: {message}");
    }
}
