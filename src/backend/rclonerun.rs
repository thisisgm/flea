// Each selected item copies independently, then all successful copies are checked before completion.
use super::menu_actions::Selected;
use super::opsreq::OpMsg;
use super::rclonelog::Log;
use crate::json::escape;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct Send {
    pub remote: String,
    pub folder: String,
    pub paths: Vec<String>,
    pub request: usize,
    pub selection: Option<Vec<Selected>>,
}

struct Progress<'a> {
    id: usize,
    tx: &'a Sender<OpMsg>,
    done: usize,
    total: usize,
    bytes: u64,
    total_bytes: u64,
}

impl Progress<'_> {
    fn sample(&self, file: &str, item_bytes: u64, bytes: u64) {
        let _ = self.tx.send(OpMsg::JobEntries {
            id: self.id,
            done: self.done,
            total: self.total,
            file: file.into(),
            bytes,
            total_bytes: self.total_bytes.max(bytes),
            item_bytes,
        });
    }

    fn status(&self, phase: &str, check: &str, receipt: &str, rate: u64, left: Option<u64>) {
        let _ = self.tx.send(OpMsg::SendStatus {
            id: self.id,
            phase: phase.into(),
            check: check.into(),
            receipt: receipt.into(),
            rate,
            left,
        });
    }
}

pub(super) fn background(id: usize, send: Send, cancel: Arc<AtomicBool>, tx: Sender<OpMsg>) {
    let items = send
        .selection
        .clone()
        .unwrap_or_else(|| super::jobwork::capture(&send.paths));
    let mut progress = Progress {
        id,
        tx: &tx,
        done: 0,
        total: send.paths.len(),
        bytes: 0,
        total_bytes: 0,
    };
    let mut failed = Vec::new();
    let mut errors = Vec::new();
    let mut copied = Vec::new();
    let mut check = "none";
    let mut checked = false;
    let result = prepare(&send, &items, &cancel);
    match result {
        Err(error) => {
            failed.extend(super::jobwork::failures(&items));
            errors.push(error);
        }
        Ok(features) => {
            let destination = format!("{}:{}", send.remote, send.folder);
            progress.total_bytes = items
                .iter()
                .filter_map(|item| item.current().ok())
                .filter(|m| m.is_file())
                .map(|m| m.len())
                .sum();
            progress.status("sending", "none", "", 0, None);
            for path in &send.paths {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let source = flea::bytename::view(path);
                let source = &*source;
                let selected: Vec<_> = items.iter().filter(|i| i.path == source).cloned().collect();
                let name = source.file_name().unwrap().to_string_lossy();
                let mut log = Log::default();
                progress.sample(&name, 0, progress.bytes);
                let result =
                    super::menu_actions::validate_sources(Some(&selected), &[source.to_path_buf()])
                        .and_then(|()| selected[0].current())
                        .and_then(|meta| {
                            if meta.is_dir() {
                                features.folder(source, &send.remote, &cancel)?;
                            }
                            let remaining = progress
                                .total_bytes
                                .saturating_sub(progress.bytes)
                                .saturating_sub(if meta.is_file() { meta.len() } else { 0 });
                            let target = child_target(&destination, &name);
                            if meta.is_dir() {
                                features.create(&target, &cancel)?;
                            }
                            let mut args = arguments(
                                if meta.is_dir() { "copy" } else { "copyto" },
                                path,
                                &target,
                                false,
                            );
                            if send.selection.is_some() {
                                args.insert(1, "--ignore-times".into());
                            }
                            selected[0].current()?;
                            super::rcloneproc::run(&args, &cancel, None, false, |line| {
                                if log.line(line) {
                                    let file: &str = if log.file.is_empty() {
                                        &name
                                    } else {
                                        &log.file
                                    };
                                    progress.total_bytes = progress.total_bytes.max(
                                        progress
                                            .bytes
                                            .saturating_add(log.total)
                                            .saturating_add(remaining),
                                    );
                                    progress.status("sending", "none", "", log.rate, log.left);
                                    progress.sample(
                                        file,
                                        log.size,
                                        progress.bytes.saturating_add(log.bytes),
                                    );
                                }
                            })
                            .and_then(|out| verdict(out.success, &log, &name, &send.remote, false))
                        });
                progress.bytes = progress.bytes.saturating_add(log.bytes);
                progress.total_bytes = progress.total_bytes.max(progress.bytes);
                match result {
                    Ok(()) => {
                        progress.done += 1;
                        copied.push((path, selected));
                    }
                    Err(error) => {
                        failed.extend(super::jobwork::failures(&selected));
                        errors.push(error);
                    }
                }
                progress.sample(&name, 0, progress.bytes);
            }
            if !copied.is_empty() && !cancel.load(Ordering::Relaxed) {
                let hashes = features.hashes;
                check = if hashes { "checksum" } else { "size" };
                for (path, selected) in &copied {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let source = flea::bytename::view(path);
                    let source = &*source;
                    let name = source.file_name().unwrap().to_string_lossy();
                    let detail = if check == "size" {
                        "comparing sizes"
                    } else {
                        "comparing checksums"
                    };
                    progress.sample(detail, 0, progress.bytes);
                    progress.status("checking", check, "", 0, None);
                    let mut log = Log::default();
                    let result = super::menu_actions::validate_sources(
                        Some(selected),
                        &[source.to_path_buf()],
                    )
                    .and_then(|()| selected[0].current())
                    .and_then(|meta| {
                        if meta.is_dir() {
                            features.folder(source, &send.remote, &cancel)?;
                        }
                        let target = if meta.is_dir() {
                            child_target(&destination, &name)
                        } else {
                            destination.clone()
                        };
                        let args = arguments("check", path, &target, !hashes);
                        super::rcloneproc::run(&args, &cancel, None, false, |line| {
                            log.line(line);
                            if log.no_hash {
                                check = "size";
                                progress.sample("comparing sizes", 0, progress.bytes);
                                progress.status("checking", check, "", 0, None);
                            }
                        })
                        .and_then(|out| verdict(out.success, &log, &name, &send.remote, true))
                    });
                    if let Err(error) = result {
                        failed.extend(super::jobwork::failures(selected));
                        errors.push(error);
                    }
                }
                checked = errors.is_empty() && !cancel.load(Ordering::Relaxed);
            }
        }
    }
    let cancelled = cancel.load(Ordering::Relaxed);
    let error = if cancelled {
        String::new()
    } else {
        errors.join(" ")
    };
    if !checked {
        check = "none";
    }
    let receipt = if checked || cancelled && progress.done > 0 {
        let folder = if send.folder.is_empty() {
            "".into()
        } else {
            format!("{} on ", send.folder)
        };
        format!(
            "Sent {} {} to {}{}",
            progress.done,
            if progress.done == 1 { "item" } else { "items" },
            folder,
            send.remote
        )
    } else {
        String::new()
    };
    progress.status(
        if checked { "checked" } else { "finished" },
        check,
        &receipt,
        0,
        None,
    );
    let line = format!(
        r#"{{"t":"rclonesent","id":{},"requestId":{},"ok":{},"cancelled":{},"check":"{}","receipt":"{}","note":"{}","error":"{}"}}"#,
        id,
        send.request,
        checked,
        cancelled,
        check,
        escape(&receipt),
        if cancelled && progress.done > 0 {
            "not checked"
        } else {
            ""
        },
        escape(&error)
    );
    let _ = tx.send(OpMsg::JobDone {
        id,
        failed,
        error,
        line,
    });
}

fn prepare(
    send: &Send,
    items: &[Selected],
    cancel: &AtomicBool,
) -> Result<super::rclonefolders::Features, String> {
    let mut names = std::collections::HashSet::new();
    for path in &send.paths {
        let selected = items
            .iter()
            .find(|item| item.path == *flea::bytename::view(path))
            .ok_or_else(|| format!("Could not inspect {}.", flea::bytename::display(path)))?;
        let meta = selected.current()?;
        if !meta.is_dir() && !meta.is_file() {
            return Err(format!(
                "Cannot send {} with rclone.",
                flea::bytename::display(path)
            ));
        }
        if !names.insert(selected.path.file_name()) {
            return Err("Selected items have the same destination name.".into());
        }
    }
    if !super::rclone::remotes(cancel)?.contains(&send.remote) {
        return Err(format!(
            "rclone remote {} is no longer configured.",
            send.remote
        ));
    }
    super::rclonefolders::Features::read(&send.remote, cancel)
}

fn child_target(destination: &str, name: &str) -> String {
    format!(
        "{}{}{}",
        destination,
        if destination.ends_with('/')
            || destination
                .split_once(':')
                .is_some_and(|(_, folder)| folder.is_empty())
        {
            ""
        } else {
            "/"
        },
        name
    )
}

fn arguments(op: &str, source: &str, target: &str, sizes: bool) -> Vec<String> {
    let mut args = vec![
        op.into(),
        "--use-json-log".into(),
        "--stats".into(),
        "150ms".into(),
        "--stats-log-level".into(),
        "NOTICE".into(),
        "--log-level".into(),
        "INFO".into(),
        "--copy-links=false".into(),
        "--links=false".into(),
        "--local-links=false".into(),
        "--skip-links=false".into(),
        "--skip-specials=false".into(),
        "--one-file-system=false".into(),
    ];
    if op == "copy" {
        args.push("--create-empty-src-dirs".into());
    }
    if op == "check" {
        args.push("--one-way".into());
    }
    args.push(format!("--size-only={}", sizes && op == "check"));
    args.push("--dry-run=false".into());
    args.extend(["--".into(), source.into(), target.into()]);
    args
}

fn verdict(ok: bool, log: &Log, name: &str, remote: &str, checking: bool) -> Result<(), String> {
    if !log.skipped.is_empty() {
        return Err(format!(
            "rclone skipped {} while sending {} to {}.",
            log.skipped, name, remote
        ));
    }
    if ok {
        return Ok(());
    }
    let error = if checking {
        format!("{} did not match on {}.", name, remote)
    } else {
        format!("Could not send {} to {}.", name, remote)
    };
    Err(log.explain(error))
}

#[cfg(test)]
#[path = "rclonerun_tests.rs"]
mod tests;
