// Native selected-font preference transactions. No interpreter or Cargo dependencies.
mod alacritty;
mod edit;
mod files;
mod journal;
mod settings;
use crate::jsondoc::Json;
use journal::{Entry, Journal};
use std::path::Path;

const TARGETS: [(&str, &str, bool); 7] = [
    ("desktop", "Desktop interface (GTK / sans-serif)", false), ("serif", "Documents (serif)", false),
    ("monospace", "System monospace / Omarchy", true), ("alacritty", "Alacritty", true),
    ("kitty", "Kitty", true), ("ghostty", "Ghostty", true), ("foot", "Foot", true),
];
const NOTICE: &str = "Preference saved. Reopen applications or terminals to use it; the Omarchy shell may need restarting. Applications with their own font settings may keep those settings.";

// Match the former helper's overall deadline, including local preference I/O.
// Dropping the sender cancels this process-local watchdog on every return path.
fn deadline() -> Result<std::sync::mpsc::Sender<()>, String> {
    let (cancel, wait) = std::sync::mpsc::channel();
    std::thread::Builder::new().name("font-preference-deadline".into()).spawn(move || {
        if matches!(wait.recv_timeout(std::time::Duration::from_secs(60)), Err(std::sync::mpsc::RecvTimeoutError::Timeout)) {
            println!("{{\"error\":\"The font preference operation timed out. Reopen the preview to check its state.\"}}");
            std::process::exit(2);
        }
    }).map_err(|e| e.to_string())?;
    Ok(cancel)
}

pub fn run(verb: &str, target: &str, family: &str) -> Result<String, String> {
    let _deadline = deadline()?;
    let config = files::root("XDG_CONFIG_HOME", ".config")?;
    let state = files::root("XDG_STATE_HOME", ".local/state")?.join("flea/font-defaults");
    if verb != "status" {
        if !["apply", "restore"].contains(&verb) || !TARGETS.iter().any(|(name, _, _)| *name == target) { return Err("Choose a supported destination.".into()); }
        if verb == "apply" && (family.is_empty() || family.len() > 512 || family.chars().any(char::is_control)) { return Err("The font family name cannot be used in preferences.".into()); }
        let _lock = files::lock(&state)?;
        mutate(&config, &state, verb, target, family)?;
    }
    let mut targets = Vec::new();
    for (id, label, mono) in TARGETS {
        let previous = Journal::read(&state.join(format!("{id}.json")))?;
        targets.push(Json::Obj(vec![("id".into(), Json::Str(id.into())), ("label".into(), Json::Str(label.into())),
            ("monospace".into(), Json::Bool(mono)), ("available".into(), Json::Bool(edit::available(&config, id))),
            ("canRestore".into(), Json::Bool(previous.is_some())), ("family".into(), Json::Str(previous.map(|j| j.family).unwrap_or_default()))]));
    }
    Ok(crate::jsondoc::render(&Json::Obj(vec![("targets".into(), Json::Arr(targets)),
        ("notice".into(), Json::Str(if verb == "status" { "" } else { NOTICE }.into()))])))
}
fn verify(journal: &Journal) -> Result<(), String> {
    if let Some(setting) = &journal.setting {
        if settings::call("get", None)? != setting.applied { return Err("The desktop font changed outside Flea. Restore it manually to preserve your edits.".into()); }
    }
    for entry in &journal.files {
        if files::read(&entry.path)?.as_deref() != Some(&entry.applied) { return Err("Preferences changed outside Flea. Restore them manually to avoid overwriting your edits.".into()); }
    }
    Ok(())
}
fn prepare(config: &Path, target: &str, family: &str, previous: Option<&Journal>) -> Result<Journal, String> {
    if !edit::available(config, target) { return Err("This terminal is not installed or configured.".into()); }
    let mut entries = Vec::new();
    for (configured, content) in edit::changes(config, target, family)? {
        let path = files::resolve(&configured)?;
        if path.starts_with("/usr") || path.starts_with("/etc") { return Err("The configuration points at a system file; choose a user configuration.".into()); }
        path.to_str().ok_or("The preference path is not valid UTF-8.")?;
        let original = files::read(&path)?;
        let mode = files::mode(&path);
        entries.push(Entry { path, original, applied: content, mode });
    }
    if let Some(old) = previous {
        if old.files.len() != entries.len() { return Err("Configuration locations changed. Restore the previous preference first.".into()); }
        for entry in &mut entries {
            let old = old.files.iter().find(|old| old.path == entry.path).ok_or("Configuration locations changed. Restore the previous preference first.")?;
            entry.original = old.original.clone(); entry.mode = old.mode;
        }
    }
    let mut setting = if target == "desktop" { settings::prepare(family) } else { None };
    if let Some(old) = previous.and_then(|j| j.setting.as_ref()) {
        let new = setting.as_mut().ok_or("The desktop font settings service is unavailable.")?;
        new.original = old.original.clone();
    }
    Ok(Journal { family: family.into(), files: entries, setting })
}
fn mutate(config: &Path, state: &Path, verb: &str, target: &str, family: &str) -> Result<(), String> {
    let path = state.join(format!("{target}.json"));
    let previous = Journal::read(&path)?;
    if let Some(old) = &previous { verify(old)?; }
    let restore = verb == "restore";
    let mut journal = if restore { previous.ok_or("There is no previous preference saved for this destination.")? }
        else { prepare(config, target, family, previous.as_ref())? };
    let rollback: Vec<_> = journal.files.iter().map(|entry| Ok((entry.path.clone(), files::read(&entry.path)?, files::mode(&entry.path)))).collect::<Result<_, String>>()?;
    let setting_before = if journal.setting.is_some() { Some(settings::call("get", None)?) } else { None };
    let result = (|| {
        for entry in &journal.files {
            files::write(&entry.path, if restore { entry.original.as_deref() } else { Some(&entry.applied) }, entry.mode)?;
        }
        if let Some(setting) = &mut journal.setting {
            let wanted = if restore { &setting.original } else { &setting.applied };
            settings::call("set", Some(wanted))?;
            let actual = settings::call("get", None)?;
            if settings::decode(&actual)? != settings::decode(wanted)? { return Err("The desktop settings service did not save the chosen font.".into()); }
            if !restore { setting.applied = actual; }
        }
        if restore { std::fs::remove_file(&path).map_err(|e| e.to_string())?; }
        else { files::atomic(&path, &journal.json(), 0o600)?; }
        Ok(())
    })();
    if let Err(error) = result {
        let mut failures = Vec::new();
        for (path, content, mode) in rollback { if let Err(e) = files::write(&path, content.as_deref(), mode) { failures.push(e); } }
        if let Some(value) = setting_before { if let Err(e) = settings::call("set", Some(&value)) { failures.push(e); } }
        if failures.is_empty() { return Err(error); }
        return Err(format!("{error} Rollback reported: {}", failures.join("; ")));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
