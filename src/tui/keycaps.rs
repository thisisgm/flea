//! The active keyboard group behind every key label, read from Hyprland's devices answer off the loop.
use crate::jsondoc::{self, Json};
use flea::keys::display::{KeycapError, KeycapSnapshot};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::Duration;

// The first wait before a lost or missing socket2 subscription is retried, and the longest.
const RECONNECT_FIRST: Duration = Duration::from_millis(50);
const RECONNECT_CAP: Duration = Duration::from_secs(1);
// How long start waits for the first readback, so the first key sheet and hints draw the active group.
pub(super) const FIRST_WAIT: Duration = Duration::from_millis(50);
// A hung compositor ends the readback here; a devices answer is a few KB, so the cap only stops a runaway.
const QUERY_TIMEOUT: Duration = Duration::from_millis(500);
const QUERY_BYTES: u64 = 1 << 20;
// XKB holds at most four groups, so active_layout_index is 0 to 3.
const XKB_GROUPS: f64 = 4.;

/// The XKB configuration and active group of the main keyboard, as the compositor reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    pub layout: String,
    pub variant: String,
    pub group: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Fault {
    // An ssh session's keyboard belongs to the client, which no server-side source can read.
    Remote,
    NoCompositor,
    Query(String),
    Malformed(&'static str),
    Keycap(KeycapError),
    Disconnected,
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Remote => write!(f, "ssh session, the client's keyboard is not readable here"),
            Self::NoCompositor => write!(f, "no Hyprland session"),
            Self::Query(error) => write!(f, "devices readback failed: {error}"),
            Self::Malformed(reason) => write!(f, "{reason}"),
            Self::Keycap(error) => write!(f, "{error}"),
            Self::Disconnected => write!(f, "Hyprland event socket closed"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Status {
    Waiting,
    Active,
    Failed(Fault),
}

pub(super) type Query = Arc<dyn Fn() -> Result<Vec<u8>, String> + Send + Sync>;
pub(super) type Subscribe = Box<dyn FnMut() -> Option<UnixStream> + Send>;
type Reading = Result<Identity, Fault>;

pub(super) struct Keycaps {
    readings: Option<mpsc::Receiver<Reading>>,
    sender: Option<mpsc::Sender<Reading>>,
    query: Option<Query>,
    stopped: Arc<AtomicBool>,
    asking: Arc<AtomicBool>,
    published: Option<Identity>,
    generation: u64,
    pub(super) status: Status,
    trace: bool,
}

// Hyprland's directory for this session's sockets, absent outside one.
fn instance() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|value| !value.is_empty())
        .zip(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").filter(|value| !value.is_empty()))
        .map(|(runtime, signature)| PathBuf::from(runtime).join("hypr").join(signature))
}

fn remote() -> bool {
    ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"]
        .iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

// The same request hyprctl -j devices sends, over the request socket, so no process starts.
fn devices(socket: &std::path::Path) -> Result<Vec<u8>, String> {
    let mut stream = UnixStream::connect(socket).map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(QUERY_TIMEOUT))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(QUERY_TIMEOUT))
        .map_err(|error| error.to_string())?;
    stream
        .write_all(b"j/devices")
        .map_err(|error| error.to_string())?;
    let mut answer = Vec::new();
    stream
        .take(QUERY_BYTES)
        .read_to_end(&mut answer)
        .map_err(|error| error.to_string())?;
    Ok(answer)
}

// Sample input: {"keyboards":[{"name":"kb","layout":"us,fr","variant":",","active_keymap":"French","main":true}]}.
pub(super) fn identity(answer: &[u8]) -> Reading {
    let text =
        std::str::from_utf8(answer).map_err(|_| Fault::Malformed("devices answer is not UTF-8"))?;
    let value = jsondoc::parse(text).map_err(|_| Fault::Malformed("devices answer is not JSON"))?;
    let keyboards = value
        .get("keyboards")
        .and_then(Json::as_array)
        .ok_or(Fault::Malformed("devices answer has no keyboards"))?;
    let keyboard = keyboards
        .iter()
        .find(|keyboard| keyboard.get("main").and_then(Json::as_bool) == Some(true))
        .or_else(|| keyboards.first())
        .ok_or(Fault::Malformed("devices answer has no keyboards"))?;
    let layout = keyboard
        .get("layout")
        .and_then(Json::as_str)
        .ok_or(Fault::Malformed("keyboard has no layout"))?;
    let variant = keyboard.get("variant").and_then(Json::as_str).unwrap_or("");
    let group = match keyboard.get("active_layout_index").and_then(Json::as_f64) {
        Some(index) if index.fract() == 0. && (0. ..XKB_GROUPS).contains(&index) => index as u32,
        Some(_) => {
            return Err(Fault::Malformed(
                "keyboard has an invalid active_layout_index",
            ))
        }
        // Older Hyprland names the active group only by its XKB description.
        None => {
            let name = keyboard
                .get("active_keymap")
                .and_then(Json::as_str)
                .ok_or(Fault::Malformed("keyboard has no active keymap"))?;
            let named = KeycapSnapshot::from_group_name(name, 0, 0).map_err(Fault::Keycap)?;
            let variants: Vec<&str> = variant.split(',').collect();
            let position = layout
                .split(',')
                .enumerate()
                .position(|(index, code)| {
                    code == named.layout()
                        && variants.get(index).copied().unwrap_or("") == named.variant()
                })
                .ok_or_else(|| Fault::Keycap(KeycapError::UnknownGroupName(name.to_owned())))?;
            position as u32
        }
    };
    Ok(Identity {
        layout: layout.to_owned(),
        variant: variant.to_owned(),
        group,
    })
}

// Sample input: activelayout>>at-translated-set-2-keyboard,French; layout switches and reloads ask again.
fn changes_layout(line: &[u8]) -> bool {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.starts_with(b"activelayout>>") || line == b"configreloaded>>"
}

fn read(query: &Query) -> Reading {
    query()
        .map_err(Fault::Query)
        .and_then(|answer| identity(&answer))
}

impl Keycaps {
    pub(super) fn start() -> Self {
        let mut keycaps = match instance() {
            _ if remote() => Self::unsupported(Fault::Remote),
            Some(directory) => {
                let request = directory.join(".socket.sock");
                let events = directory.join(".socket2.sock");
                Self::at(
                    Arc::new(move || devices(&request)),
                    Box::new(move || UnixStream::connect(&events).ok()),
                )
            }
            None => Self::unsupported(Fault::NoCompositor),
        };
        keycaps.trace = std::env::var("FLEA_TUI_KEYCAPS").as_deref() == Ok("1");
        keycaps.report();
        keycaps
    }

    pub(super) fn unsupported(fault: Fault) -> Self {
        Self {
            readings: None,
            sender: None,
            query: None,
            stopped: Arc::default(),
            asking: Arc::default(),
            published: None,
            generation: 0,
            status: Status::Failed(fault),
            trace: false,
        }
    }

    // Subscribes before each readback, so a switch racing the query is not lost; Drop ends the retries.
    pub(super) fn at(query: Query, mut subscribe: Subscribe) -> Self {
        let (sender, readings) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let keycaps = Self {
            readings: Some(readings),
            sender: Some(sender.clone()),
            query: Some(query.clone()),
            stopped: stopped.clone(),
            asking: Arc::default(),
            published: None,
            generation: 0,
            status: Status::Waiting,
            trace: false,
        };
        let spawned = std::thread::Builder::new()
            .name("flea-keycaps".into())
            .spawn(move || {
                let mut first = true;
                let mut backoff = RECONNECT_FIRST;
                while !stopped.load(Ordering::Relaxed) {
                    let stream = subscribe();
                    if (first || stream.is_some()) && sender.send(read(&query)).is_err() {
                        return;
                    }
                    first = false;
                    if let Some(stream) = stream {
                        backoff = RECONNECT_FIRST;
                        let mut reader = BufReader::new(stream);
                        let mut line = Vec::new();
                        loop {
                            line.clear();
                            match reader.read_until(b'\n', &mut line) {
                                Ok(0) | Err(_) => break,
                                Ok(_) if changes_layout(&line) => {
                                    if stopped.load(Ordering::Relaxed)
                                        || sender.send(read(&query)).is_err()
                                    {
                                        return;
                                    }
                                }
                                Ok(_) => {}
                            }
                        }
                        if stopped.load(Ordering::Relaxed)
                            || sender.send(Err(Fault::Disconnected)).is_err()
                        {
                            return;
                        }
                    }
                    // Retry the subscription only, never poll the layout.
                    std::thread::sleep(backoff);
                    backoff = (backoff * 2).min(RECONNECT_CAP);
                }
            });
        if spawned.is_err() {
            return Self::unsupported(Fault::Query(
                "could not start the keyboard layout reader".into(),
            ));
        }
        keycaps
    }

    // A resume asks once more off the loop; a readback still in flight already answers it.
    pub(super) fn resume(&self) {
        let (Some(query), Some(sender)) = (&self.query, &self.sender) else {
            return;
        };
        if self.asking.swap(true, Ordering::Relaxed) {
            return;
        }
        let (query, sender, asking) = (query.clone(), sender.clone(), self.asking.clone());
        let spawned = std::thread::Builder::new()
            .name("flea-keycaps-resume".into())
            .spawn(move || {
                let reading = read(&query);
                asking.store(false, Ordering::Relaxed);
                let _ = sender.send(reading);
            });
        if spawned.is_err() {
            self.asking.store(false, Ordering::Relaxed);
        }
    }

    // Waits up to limit for the first readback, then applies whatever has arrived.
    pub(super) fn settle(
        &mut self,
        model: &mut super::model::Model,
        map: &super::keymap::Map,
        limit: Duration,
    ) {
        let first = self
            .readings
            .as_ref()
            .and_then(|readings| readings.recv_timeout(limit).ok());
        if let Some(reading) = first {
            self.take(reading, model, map);
        }
        self.apply(model, map);
    }

    // Publishes each complete snapshot through the model's setter; a failure keeps the last valid epoch.
    pub(super) fn apply(&mut self, model: &mut super::model::Model, map: &super::keymap::Map) {
        let readings: Vec<Reading> = self
            .readings
            .as_ref()
            .map(|readings| readings.try_iter().collect())
            .unwrap_or_default();
        for reading in readings {
            self.take(reading, model, map);
        }
    }

    fn take(
        &mut self,
        reading: Reading,
        model: &mut super::model::Model,
        map: &super::keymap::Map,
    ) {
        let before = (self.status.clone(), self.generation);
        self.status = match reading {
            Ok(identity) if self.published.as_ref() == Some(&identity) => Status::Active,
            Ok(identity) => {
                let generation = self.generation + 1;
                match KeycapSnapshot::new(
                    &identity.layout,
                    Some(&identity.variant),
                    Some(identity.group),
                    generation,
                )
                .and_then(|snapshot| model.set_keycaps(snapshot, map))
                {
                    Ok(()) => {
                        self.generation = generation;
                        self.published = Some(identity);
                        Status::Active
                    }
                    Err(error) => Status::Failed(Fault::Keycap(error)),
                }
            }
            Err(fault) => Status::Failed(fault),
        };
        if (&self.status, self.generation) != (&before.0, before.1) {
            self.report();
        }
    }

    // FLEA_TUI_KEYCAPS=1 names each status change on stderr, the same way FLEA_TUI_CAPABILITIES traces negotiation.
    fn report(&self) {
        if !self.trace {
            return;
        }
        let state = match (&self.status, &self.published) {
            (Status::Failed(fault), _) => format!("failed: {fault}"),
            (Status::Waiting, _) => "waiting".to_owned(),
            (Status::Active, Some(identity)) => format!(
                "active {} ({}) group {}",
                identity.layout, identity.variant, identity.group
            ),
            (Status::Active, None) => "active".to_owned(),
        };
        let _ = writeln!(
            std::io::stderr().lock(),
            "flea: keycaps {state}, generation {}",
            self.generation
        );
    }
}

impl Drop for Keycaps {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
#[path = "keycaps_tests.rs"]
mod tests;
