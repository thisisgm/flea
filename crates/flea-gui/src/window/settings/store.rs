//! Settings writes use the core CLI's locked merge; polling and child processes stay off the UI thread.
use crate::g2::settings_pages::seams::Store;
use flea::{jsondoc::Json, ui_logic_settings_pages::Text};
use std::{
    cell::RefCell, io::Read, path::PathBuf, process::Command, rc::Rc, sync::mpsc, time::Duration,
};

pub type Patches = Rc<RefCell<Vec<Json>>>;

pub struct SessionStore {
    pub document: Json,
    pub patches: Patches,
}

impl Store for SessionStore {
    fn document(&self) -> &Json {
        &self.document
    }
    fn write(&mut self, patch: &Json) -> Result<(), Text> {
        self.document = flea::uistate::patched(&self.document, patch)
            .map_err(|_| Text::Key("settings.error.not_saved"))?;
        self.patches.borrow_mut().push(patch.clone());
        Ok(())
    }
}

pub enum Update {
    Corner(f32),
    Unreadable,
    Loaded {
        document: Json,
        through: u64,
        saved: bool,
    },
    Failed {
        through: u64,
        keep: bool,
    },
    FavouriteFailed {
        through: u64,
        detail: String,
    },
    FavouriteCommitted {
        through: u64,
    },
}

pub struct Worker {
    pub writes: mpsc::Sender<(u64, Json, Option<Json>)>,
    pub updates: async_channel::Receiver<Update>,
}

// Retry only failed fields, using any newer queued choice instead of resending an obsolete value.
pub(super) fn overlay_retry(retry: &mut Json, pending: &Json) {
    match (retry, pending) {
        (Json::Obj(fields), Json::Obj(changes)) => {
            for (key, value) in fields {
                if let Some((_, next)) = changes.iter().find(|(name, _)| name == key) {
                    overlay_retry(value, next);
                }
            }
        }
        (retry, pending) => *retry = pending.clone(),
    }
}

// A newer successful patch supersedes only the fields it wrote, including nested preference fields.
pub(super) fn unwritten(patch: &mut Json, written: &Json) {
    let (Json::Obj(fields), Json::Obj(changes)) = (patch, written) else {
        return;
    };
    fields.retain_mut(|(key, value)| {
        let Some((_, next)) = changes.iter().find(|(name, _)| name == key) else {
            return true;
        };
        if value.as_object().is_some() && next.as_object().is_some() {
            unwritten(value, next);
            value.as_object().is_some_and(|fields| !fields.is_empty())
        } else {
            false
        }
    });
}

// A reread keeps this window's own global sort, a per-window key, and then every patch it still owes.
pub(super) fn keep_own<'a>(document: &mut Json, own: &Json, owed: impl Iterator<Item = &'a Json>) {
    if let Json::Obj(fields) = document {
        fields.retain(|(key, _)| key != "sort");
        if let Some(sort) = own.get("sort") {
            fields.push(("sort".into(), sort.clone()));
        }
    }
    for patch in owed {
        if let Ok(next) = flea::uistate::patched(document, patch) {
            *document = next;
        }
    }
}

impl Worker {
    pub fn start() -> std::io::Result<Self> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let state = std::env::var_os("XDG_STATE_HOME")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/state"));
        Self::at(state.join("flea/ui.json"), crate::window::startup::binary())
    }

    pub fn at(path: PathBuf, binary: PathBuf) -> std::io::Result<Self> {
        let (writes, queue) = mpsc::channel::<(u64, Json, Option<Json>)>();
        let (send, updates) = async_channel::unbounded();
        let corners = send.clone();
        let compositor = crate::compositor::Service::start(move |corner| {
            corners.send_blocking(Update::Corner(corner)).is_ok()
        })?;
        std::thread::Builder::new()
            .name("flea-settings".into())
            .spawn(move || {
                let _compositor = compositor;
                let mut last = None;
                let mut through = 0;
                let mut read_failed = false;
                let mut initial = true;
                loop {
                    let request = if std::mem::take(&mut initial) {
                        Err(mpsc::RecvTimeoutError::Timeout)
                    } else {
                        queue.recv_timeout(Duration::from_millis(200))
                    };
                    match request {
                        Ok((id, patch, operation)) => {
                            through = id;
                            let (mode, value) = match &operation {
                                Some(op) => ("--favourites", op),
                                None => ("--ui-state", &patch),
                            };
                            let output = Command::new(&binary)
                                .args([mode, flea::jsondoc::render(value).trim_end()])
                                .output();
                            let document = output
                                .as_ref()
                                .ok()
                                .filter(|o| o.status.success())
                                .and_then(|o| {
                                    // Sample response: {"places":{"favourites":[{"label":"Personal","path":"smb://host/share/"}]}}.
                                    flea::jsondoc::parse(&String::from_utf8_lossy(&o.stdout)).ok()
                                })
                                .filter(|document| {
                                    operation.is_none()
                                        || document
                                            .get("places")
                                            .and_then(|places| places.get("favourites"))
                                            .and_then(Json::as_array)
                                            .is_some()
                                });
                            let update = match document {
                                Some(document) => Update::Loaded {
                                    document,
                                    through,
                                    saved: operation.is_none(),
                                },
                                None if operation.is_some()
                                    && output
                                        .as_ref()
                                        .is_ok_and(|output| output.status.success()) =>
                                {
                                    Update::FavouriteCommitted { through }
                                }
                                None if operation.is_some() => Update::FavouriteFailed {
                                    through,
                                    detail: output.as_ref().ok().map_or_else(
                                        String::new,
                                        |output| {
                                            // Sample stderr: "flea: /fixture/ui.json could not be read, so no settings were written\n".
                                            let stderr = String::from_utf8_lossy(&output.stderr);
                                            let line =
                                                stderr.lines().next().unwrap_or_default().trim();
                                            line.strip_prefix("flea: ").unwrap_or(line).into()
                                        },
                                    ),
                                },
                                None => Update::Failed {
                                    through,
                                    keep: operation.is_none(),
                                },
                            };
                            last = None;
                            if send.send_blocking(update).is_err() {
                                break;
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            let body = match read_document(&path) {
                                Ok(body) => body,
                                Err(()) => {
                                    if !read_failed {
                                        let _ = send.send_blocking(Update::Unreadable);
                                        read_failed = true;
                                    }
                                    continue;
                                }
                            };
                            read_failed = false;
                            if body == last {
                                continue;
                            }
                            last = body.clone();
                            let Some(body) = body else { continue };
                            let document = flea::uistate::from_file(&body);
                            if send
                                .send_blocking(Update::Loaded {
                                    document,
                                    through,
                                    saved: false,
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            })?;
        Ok(Self { writes, updates })
    }
}

fn read_document(path: &std::path::Path) -> Result<Option<String>, ()> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
        Ok(_) => {}
    }
    let file = flea::backend::regfile::open_regular(path).ok_or(())?;
    let mut body = String::new();
    file.take(4 * 1024 * 1024 + 1)
        .read_to_string(&mut body)
        .map_err(|_| ())?;
    if body.len() > 4 * 1024 * 1024
        || flea::jsondoc::parse(&body)
            .ok()
            .is_none_or(|value| value.as_object().is_none())
    {
        return Err(());
    }
    Ok(Some(body))
}
