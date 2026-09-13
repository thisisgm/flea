use crate::jsondoc::{self, Json};
use std::io::{self, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

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
    pub submenu: bool,
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
            submenu: false,
            sending: None,
            sent: None,
        }
    }
    pub fn refresh(&mut self) {
        if self.child.is_some() {
            return;
        }
        self.peers.clear();
        self.error.clear();
        self.result = None;
        match Command::new("tailscale")
            .args(["status", "--json"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(mut child) => {
                let Some(output) = child.stdout.take() else {
                    self.error = "Taildrop status output unavailable".into();
                    return;
                };
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    const MAX_STATUS_BYTES: u64 = 4 * 1024 * 1024;
                    let mut bytes = Vec::new();
                    let result = output
                        .take(MAX_STATUS_BYTES + 1)
                        .read_to_end(&mut bytes)
                        .and_then(|_| {
                            if bytes.len() as u64 > MAX_STATUS_BYTES {
                                Err(io::Error::other("Taildrop status exceeds its limit"))
                            } else {
                                Ok(bytes)
                            }
                        });
                    let _ = tx.send(result);
                });
                self.child = Some(child);
                self.output = Some(rx);
                self.started = Instant::now();
            }
            Err(_) => self.error = "Taildrop unavailable: tailscale could not start".into(),
        }
    }
    pub fn loading(&self) -> bool {
        self.child.is_some()
    }
    pub fn reason(&self) -> &str {
        if self.sending.is_some() { "sending" }
        else if self.loading() { "checking" }
        else { &self.error }
    }
    pub fn current_peer(&self, selected: &Peer) -> Option<Peer> {
        self.peers.iter().find(|peer| peer.id == selected.id && peer.address == selected.address).cloned()
    }
    pub fn poll(&mut self) {
        let Some(child) = &mut self.child else { return };
        if let Some(label) = &self.sending {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.sent = Some(if status.success() {
                        Ok(format!("Sent to {}", label))
                    } else {
                        Err(format!("Taildrop to {} failed ({})", label, status))
                    });
                    self.child = None;
                    self.sending = None;
                }
                Err(e) => {
                    self.sent = Some(Err(format!("Taildrop process: {}", e)));
                    let _ = child.kill();
                    let _ = child.wait();
                    self.child = None;
                    self.sending = None;
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
            self.error = "Taildrop status timed out".into();
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
                self.error = "Taildrop status process could not be observed".into();
                return;
            }
        };
        let Some(result) = self.result.take() else {
            return;
        };
        self.child = None;
        self.output = None;
        match (status, result) {
            (status, Ok(bytes)) if status.success() => {
                match jsondoc::parse(&String::from_utf8_lossy(&bytes)) {
                    Ok(value) => match available_peers(&value) {
                        Ok(peers) => self.peers = peers,
                        Err(reason) => self.error = reason,
                    },
                    Err(_) => self.error = "Taildrop status could not be read".into(),
                }
            }
            _ => self.error = "Taildrop status failed".into(),
        }
    }
    pub fn send(&mut self, peer: &Peer, paths: &[String]) -> io::Result<()> {
        if self.child.is_some() {
            return Err(io::Error::other("Taildrop is already busy"));
        }
        if paths.is_empty()
            || paths
                .iter()
                .any(|path| !std::path::Path::new(path).is_absolute())
        {
            return Err(io::Error::other(
                "Taildrop needs an absolute file selection",
            ));
        }
        self.child = Some(
            Command::new("omarchy-tailscale-send")
                .arg(&peer.address)
                .args(paths)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        );
        self.sending = Some(peer.label.clone());
        self.sent = None;
        Ok(())
    }
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
fn available_peers(value: &Json) -> Result<Vec<Peer>, String> {
    match text(value, "BackendState") {
        "Running" => {}
        "NeedsLogin" => return Err("signed out".into()),
        "" => return Err("unavailable".into()),
        state => return Err(state.to_lowercase()),
    }
    let capability = "https://tailscale.com/cap/file-sharing";
    let owner = value.get("Self");
    let mapped = owner.and_then(|owner| owner.get("CapMap")).and_then(|map| map.get(capability)).is_some();
    let listed = owner.and_then(|owner| owner.get("Capabilities")).and_then(Json::as_array)
        .unwrap_or(&[]).iter().any(|value| value.as_str() == Some(capability));
    if !mapped && !listed { return Err("disabled for this account".into()); }
    let peers = peers(value);
    if peers.is_empty() { Err("no peers".into()) } else { Ok(peers) }
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
    out.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refreshed_target_keeps_node_and_address_identity() {
        let mut taildrop = Taildrop::new();
        let selected = Peer { id: "node-one".into(), label: "before".into(), address: "alpha.invalid".into() };
        taildrop.peers = vec![Peer { label: "renamed".into(), ..selected.clone() }];
        assert_eq!(taildrop.current_peer(&selected).unwrap().label, "renamed");
        taildrop.peers[0].id = "node-two".into();
        assert!(taildrop.current_peer(&selected).is_none());
        taildrop.peers[0] = Peer { address: "beta.invalid".into(), ..selected.clone() };
        assert!(taildrop.current_peer(&selected).is_none());
        taildrop.peers.clear();
        assert!(taildrop.current_peer(&selected).is_none());
    }

    #[test]
    fn provider_state_overrides_stale_peer_eligibility() {
        let fixture = r#"{"BackendState":"Running","Self":{"Capabilities":["https://tailscale.com/cap/file-sharing"]},"Peer":{"old":{"Online":true,"TaildropTarget":1,"HostName":"alpha"}}}"#;
        assert_eq!(available_peers(&jsondoc::parse(fixture).unwrap()).unwrap()[0].label, "alpha");
        for (state, reason) in [("NeedsLogin", "signed out"), ("Stopped", "stopped"), ("", "unavailable")] {
            let value = jsondoc::parse(&fixture.replace("Running", state)).unwrap();
            assert_eq!(available_peers(&value).err().as_deref(), Some(reason));
        }
        let disabled = jsondoc::parse(&fixture.replace("https://tailscale.com/cap/file-sharing", "unrelated")).unwrap();
        assert_eq!(available_peers(&disabled).err().as_deref(), Some("disabled for this account"));
        let mapped = fixture.replace(r#""Capabilities":["https://tailscale.com/cap/file-sharing"]"#,
                                     r#""CapMap":{"https://tailscale.com/cap/file-sharing":[]}"#);
        assert_eq!(available_peers(&jsondoc::parse(&mapped).unwrap()).unwrap().len(), 1);
        let empty = jsondoc::parse(&fixture.replace(r#""Online":true"#, r#""Online":false"#)).unwrap();
        assert_eq!(available_peers(&empty).err().as_deref(), Some("no peers"));
    }

    #[test]
    fn targets_match_oem_eligibility_and_refuse_option_names() {
        let value = jsondoc::parse(r#"{"Self":{"UserID":1},"Peer":{"a":{"Online":true,"UserID":1,"HostName":"alpha"},"b":{"Online":false,"TaildropTarget":1,"HostName":"offline"},"c":{"Online":true,"TaildropTarget":2,"HostName":"denied"},"d":{"Online":true,"TaildropTarget":1,"DNSName":"exit.mullvad.ts.net."},"e":{"Online":true,"TaildropTarget":1,"HostName":"--help"},"f":{"Online":true,"TaildropTarget":1,"HostName":"localhost","DNSName":"beta.tail.ts.net."}}}"#).unwrap();
        let found = peers(&value);
        assert_eq!(
            found.iter().map(|p| p.label.as_str()).collect::<Vec<_>>(),
            vec!["alpha", "beta"]
        );
        assert_eq!(found[1].address, "beta.tail.ts.net");
        assert!(peers(&Json::Null).is_empty());
    }
}
