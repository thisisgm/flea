use super::words::Text;
use crate::i18n::I18n;
use crate::jsondoc::{self, Json};
use std::io::{self, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const UNREAD: &str = "tui.taildrop.unread";
const TOO_LARGE: &str = "tui.taildrop.too_large";
const MAX_STATUS_BYTES: u64 = 4 * 1024 * 1024;
const SENDER: &str = "omarchy-tailscale-send";

// The reader thread: the status call's whole answer, or InvalidData once it passes its limit.
fn read_status(output: impl Read + Send + 'static) -> mpsc::Receiver<io::Result<Vec<u8>>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = output
            .take(MAX_STATUS_BYTES + 1)
            .read_to_end(&mut bytes)
            .and_then(|_| {
                if bytes.len() as u64 > MAX_STATUS_BYTES {
                    Err(io::Error::new(io::ErrorKind::InvalidData, TOO_LARGE))
                } else {
                    Ok(bytes)
                }
            });
        let _ = tx.send(result);
    });
    rx
}

#[derive(Clone)]
pub struct Peer {
    pub id: String,
    pub label: String,
    pub address: String,
}

pub struct Taildrop {
    child: Option<Child>,
    output: Option<Receiver<io::Result<Vec<u8>>>>,
    result: Option<io::Result<Vec<u8>>>,
    started: Instant,
    pub peers: Vec<Peer>,
    pub error: String,
    problem: Option<Text>,
    state_key: &'static str,
    i18n: Option<I18n>,
    outcome: Option<Result<Text, Text>>,
    sending: Option<String>,
    pub sent: Option<Result<String, String>>,
}

impl Taildrop {
    pub fn new() -> Self {
        Self {
            child: None,
            output: None,
            result: None,
            started: Instant::now(),
            peers: Vec::new(),
            error: String::new(),
            problem: None,
            state_key: "menu.state.offline",
            i18n: None,
            outcome: None,
            sending: None,
            sent: None,
        }
    }
    pub fn refresh_locale(&mut self, i18n: &I18n) {
        self.i18n = Some(i18n.clone());
        if let Some(words) = &self.problem {
            self.error = words.render(i18n);
        }
        if self.sent.is_some() {
            self.sent = self.outcome.as_ref().map(|result| match result {
                Ok(text) => Ok(text.render(i18n)),
                Err(text) => Err(text.render(i18n)),
            });
        }
    }
    pub fn state_key(&self) -> &'static str {
        self.state_key
    }
    pub fn status(&mut self, value: &Json) {
        match available_peers(value) {
            Ok(peers) => {
                self.peers = peers;
                self.error.clear();
                self.problem = None;
            }
            Err(key) => {
                self.peers.clear();
                self.fault(key);
            }
        }
    }
    pub fn take_sent(&mut self) -> Option<Result<Text, Text>> {
        self.sent = None;
        self.outcome.take()
    }
    fn fault(&mut self, key: &'static str) {
        self.problem = Some(Text::Key(key));
        self.error = self
            .i18n
            .as_ref()
            .map_or_else(String::new, |i18n| i18n.tr(key));
        self.state_key = match key {
            "menu.state.signedOut" | "menu.state.noDevices" => key,
            _ => "menu.state.offline",
        };
    }
    fn sent(&mut self, result: Result<Text, Text>) {
        self.sent = self.i18n.as_ref().map(|i18n| match &result {
            Ok(text) => Ok(text.render(i18n)),
            Err(text) => Err(text.render(i18n)),
        });
        self.outcome = Some(result);
    }
    pub fn refresh(&mut self) {
        if self.child.is_some() {
            return;
        }
        self.peers.clear();
        self.error.clear();
        self.problem = None;
        self.state_key = "menu.state.noDevices";
        self.result = None;
        match crate::backend::locale::command("tailscale")
            .args(["status", "--json"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(mut child) => {
                let Some(output) = child.stdout.take() else {
                    self.fault(UNREAD);
                    return;
                };
                self.child = Some(child);
                self.output = Some(read_status(output));
                self.started = Instant::now();
            }
            Err(_) => self.fault("tui.taildrop.unavailable"),
        }
    }
    pub fn loading(&self) -> bool {
        self.child.is_some()
    }
    pub fn current_peer(&self, selected: &Peer) -> Option<Peer> {
        self.peers
            .iter()
            .find(|peer| peer.id == selected.id && peer.address == selected.address)
            .cloned()
    }
    pub fn poll(&mut self) {
        let Some(child) = &mut self.child else { return };
        if let Some(label) = &self.sending {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let result = if status.success() {
                        Ok(Text::Args("tui.taildrop.sent", vec![label.clone().into()]))
                    } else {
                        Err(Text::Args(
                            "tui.taildrop.failed",
                            vec![label.clone().into(), status.to_string().into()],
                        ))
                    };
                    self.sent(result);
                    self.child = None;
                    self.sending = None;
                }
                Err(e) => {
                    let result = Text::Args(
                        "tui.taildrop.failed",
                        vec![label.clone().into(), crate::error::io_message(&e).into()],
                    );
                    let _ = child.kill();
                    let _ = child.wait();
                    self.child = None;
                    self.sending = None;
                    self.sent(Err(result));
                }
                Ok(None) => {}
            }
            return;
        }
        const STATUS_DEADLINE: Duration = Duration::from_secs(5);
        if self.started.elapsed() > STATUS_DEADLINE {
            let _ = child.kill();
            let _ = child.wait();
            self.child = None;
            self.output = None;
            self.fault("tui.taildrop.timeout");
            return;
        }
        if self.result.is_none() {
            self.result = self.output.as_ref().and_then(|rx| rx.try_recv().ok());
        }
        if self.result.as_ref().is_some_and(Result::is_err) {
            let _ = child.kill();
        }
        let status = match child.try_wait() {
            Ok(Some(status)) => status,
            Ok(None) => return,
            Err(_) => {
                self.fault(UNREAD);
                return;
            }
        };
        let Some(result) = self.result.take() else {
            return;
        };
        self.child = None;
        self.output = None;
        match (status, result) {
            // A reply past its limit or unreadable had its child killed, so its exit status says nothing more.
            (_, Err(error)) if error.kind() == io::ErrorKind::InvalidData => self.fault(TOO_LARGE),
            (_, Err(_)) => self.fault(UNREAD),
            (status, Ok(_)) if !status.success() => self.fault("tui.taildrop.status_failed"),
            (_, Ok(bytes)) => match jsondoc::parse(&String::from_utf8_lossy(&bytes)) {
                Ok(value) => self.status(&value),
                Err(_) => self.fault(UNREAD),
            },
        }
    }
    pub fn send(&mut self, peer: &Peer, paths: &[String]) -> io::Result<()> {
        self.send_with(SENDER, peer, paths)
    }
    fn send_with(&mut self, sender: &str, peer: &Peer, paths: &[String]) -> io::Result<()> {
        if self.child.is_some() {
            return Err(io::Error::other(Text::Key("tui.taildrop.busy")));
        }
        if paths.is_empty()
            || paths
                .iter()
                .any(|path| !std::path::Path::new(path).is_absolute())
        {
            return Err(io::Error::other(Text::Key("tui.taildrop.absolute")));
        }
        self.child = Some(
            Command::new(sender)
                .arg(&peer.address)
                .args(flea::bytename::args(paths))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|error| spawn_failure(sender, &peer.label, &error))?,
        );
        self.sending = Some(peer.label.clone());
        self.sent = None;
        self.outcome = None;
        Ok(())
    }
}

// A sender that cannot start names Taildrop, the peer, the sender program and the OS cause, since tailscale itself may be running fine.
fn spawn_failure(sender: &str, label: &str, error: &io::Error) -> io::Error {
    let cause = format!("{sender}: {}", crate::error::io_message(error));
    io::Error::other(Text::Args(
        "jobs.taildrop.reason",
        vec![Text::Args(
            "tui.taildrop.failed",
            vec![label.to_string().into(), cause.into()],
        )],
    ))
}

impl Drop for Taildrop {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if self.sending.is_some() {
                // The OEM sender owns its notification and continues after the TUI closes.
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            } else {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

fn text<'a>(value: &'a Json, key: &str) -> &'a str {
    value.get(key).and_then(Json::as_str).unwrap_or("")
}

// Sample input: {"BackendState":"Running","Self":{"CapMap":{"https://tailscale.com/cap/file-sharing":[]}},"Peer":{}}.
fn available_peers(value: &Json) -> Result<Vec<Peer>, &'static str> {
    match text(value, "BackendState") {
        "Running" => {}
        "NeedsLogin" => return Err("menu.state.signedOut"),
        "" => return Err("menu.state.offline"),
        _ => return Err("menu.state.offline"),
    }
    let capability = "https://tailscale.com/cap/file-sharing";
    let owner = value.get("Self");
    let mapped = owner
        .and_then(|owner| owner.get("CapMap"))
        .and_then(|map| map.get(capability))
        .is_some();
    let listed = owner
        .and_then(|owner| owner.get("Capabilities"))
        .and_then(Json::as_array)
        .unwrap_or(&[])
        .iter()
        .any(|value| value.as_str() == Some(capability));
    if !mapped && !listed {
        return Err("tui.taildrop.disabled");
    }
    let peers = peers(value);
    if peers.is_empty() {
        Err("menu.state.noDevices")
    } else {
        Ok(peers)
    }
}

// Sample input: {"Self":{"UserID":1},"Peer":{"node":{"Online":true,"TaildropTarget":1,"DNSName":"host.tail.ts.net."}}}.
fn peers(value: &Json) -> Vec<Peer> {
    let owner = value.get("Self").and_then(|s| s.get("UserID"));
    let mut out = Vec::new();
    for (id, peer) in value.get("Peer").and_then(Json::as_object).unwrap_or(&[]) {
        if peer.get("Online").and_then(Json::as_bool) != Some(true) {
            continue;
        }
        let host = text(peer, "HostName");
        let dns = text(peer, "DNSName").trim_end_matches('.');
        if [host, dns]
            .iter()
            .any(|s| s.to_lowercase().ends_with(".mullvad.ts.net"))
        {
            continue;
        }
        let target = peer
            .get("TaildropTarget")
            .and_then(Json::as_f64)
            .unwrap_or(0.0);
        if target != 0.0 {
            if target != 1.0 {
                continue;
            }
        } else if owner.is_none() || peer.get("UserID") != owner {
            continue;
        }
        let address = if !dns.is_empty() {
            dns
        } else if !host.is_empty() {
            host
        } else {
            peer.get("TailscaleIPs")
                .and_then(Json::as_array)
                .unwrap_or(&[])
                .iter()
                .filter_map(Json::as_str)
                .find(|ip| ip.starts_with("100."))
                .unwrap_or("")
        };
        if address.is_empty() || address.starts_with('-') || address.chars().any(char::is_control) {
            continue;
        }
        let label = if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
            host
        } else {
            dns.split('.')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or(address)
        };
        out.push(Peer {
            id: id.clone(),
            label: label.into(),
            address: address.into(),
        });
    }
    out.sort_by_key(|a| a.label.to_lowercase());
    out
}

#[cfg(test)]
#[path = "taildrop_words_tests.rs"]
mod tests;
