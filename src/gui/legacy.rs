// Replace verified QML windows through their compositor close path, which drains their backends.
use super::legacy_command::output;
use super::legacy_identity::{self, Instance};
use crate::jsondoc::{self, Json};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

// The carried window state rides an environment variable, so it stays far below the argument limit.
const MAX_CARRY_BYTES: usize = 48 * 1024;

#[derive(Default)]
pub(super) struct Carry {
    pub path: Option<String>,
    pub select: Option<String>,
    pub selection: Vec<String>,
    pub windows: Vec<Json>,
    _guard: Option<std::fs::File>,
}

fn ipc(instance: &Instance, function: &str) -> Result<String, String> {
    if !instance.verified() {
        return Err("the old window changed identity during the upgrade".into());
    }
    output(instance.command().args([
        "ipc",
        "--pid",
        &instance.pid.to_string(),
        "call",
        "flea",
        function,
    ]))
}

fn document(instance: &Instance, function: &str) -> Result<Json, String> {
    jsondoc::parse(&ipc(instance, function)?)
        .map_err(|_| "the old window answered with an unsupported state".into())
}

fn busy(instance: &Instance) -> Result<bool, String> {
    let activity = document(instance, "statusActivityState")?;
    let activities = activity
        .get("activities")
        .and_then(Json::as_array)
        .ok_or("the old window did not report its operations")?;
    let running = activities
        .iter()
        .any(|item| item.get("running").and_then(Json::as_bool) != Some(false));
    let editor = ipc(instance, "renameEditorLive")?;
    if !matches!(editor.trim(), "true" | "false") {
        return Err("the old window did not report its editor state".into());
    }
    Ok(running || editor.trim() == "true")
}

fn absolute(value: Option<&Json>) -> Option<String> {
    value
        .and_then(Json::as_str)
        .filter(|s| s.starts_with('/') && !s.contains('\0'))
        .map(str::to_string)
}

// A Lua session answers the classic shape with a syntax error and closes nothing, so it is safe to try first;
// the classic shape stays because an update can land under a session that predates the Lua dispatch.
fn close_window(address: &str) -> Result<(), String> {
    let classic = format!("address:{address}");
    let lua = format!("hl.dsp.window.close({{ window = \"address:{address}\" }})");
    output(Command::new("hyprctl").args(["dispatch", "closewindow", &classic]))
        .or_else(|_| output(Command::new("hyprctl").args(["dispatch", &lua])))
        .map(drop)
        .map_err(|_| "the compositor refused to close the old window".to_string())
}

pub(super) fn replace() -> Result<Carry, String> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(format!(
                "/run/user/{}",
                crate::backend::manifestdir::current_uid()
            ))
        });
    let instances = legacy_identity::find(&runtime)?;
    let mut carry = Carry {
        path: None,
        select: None,
        selection: Vec::new(),
        windows: Vec::new(),
        _guard: None,
    };
    if instances.is_empty() {
        return Ok(carry);
    }
    let guard = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(crate::oflags::O_NOFOLLOW | crate::oflags::O_NONBLOCK)
        .open(runtime.join("flea-upgrade.lock"))
        .map_err(|_| "the window upgrade lock could not be opened")?;
    if !guard.metadata().is_ok_and(|m| {
        m.is_file()
            && m.uid() == crate::backend::manifestdir::current_uid()
            && m.mode() & 0o022 == 0
    }) || !crate::uistore::try_lock_exclusive(&guard)
        .map_err(|_| "the window upgrade lock could not be taken")?
    {
        return Err("another window upgrade is already running".into());
    }
    carry._guard = Some(guard);
    let clients = jsondoc::parse(&output(Command::new("hyprctl").args(["-j", "clients"]))?)
        .map_err(|_| "the compositor did not report the old windows")?;
    let clients = clients
        .as_array()
        .ok_or("the compositor did not report the old windows")?;
    let mut addresses = Vec::new();
    for instance in &instances {
        if busy(instance)? {
            return Err(
                "an old window is busy; finish its operation before opening the new window".into(),
            );
        }
        let state = document(instance, "providerState")?;
        let path = absolute(state.get("path"))
            .ok_or("the old window did not report an absolute folder")?;
        let selection = state
            .get("selectedPaths")
            .and_then(Json::as_array)
            .ok_or("the old window did not report its selection")?;
        let selection: Option<Vec<_>> = selection
            .iter()
            .map(|value| absolute(Some(value)))
            .collect();
        let selection = selection.ok_or("the old window has a selection outside its held rows")?;
        let cursor = absolute(state.get("cursorPath"));
        carry.windows.push(Json::Obj(vec![
            ("path".into(), Json::Str(path.clone())),
            (
                "cursorPath".into(),
                state.get("cursorPath").cloned().unwrap_or(Json::Null),
            ),
            (
                "selectedPaths".into(),
                state.get("selectedPaths").cloned().unwrap_or(Json::Null),
            ),
        ]));
        if jsondoc::render(&Json::Arr(carry.windows.clone())).len() > MAX_CARRY_BYTES {
            return Err(
                "the old window state is too large to hand to the new window safely".into(),
            );
        }
        carry.path = Some(path);
        carry.select = selection.first().cloned().or(cursor);
        carry.selection = selection;
        let client = clients
            .iter()
            .find(|c| c.get("pid").and_then(Json::as_f64) == Some(instance.pid as f64))
            .ok_or("the compositor cannot close the old window cleanly")?;
        let address = client
            .get("address")
            .and_then(Json::as_str)
            .filter(|s| {
                s.strip_prefix("0x").is_some_and(|tail| {
                    !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_hexdigit())
                })
            })
            .ok_or("the compositor did not report the old window's address")?;
        addresses.push(address.to_string());
    }
    // Check every window before closing any, so one busy or hung neighbour leaves the set intact.
    for instance in &instances {
        if !instance.verified() || busy(instance)? {
            return Err("an old window became busy during the upgrade".into());
        }
    }
    for (instance, address) in instances.iter().zip(addresses) {
        if !instance.verified() {
            return Err("the old window changed identity during the upgrade".into());
        }
        close_window(&address)?;
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    while instances.iter().any(Instance::alive) {
        if Instant::now() >= deadline {
            return Err(
                "the old window has not finished closing; the new window was not opened".into(),
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // An old helper can open another window while the previous shells drain.
    if !legacy_identity::find(&runtime)?.is_empty() {
        return Err("another old window opened during the upgrade; try again".into());
    }
    Ok(carry)
}
