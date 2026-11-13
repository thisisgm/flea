//! AirDrop through the owly the user installed (https://github.com/t4t5/owly, MIT, by Tristan, @t4t5), its CLI read once for every front end.
use crate::i18n::I18n;
use crate::jsondoc::{self, Json};

pub const PROGRAM: &str = "owly";
// Everyone for 10 Minutes, Apple's own words, is how long one turn-on holds the radio.
pub const WINDOW_SECS: u64 = 600;
// One flyout open browses this long; a receiver answers within a few seconds of its first advert.
pub const BROWSE_SECS: u64 = 30;
pub const DOCTOR_TIMEOUT_SECS: u64 = 10;
// coreutils timeout exits 124 when its command ran the whole time, as the receiver does when its ten minutes end.
pub const TIMEOUT_EXPIRED: i32 = 124;
// Sample input: `owly: the radio went down: expired`, the line `owly radio up --for` prints, exiting 1, when its lease ends on time.
pub const RADIO_EXPIRED: &str = "owly: the radio went down: expired";

// What `owly doctor` says about this computer's Wi-Fi, in the order the page words it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Readiness {
    #[default]
    Checking,
    // owly is not on PATH, so nothing was asked.
    Missing,
    Ready,
    // No card owly can drive.
    NoCard,
    // owly's table marks this card as known not to work.
    Broken,
    // A card owly has not tested, which runs only once allowed.
    Untested,
    // owlyd, the radio service, is not running.
    NoService,
    // Anything else in the way, such as an access point on the wrong channel.
    NotReady,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Adapter {
    pub readiness: Readiness,
    // The card's name for the page: "Intel AX210", else its driver.
    pub card: String,
}

impl Adapter {
    pub fn ready(&self) -> bool {
        self.readiness == Readiness::Ready
    }
}

// Card names for the PCI and USB ids owly's hardware table lists (owly README, 2026-11-03).
const CARDS: [(&str, &str); 30] = [
    ("8086:2725", "Intel AX210"),
    ("8086:2723", "Intel AX200"),
    ("8086:02f0", "Intel AX201"),
    ("8086:51f0", "Intel AX211"),
    ("8086:51f1", "Intel AX211"),
    ("8086:7a70", "Intel AX211"),
    ("8086:7740", "Intel AX211"),
    ("8086:2526", "Intel 9260"),
    ("8086:a370", "Intel 9560"),
    ("8086:272b", "Intel BE200"),
    ("8086:e440", "Intel BE"),
    ("8086:24fd", "Intel 8265"),
    ("8086:24f3", "Intel 8260"),
    ("8086:095a", "Intel 7265"),
    ("14c3:0717", "MediaTek MT7925"),
    ("14c3:7961", "MediaTek MT7921"),
    ("14c3:0616", "MediaTek MT7922"),
    ("14c3:7927", "MediaTek MT7927"),
    ("0e8d:7612", "MediaTek MT7612U"),
    ("10ec:b822", "Realtek RTL8822BE"),
    ("10ec:b851", "Realtek RTL8851BE"),
    ("10ec:8852", "Realtek RTL8852AE"),
    ("10ec:b852", "Realtek RTL8852BE"),
    ("10ec:c852", "Realtek RTL8852CE"),
    ("10ec:8922", "Realtek RTL8922AE"),
    ("35bc:0108", "Realtek RTL8852BU"),
    ("10ec:c821", "Realtek RTL8821CE"),
    ("2357:0120", "Realtek RTL8821AU"),
    ("168c:002e", "Atheros AR9287"),
    ("17cb:1103", "Qualcomm WCN6855"),
];

fn field<'a>(doc: &'a Json, key: &str) -> &'a str {
    doc.get(key).and_then(Json::as_str).unwrap_or_default()
}

// Sample input: {"station":"wlp2s0","driver":"iwlwifi","device":"8086:24fd","status":"tested","hardware":true,"missing":[{"id":"backend","say":"..."}]}
pub fn doctor(stdout: &str, exit_code: i32) -> Adapter {
    let Ok(doc) = jsondoc::parse(stdout.trim()) else {
        return Adapter {
            readiness: Readiness::NotReady,
            card: String::new(),
        };
    };
    let device = field(&doc, "device").to_ascii_lowercase();
    let card = CARDS
        .iter()
        .find(|(id, _)| *id == device)
        .map(|(_, name)| (*name).to_string())
        .unwrap_or_else(|| field(&doc, "driver").to_string());
    let missing: Vec<&str> = doc
        .get("missing")
        .and_then(Json::as_array)
        .map(|items| items.iter().map(|m| field(m, "id")).collect())
        .unwrap_or_default();
    let readiness = if doc.get("hardware").and_then(Json::as_bool) != Some(true) {
        Readiness::NoCard
    } else if field(&doc, "status") == "broken" {
        Readiness::Broken
    } else if missing.is_empty() && exit_code == 0 {
        Readiness::Ready
    } else if field(&doc, "status") == "untested" {
        Readiness::Untested
    } else if missing.contains(&"backend") {
        Readiness::NoService
    } else {
        Readiness::NotReady
    };
    Adapter { readiness, card }
}

// The Wi-Fi adapter fact's value, in the board's "Intel AX210, works with AirDrop" shape.
pub fn adapter_value(adapter: &Adapter, i18n: &I18n) -> String {
    let key = match adapter.readiness {
        Readiness::Checking => return i18n.tr("plugins.airdrop.adapter.checking"),
        Readiness::Missing => return i18n.tr("plugins.airdrop.adapter.missing"),
        Readiness::NoCard => return i18n.tr("plugins.airdrop.adapter.no_card"),
        Readiness::Ready => "plugins.airdrop.adapter.works",
        Readiness::Broken => "plugins.airdrop.adapter.broken",
        Readiness::Untested => "plugins.airdrop.adapter.untested",
        Readiness::NoService => "plugins.airdrop.adapter.no_service",
        Readiness::NotReady => "plugins.airdrop.adapter.not_ready",
    };
    i18n.tr_args(key, &[&adapter.card])
}

// One receiver that answered: its service ID, which `owly send --to` takes, and the name its owner gave it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Peer {
    pub id: String,
    pub name: String,
    // An iPhone, iPad or iPod draws the phone mark, anything else the laptop mark; owly prints no model.
    pub phone: bool,
}

// Sample input: `3a9f0c2be41d   "Jane's iPhone" ([fe80::1%awdl0]:8770)`, the name in Rust's Debug quoting.
pub fn browse_line(line: &str) -> Option<Peer> {
    let quote = line.find('"')?;
    let id = line[..quote].trim();
    let (name, rest) = quoted(&line[quote..])?;
    let rest = rest.trim();
    if id.is_empty() || name.is_empty() || !rest.starts_with('(') || !rest.ends_with(')') {
        return None;
    }
    let phone = browse_name_is_phone(&name);
    Some(Peer {
        id: id.into(),
        name,
        phone,
    })
}

// An iPhone, iPad or iPod by its owner's name for it, the only hint a device gives.
pub(crate) fn browse_name_is_phone(name: &str) -> bool {
    let lower = name.to_lowercase();
    ["iphone", "ipad", "ipod"].iter().any(|word| lower.contains(word))
}

// Sample input: `"Jane's \"B\" iPhone" ([fe80::1]:8770)`, a Rust Debug string first; gives the name and what follows it.
pub(crate) fn quoted(text: &str) -> Option<(String, &str)> {
    let mut name = String::new();
    let mut chars = text.strip_prefix('"')?.chars();
    loop {
        match chars.next()? {
            '"' => return Some((name, chars.as_str())),
            '\\' => name.push(unescape(&mut chars)?),
            c => name.push(c),
        }
    }
}

fn unescape(chars: &mut std::str::Chars) -> Option<char> {
    Some(match chars.next()? {
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        '0' => '\0',
        'u' => {
            if chars.next()? != '{' {
                return None;
            }
            let hex: String = chars.by_ref().take_while(|c| *c != '}').collect();
            char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?
        }
        other => other,
    })
}

// What one `owly send` ended in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Delivered,
    Declined,
    // owly's own words after "failed: " or "owly: ", English diagnostic data.
    Failed(String),
}

// Sample input: stdout "delivered\n" exit 0; "declined\n" exit 1; "failed: timed out\n" exit 2; stderr "owly: no such receiver found" exit 2.
pub fn verdict(stdout: &str, stderr: &str, exit_code: i32) -> Verdict {
    let said = stdout
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or_default();
    match (exit_code, said) {
        (0, "delivered") => Verdict::Delivered,
        (1, "declined") => Verdict::Declined,
        _ => {
            let why = said
                .strip_prefix("failed: ")
                .map_or_else(|| reason(stderr), str::to_string);
            Verdict::Failed(why)
        }
    }
}

// Sample input: "\r1048576 / 2400000 bytes", one carriage-return step of owly's progress.
pub fn progress(step: &str) -> Option<(u64, u64)> {
    let (sent, rest) = step.trim().split_once(" / ")?;
    let total = rest.strip_suffix(" bytes")?;
    Some((sent.parse().ok()?, total.parse().ok()?))
}

// `owly radio up` says "awdl0 is up: fe80::...%awdl0 (mac). Ctrl-C takes it down." once the link carries traffic.
pub fn radio_is_up(line: &str) -> bool {
    line.split_once(" is up: ")
        .is_some_and(|(iface, _)| !iface.is_empty() && !iface.contains(' '))
}

// The last "owly: ..." line of a run that failed, its reason for the sentence; empty when it said none.
pub fn reason(stderr: &str) -> String {
    stderr
        .lines()
        .map(str::trim)
        .rfind(|line| line.starts_with("owly: "))
        .map_or(String::new(), |line| line["owly: ".len()..].to_string())
}

// One file names the file and the device; several are counted, as a LocalSend send says it.
pub fn sending(i18n: &I18n, peer: &str, paths: &[String]) -> String {
    let what = match paths {
        [only] => only.rsplit('/').next().unwrap_or(only).to_string(),
        _ => i18n.trn("items.count", paths.len() as i64),
    };
    i18n.tr_args("send.providers.airdrop_sending", &[&what, peer])
}

// The device's answer in the status bar; owly's own reason rides the argument slot untranslated.
pub fn said(i18n: &I18n, peer: &str, verdict: &Verdict) -> String {
    match verdict {
        Verdict::Delivered => i18n.tr("send.providers.airdrop_delivered"),
        Verdict::Declined => i18n.tr_args("send.providers.airdrop_declined", &[peer]),
        Verdict::Failed(reason) if reason.is_empty() => {
            i18n.tr("send.providers.airdrop_unfinished")
        }
        Verdict::Failed(reason) => i18n.tr_args("send.providers.airdrop_reason", &[reason]),
    }
}

// A radio that would not come up, in the window's own sentence.
pub fn radio_failed(i18n: &I18n, stderr: &str) -> String {
    match reason(stderr) {
        why if why.is_empty() => i18n.tr("send.providers.airdrop_radio_failed"),
        why => i18n.tr_args("send.providers.airdrop_reason", &[&why]),
    }
}

// Sample output: {"c":"airdrop","op":"send","id":3,"peer":"3a9f0c2be41d","name":"Jane's iPhone","paths":["/a/b.png"]}, sent by service ID.
pub fn send_line(id: u64, peer: &str, name: &str, paths: &[String]) -> String {
    let files: Vec<String> = paths
        .iter()
        .map(|p| format!("\"{}\"", crate::json::escape(p)))
        .collect();
    format!(
        r#"{{"c":"airdrop","op":"send","id":{id},"peer":"{}","name":"{}","paths":[{}]}}"#,
        crate::json::escape(peer),
        crate::json::escape(name),
        files.join(",")
    )
}

// Sample input: {"t":"airdropsent","id":3,"ok":false,"code":"declined","reason":""}, read as its id, code and reason.
pub fn sent(line: &str) -> Option<(u64, String, String)> {
    let doc = jsondoc::parse(line).ok()?;
    if doc.get("t").and_then(Json::as_str) != Some("airdropsent") {
        return None;
    }
    let id = doc
        .get("id")
        .and_then(Json::as_f64)
        .filter(|n| *n >= 0.0 && n.fract() == 0.0)? as u64;
    Some((
        id,
        field(&doc, "code").to_string(),
        field(&doc, "reason").to_string(),
    ))
}

// The status bar's sentence for a finished send; a cancel says nothing, the jobs list already showed it.
pub fn sent_said(i18n: &I18n, peer: &str, code: &str, reason: &str) -> Option<String> {
    Some(match code {
        "delivered" => said(i18n, peer, &Verdict::Delivered),
        "declined" => said(i18n, peer, &Verdict::Declined),
        "cancelled" => return None,
        "stale" => i18n.tr("send.providers.menu_stale"),
        "missing" => i18n.tr_args(
            "send.providers.airdrop_reason",
            &[&i18n.tr("plugins.airdrop.adapter.missing")],
        ),
        _ => said(i18n, peer, &Verdict::Failed(reason.to_string())),
    })
}

// Any of the owner, group and other execute bits, as a shell's PATH search asks for.
const EXECUTE_BITS: u32 = 0o111;

// A program's path in a PATH value, as a shell would find it, in exact text; None when no directory holds an executable one.
pub fn find(path: &std::ffi::OsStr, program: &str) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(path)
        .map(|dir| dir.join(program))
        .find(|candidate| {
            candidate
                .metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & EXECUTE_BITS != 0)
        })
        .map(|found| crate::bytename::ExactText::exact_text(found.as_path()).into_owned())
}

pub fn doctor_argv(owly: &str) -> Vec<String> {
    let mut argv = vec![
        "timeout".into(),
        "--signal=KILL".into(),
        DOCTOR_TIMEOUT_SECS.to_string(),
    ];
    argv.extend([owly.to_string(), "doctor".into(), "--json".into()]);
    argv
}

// The radio gives itself back after the window even if Flea is gone, so Wi-Fi never stays taken.
pub fn radio_argv(owly: &str) -> Vec<String> {
    vec![
        owly.into(),
        "radio".into(),
        "up".into(),
        "--for".into(),
        WINDOW_SECS.to_string(),
    ]
}

// The receiver is bounded by the same window and asks before each request; airdrop_ask answers it.
pub fn receive_argv(owly: &str, folder: &str) -> Vec<String> {
    vec![
        "timeout".into(),
        WINDOW_SECS.to_string(),
        owly.into(),
        "receive".into(),
        "--dir".into(),
        folder.into(),
    ]
}

pub fn browse_argv(owly: &str) -> Vec<String> {
    vec![
        owly.into(),
        "browse".into(),
        "--seconds".into(),
        BROWSE_SECS.to_string(),
    ]
}

// The device rides --to= and files go after "--", so no name starting with a dash is read as a flag.
pub fn send_argv(owly: &str, peer: &str, paths: &[String]) -> Vec<String> {
    let mut argv = vec![
        owly.to_string(),
        "send".into(),
        format!("--to={peer}"),
        "--".into(),
    ];
    argv.extend(paths.iter().cloned());
    argv
}

#[cfg(test)]
#[path = "airdrop_tests.rs"]
mod tests;
