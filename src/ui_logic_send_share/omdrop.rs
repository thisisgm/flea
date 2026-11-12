//! AirDrop through the Omdrop the user installed (https://github.com/netmojo-systems/omdrop-plugin, MIT), when owly is absent.
//! Omdrop keeps its own radio, receiver and download folder, so Flea only reads its status, turns it on or off, lists and sends.
use super::airdrop::{browse_name_is_phone, Adapter, Peer, Readiness, Verdict, DOCTOR_TIMEOUT_SECS};
use crate::jsondoc::{self, Json};

pub const PROGRAM: &str = "omdrop";
// What the adapter fact names, since Omdrop reports no card of its own.
pub const STACK: &str = "Omdrop";
// What a cancelled send prints last, which says nothing of why a send failed.
const CANCELLED: &str = "Cancelled.";
// Omdrop's own stderr line for a refused Ask, the only refusal it words.
const DECLINED: &str = "They declined it.";

// Which AirDrop stack runs: owly when it is on PATH, else Omdrop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stack {
    Owly,
    Omdrop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tool {
    pub stack: Stack,
    pub path: String,
}

// owly first, as Flea shipped it; Omdrop only when owly is not installed. Both empty is no AirDrop at all.
pub fn pick(owly: &str, omdrop: &str) -> Option<Tool> {
    match (owly, omdrop) {
        ("", "") => None,
        ("", path) => Some(Tool { stack: Stack::Omdrop, path: path.into() }),
        (path, _) => Some(Tool { stack: Stack::Owly, path: path.into() }),
    }
}

// The tool on a PATH value, as the backend and the Settings page find it without facts.
pub fn find(path: &std::ffi::OsStr) -> Option<Tool> {
    let owly = super::airdrop::find(path, super::airdrop::PROGRAM).unwrap_or_default();
    let omdrop = super::airdrop::find(path, PROGRAM).unwrap_or_default();
    pick(&owly, &omdrop)
}

// What one `omdrop status --json` says: whether sends can work, the radio, the receiver, and how it receives.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub adapter: Adapter,
    // The radio is visible nearby, which discovery and sending need.
    pub on: bool,
    // The receiver runs; it can while the radio is down, and the radio can be up without it.
    pub receiving: bool,
    // Only known senders may send, Omdrop's saved Contacts Only.
    pub contacts: bool,
    pub name: String,
    pub dir: String,
}

// Sample input: {"receiving": true, "visible": true, "remaining": null, "name": "Omarchy", "dir": "/home/u/Downloads", "mode": "forever", "reason": "ok", "visibility": "everyone"}
// A null `visible` is a radio backend Omdrop cannot find, so nothing can be sent; None when the run failed and said nothing.
pub fn status(stdout: &str, exit_code: i32) -> Option<Status> {
    let card = STACK.to_string();
    let doc = jsondoc::parse(stdout.trim()).ok().filter(|_| exit_code == 0)?;
    let visible = doc.get("visible").and_then(Json::as_bool);
    let text = |key: &str| doc.get(key).and_then(Json::as_str).unwrap_or_default().to_string();
    let readiness = match visible {
        Some(_) => Readiness::Ready,
        None => Readiness::NotReady,
    };
    Some(Status {
        adapter: Adapter { readiness, card },
        on: visible == Some(true),
        receiving: doc.get("receiving").and_then(Json::as_bool) == Some(true),
        contacts: text("visibility") == "contacts",
        name: text("name"),
        dir: text("dir"),
    })
}

// Sample input: {"mac": "f2:f2:ee:c4:5a:f7", "rssi": null, "name": "Jane's iPhone"}; a device that gave no name is not listed.
pub fn peer_line(line: &str) -> Option<Peer> {
    let doc = jsondoc::parse(line.trim()).ok()?;
    let mac = doc.get("mac").and_then(Json::as_str).filter(|m| !m.is_empty())?;
    let name = doc.get("name").and_then(Json::as_str).filter(|n| !n.is_empty())?;
    Some(Peer {
        id: mac.into(),
        name: name.into(),
        phone: browse_name_is_phone(name),
    })
}

// Sample input: stdout "Sent b.png (2.3 MB)" exit 0; stderr "They declined it." exit 1; stderr "They never answered: timed out" exit 1.
pub fn verdict(stderr: &str, exit_code: i32) -> Verdict {
    if exit_code == 0 {
        return Verdict::Delivered;
    }
    if stderr.lines().any(|line| line.trim() == DECLINED) {
        return Verdict::Declined;
    }
    Verdict::Failed(reason(stderr))
}

// Omdrop's last sentence on stderr, its own words for why a run failed; empty when it said none.
pub fn reason(stderr: &str) -> String {
    stderr
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty() && *line != CANCELLED)
        .unwrap_or_default()
        .to_string()
}

pub fn status_argv(omdrop: &str) -> Vec<String> {
    let mut argv = vec!["timeout".into(), "--signal=KILL".into(), DOCTOR_TIMEOUT_SECS.to_string()];
    argv.extend([omdrop.to_string(), "status".into(), "--json".into()]);
    argv
}

// 1Password refuses op from a session whose leader has exited, as Flea's has. Flea's children lead no process group,
// so setsid execs in place and the pid a cancel signals stays omdrop's.
fn own_session(mut argv: Vec<String>) -> Vec<String> {
    argv.splice(0..0, ["setsid".to_string(), "-w".to_string()]);
    argv
}

// Omdrop's own default window, ten minutes, the same Everyone for 10 Minutes owly's radio holds; it fetches the identity.
pub fn on_argv(omdrop: &str) -> Vec<String> {
    own_session(vec![omdrop.into(), "on".into()])
}

// Only Receiving set to Off asks this; a closing window leaves Omdrop as the user set it.
pub fn off_argv(omdrop: &str) -> Vec<String> {
    vec![omdrop.into(), "off".into()]
}

// Each device as it gives its name, one JSON object a line; the lookup ends on its own and an open menu runs it again.
pub fn peers_argv(omdrop: &str) -> Vec<String> {
    vec![omdrop.into(), "peers".into(), "-n".into(), "--json".into(), "--stream".into()]
}

// The device is its address on --to=, so two devices with one name stay apart; files go after "--".
// A send turns Omdrop on first when it is off, which fetches the identity.
pub fn send_argv(omdrop: &str, peer: &str, paths: &[String]) -> Vec<String> {
    let mut argv = vec![omdrop.to_string(), "send".into(), format!("--to={peer}"), "--".into()];
    argv.extend(paths.iter().cloned());
    own_session(argv)
}

// Save received files to, chosen on the page, is Omdrop's own folder setting.
// `omdrop dir` may restart the receiver; one that hangs past this is killed, so the next folder choice can run.
pub const DIR_TIMEOUT_SECS: u64 = 30;

pub fn dir_argv(omdrop: &str, folder: &str) -> Vec<String> {
    let mut argv = vec!["timeout".into(), "--signal=KILL".into(), DIR_TIMEOUT_SECS.to_string()];
    argv.extend([omdrop.into(), "dir".into(), folder.into()]);
    argv
}

#[cfg(test)]
#[path = "omdrop_tests.rs"]
mod tests;
