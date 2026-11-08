// Work descriptions stay beside their jobs so a retry retains the original arguments and selection.
use super::archive::Formats;
use super::collide::Policy;
use super::menu_actions::Selected;
use super::opsreq::OpMsg;
use super::undo::ItemIdentity;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::Arc;
#[path = "airdropsend.rs"]
pub(crate) mod airdropsend;

pub(crate) struct Spec<'a> {
    pub kind: &'a str,
    pub label: &'a str,
    pub target: Option<PathBuf>,
}

pub(crate) enum Work {
    Rclone(super::rclonerun::Send),
    Transfer {
        moving: bool,
        paths: Vec<String>,
        dest: PathBuf,
        selection: Option<Vec<Selected>>,
        destination: Option<Selected>,
        policy: Policy,
    },
    Archive {
        compressing: bool,
        paths: Vec<String>,
        format: String,
        archive: PathBuf,
        dest: PathBuf,
        formats: Arc<Formats>,
        selection: Option<Vec<Selected>>,
        password: String,
        encrypt_names: bool,
    },
    Action {
        action: super::actions::Action,
        paths: Vec<String>,
        selection: Option<Vec<Selected>>,
    },
    // A LocalSend send, or an AirDrop one through owly when airdrop is set.
    Send {
        peer: String,
        paths: Vec<String>,
        request: usize,
        selection: Option<Vec<Selected>>,
        airdrop: bool,
    },
}

impl Work {
    pub fn paths(&self) -> Vec<String> {
        match self {
            Self::Rclone(send) => send.paths.clone(),
            Self::Transfer { paths, .. }
            | Self::Action { paths, .. }
            | Self::Send { paths, .. } => paths.clone(),
            Self::Archive {
                compressing,
                paths,
                archive,
                ..
            } => {
                if *compressing {
                    paths.clone()
                } else {
                    vec![archive.to_string_lossy().into()]
                }
            }
        }
    }

    // The folder a transfer or archive writes into, whose drive orders it behind other jobs.
    pub fn target(&self) -> Option<PathBuf> {
        match self {
            Self::Transfer { dest, .. } | Self::Archive { dest, .. } => Some(dest.clone()),
            Self::Action { .. } | Self::Send { .. } | Self::Rclone(_) => None,
        }
    }

    pub fn file(&self) -> String {
        let path = match self {
            Self::Archive {
                compressing,
                dest,
                archive,
                ..
            } => {
                if *compressing {
                    dest.clone()
                } else {
                    archive.clone()
                }
            }
            _ => self.paths().first().map(PathBuf::from).unwrap_or_default(),
        };
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    }

    pub fn retry(&mut self, failed: &[(PathBuf, ItemIdentity)]) -> Result<usize, String> {
        let original = self.paths();
        let mut paths = Vec::new();
        for (path, identity) in failed {
            let current = ItemIdentity::inspect(path).map_err(|e| e.msg)?;
            if !identity.same_item(&current) {
                return Err("A failed item changed; it was not retried.".into());
            }
            paths.push(path.to_string_lossy().to_string());
        }
        if paths.is_empty() {
            return Err("No unchanged failed items can be retried.".into());
        }
        let saved = || {
            failed
                .iter()
                .map(|(path, identity)| Selected::retry_item(path.clone(), identity.clone()))
                .collect()
        };
        match self {
            Self::Rclone(send) => {
                send.selection = Some(saved());
                send.paths = paths;
            }
            Self::Transfer {
                paths: held,
                selection,
                ..
            }
            | Self::Action {
                paths: held,
                selection,
                ..
            }
            | Self::Send {
                paths: held,
                selection,
                ..
            } => {
                *selection = Some(saved());
                *held = paths;
            }
            Self::Archive { selection, .. } => {
                if paths != original {
                    return Err("The archive must be retried as one operation.".into());
                }
                *selection = Some(saved());
            }
        }
        Ok(self.paths().len())
    }

    pub fn failed_line(&self, id: usize, total: usize, error: &str) -> String {
        match self {
            Self::Transfer { .. } => {
                super::opsreq::transferdone_line(id, 0, total, 0, false, &[], false, error, &[])
            }
            Self::Archive { .. } => super::archivereq::archivedone_line(id, false, true, error),
            _ => String::new(),
        }
    }

    pub fn cancelled_line(&self, id: usize, total: usize) -> String {
        match self {
            Self::Rclone(send) => format!(
                r#"{{"t":"rclonesent","id":{},"requestId":{},"ok":false,"cancelled":true,"check":"none","receipt":"","note":"","error":""}}"#,
                id, send.request
            ),
            Self::Transfer { .. } => {
                super::opsreq::transferdone_line(id, 0, 0, total, true, &[], false, "", &[])
            }
            Self::Archive { compressing, .. } => super::archivereq::archivedone_line(
                id,
                false,
                true,
                if *compressing {
                    "operation cancelled"
                } else {
                    "cancelled"
                },
            ),
            Self::Send { request, airdrop: true, .. } => airdropsend::sent_line(*request, "cancelled", ""),
            Self::Send { request, .. } => format!(
                r#"{{"t":"localsendsent","id":{},"ok":false,"reason":"Send cancelled."}}"#,
                request
            ),
            Self::Action { .. } => String::new(),
        }
    }

    pub fn start(&mut self, id: usize, cancel: Arc<AtomicBool>, tx: Sender<OpMsg>) {
        match self {
            Self::Rclone(send) => {
                let send = send.clone();
                std::thread::spawn(move || super::rclonerun::background(id, send, cancel, tx));
            }
            Self::Transfer {
                moving,
                paths,
                dest,
                selection,
                destination,
                policy,
            } => {
                let (moving, paths, dest) = (*moving, paths.clone(), dest.clone());
                let (selection, destination, policy) =
                    (selection.clone(), destination.clone(), policy.clone());
                std::thread::spawn(move || {
                    super::opsreq::run_transfer_checked(
                        id,
                        moving,
                        paths,
                        dest,
                        cancel,
                        tx,
                        selection,
                        destination,
                        policy,
                    )
                });
            }
            Self::Archive {
                compressing,
                paths,
                format,
                archive,
                dest,
                formats,
                selection,
                password,
                encrypt_names,
            } => {
                let (compressing, paths, format, archive, dest) = (
                    *compressing,
                    paths.clone(),
                    format.clone(),
                    archive.clone(),
                    dest.clone(),
                );
                let (formats, selection, password, encrypt_names) = (
                    Arc::clone(formats),
                    selection.clone(),
                    password.clone(),
                    *encrypt_names,
                );
                std::thread::spawn(move || {
                    let sources = if compressing {
                        paths.clone()
                    } else {
                        vec![archive.to_string_lossy().into_owned()]
                    };
                    let items = selection
                        .map(pin_selection)
                        .unwrap_or_else(|| capture(&sources));
                    let line = super::archivereq::archive_line(
                        id,
                        super::archivereq::ArchiveRequest {
                            compressing, paths, format, archive, dest, password, encrypt_names,
                        },
                        &formats,
                        Some(&items),
                        cancel,
                        Some(&tx),
                    );
                    let success = crate::json::field_bool(&line, "ok");
                    let _ = tx.send(OpMsg::JobDone {
                        id,
                        failed: if success {
                            Vec::new()
                        } else {
                            failures(&items)
                        },
                        error: crate::json::field_str(&line, "err").unwrap_or_default(),
                        line,
                    });
                });
            }
            Self::Action {
                action,
                paths,
                selection,
            } => {
                let (action, paths, selection) = (action.clone(), paths.clone(), selection.clone());
                std::thread::spawn(move || {
                    super::actionrun::background(id, &action, paths, selection, cancel, tx)
                });
            }
            Self::Send {
                peer,
                paths,
                request,
                selection,
                airdrop: true,
            } => {
                let send = airdropsend::Send {
                    peer: peer.clone(),
                    paths: paths.clone(),
                    request: *request,
                    items: selection.clone().unwrap_or_else(|| capture(paths)),
                };
                std::thread::spawn(move || airdropsend::background(id, send, cancel, tx));
            }
            Self::Send {
                peer,
                paths,
                request,
                selection,
                airdrop: false,
            } => {
                let (peer, paths, request, selection) =
                    (peer.clone(), paths.clone(), *request, selection.clone());
                std::thread::spawn(move || {
                    let items = selection.unwrap_or_else(|| capture(&paths));
                    let source_paths: Vec<_> = paths.iter().map(PathBuf::from).collect();
                    let result = super::menu_actions::validate_sources(Some(&items), &source_paths)
                        .and_then(|()| super::localsend::send_cancelled(&peer, &paths, &cancel));
                    let error = result.err().unwrap_or_default();
                    let line = format!(
                        r#"{{"t":"localsendsent","id":{},"ok":{},"reason":"{}"}}"#,
                        request,
                        error.is_empty(),
                        crate::json::escape(&error)
                    );
                    let _ = tx.send(OpMsg::JobDone {
                        id,
                        failed: if error.is_empty() {
                            Vec::new()
                        } else {
                            failures(&items)
                        },
                        error,
                        line,
                    });
                });
            }
        }
    }
}

fn pin_selection(items: Vec<Selected>) -> Vec<Selected> {
    items
        .into_iter()
        .filter_map(|item| {
            let identity = item.retry_identity().or_else(|| {
                item.current()
                    .ok()
                    .map(|metadata| ItemIdentity::record(&metadata))
            })?;
            Some(Selected::retry_item(item.path, identity))
        })
        .collect()
}

pub(crate) fn capture(paths: &[String]) -> Vec<Selected> {
    let body = super::mountinfo::read_body();
    paths
        .iter()
        .filter_map(|path| {
            ItemIdentity::inspect_in(std::path::Path::new(path), &body)
                .ok()
                .map(|identity| Selected::retry_item(PathBuf::from(path), identity))
        })
        .collect()
}

pub(crate) fn failures(items: &[Selected]) -> Vec<(PathBuf, ItemIdentity)> {
    items
        .iter()
        .filter_map(|item| {
            item.retry_identity()
                .map(|identity| (item.path.clone(), identity))
        })
        .collect()
}
