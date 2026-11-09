// The updater's row state and the words its row and note show, from v0.3.12's ui/js/Update.js.
use super::rows::Role;
use super::Text;

// The background poll's period, which is also how long a finished check stands before About asks again.
pub const INTERVAL_MS: u64 = 6 * 60 * 60 * 1000;
// The first background look, a minute after the window opens, well inside the first poll.
pub const FIRST_CHECK_MS: u64 = 60 * 1000;

// The four words `flea --update check` prints first; anything else is a check that failed.
const ANSWERS: [&str; 4] = ["available", "current", "failed", "unchecked"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Idle,
    Checking,
    Available,
    Current,
    Failed,
    Unchecked,
    Launched,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub state: State,
    pub kind: String,
    pub installed: String,
    pub latest: String,
    pub checked_at: u64,
}

pub fn idle() -> Status {
    Status::default()
}

pub fn checking(status: &Status) -> Status {
    Status { state: State::Checking, ..status.clone() }
}

// A dash is an empty field, never a version.
fn field(word: &str) -> String {
    if word == "-" { String::new() } else { word.to_owned() }
}

// Sample input: "available opr 0.3.2-1 0.3.3-1\n", the one line src/update.rs prints.
pub fn answered(status: &Status, text: &str, now: u64) -> Status {
    // A launch is final until Flea restarts: a check that was already running when it began changes nothing.
    if status.state == State::Launched {
        return status.clone();
    }
    let words: Vec<&str> = text.trim().split(' ').collect();
    if words.len() != 4 || !ANSWERS.contains(&words[0]) {
        return Status { state: State::Failed, checked_at: now, ..Status::default() };
    }
    let state = match words[0] {
        "available" => State::Available,
        "current" => State::Current,
        "failed" => State::Failed,
        _ => State::Unchecked,
    };
    Status {
        state,
        kind: words[1].to_owned(),
        installed: field(words[2]),
        latest: field(words[3]),
        checked_at: now,
    }
}

// A launch hands the machine to Omarchy until Flea restarts; one that failed leaves the row as it was.
pub fn launched_from(status: &Status, started: bool) -> Status {
    if started { Status { state: State::Launched, ..status.clone() } } else { status.clone() }
}

// What the About row's Enter does: check, launch Omarchy's updater, or nothing while a check or an update runs.
pub fn on_activate(status: &Status) -> &'static str {
    match status.state {
        State::Idle | State::Current => "check",
        State::Available | State::Failed => "launch",
        _ => "",
    }
}

// The chevron: drawn exactly when Enter does something.
pub fn acts(status: &Status) -> bool {
    !on_activate(status).is_empty()
}

// A local build has nothing to check or launch, so its row is a fact.
pub fn is_fact(status: &Status) -> bool {
    status.state == State::Unchecked
}

// About opening asks again unless the switch is off, a check or an update is running, or an answer from the last period stands.
pub fn due(status: &Status, now: u64, automatic: bool) -> bool {
    if !automatic || matches!(status.state, State::Checking | State::Launched) {
        return false;
    }
    let settled = matches!(status.state, State::Available | State::Current | State::Unchecked);
    !settled || now.saturating_sub(status.checked_at) >= INTERVAL_MS
}

// UpdateCheck.qml's two timers: the first, a minute after the window opens, asks only when due; each six-hour beat asks outright.
pub fn poll_asks(status: &Status, now: u64, automatic: bool, first: bool) -> bool {
    if first {
        return due(status, now, automatic);
    }
    automatic && !matches!(status.state, State::Checking | State::Launched)
}

// How long the poll waits before its next beat.
pub fn poll_wait(first: bool) -> u64 {
    if first { FIRST_CHECK_MS } else { INTERVAL_MS }
}

// The part before the first dash, which is the pkgver of a package version.
fn pkgver(version: &str) -> &str {
    version.split('-').next().unwrap_or("")
}

// pkgver alone, unless that is what is installed and only the package release moved.
fn shown_version(latest: &str, installed: &str) -> String {
    if pkgver(latest) == pkgver(installed) { latest.to_owned() } else { pkgver(latest).to_owned() }
}

pub fn value(status: &Status) -> Text {
    match status.state {
        State::Idle => Text::Key("settings.update.check"),
        State::Checking => Text::Key("settings.update.checking"),
        State::Available => {
            Text::args("settings.update.available", vec![Text::data(shown_version(&status.latest, &status.installed))])
        }
        State::Current => Text::args("settings.update.current", vec![Text::data(pkgver(&status.installed))]),
        State::Failed => Text::Key("settings.update.failed"),
        State::Launched => Text::Key("settings.update.launched"),
        State::Unchecked => Text::Key("settings.update.source"),
    }
}

// The value's colour role in ui/SettingsRow.qml: accent for an update, muted while checking, foreground otherwise.
pub fn role(status: &Status) -> Role {
    match status.state {
        State::Available => Role::Accent,
        State::Checking => Role::None,
        _ => Role::Live,
    }
}

// The line under the Updates group, or None in the states that need no explanation.
pub fn note(status: &Status) -> Option<(Text, Role)> {
    match status.state {
        State::Launched => Some((Text::Key("settings.update.note_launched"), Role::Accent)),
        State::Failed => Some((Text::Key("settings.update.note_failed"), Role::Foreground)),
        State::Unchecked => Some((Text::Key("settings.update.note_source"), Role::Foreground)),
        _ => None,
    }
}

// The background menu's row exists only while a newer build is known, and its hint is that version.
pub fn menu_version(status: &Status) -> String {
    if status.state == State::Available { shown_version(&status.latest, &status.installed) } else { String::new() }
}

// What the footer says after a launch, as (sentence, is_error).
pub fn launch_sentence(started: bool) -> (Text, bool) {
    if started {
        (Text::Key("settings.update.opening"), false)
    } else {
        (Text::Key("settings.update.launch_failed"), true)
    }
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
