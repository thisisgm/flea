// One ordered worker owns the snapshot; navigation, refresh and visible-row requests cannot race it.
use flea::bytename::ExactText as _;
use super::events::Event;
use super::gitprobe::{self, Repository};
use super::gitstatus::Snapshot;
use crate::json::{field_bool, field_str, field_str_array, field_usize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;

const MAX_WINDOW: usize = 400;

pub struct Worker {
    queue: Sender<(String, u64)>,
    epoch: Arc<AtomicU64>,
    target: Option<(String, bool)>,
}

impl Worker {
    pub fn new(replies: crate::backend::events::Sender<Event>) -> Self {
        let (tx, rx) = mpsc::channel::<(String, u64)>();
        let epoch = Arc::new(AtomicU64::new(0));
        let current = Arc::clone(&epoch);
        std::thread::spawn(move || {
            let mut session = Session::default();
            let mut adopted = None;
            for (line, stamp) in rx {
                let cancelled = || current.load(Ordering::SeqCst) != stamp;
                if !cancelled() && adopted != Some(stamp) {
                    session = Session::default();
                    adopted = Some(stamp);
                }
                if replies
                    .send(Event::Git(session.answer_with(&line, &cancelled)))
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            queue: tx,
            epoch,
            target: None,
        }
    }

    pub fn request(&mut self, line: String) {
        if field_str(&line, "c").as_deref() == Some("gitstatus") {
            let target = (
                field_str(&line, "path").unwrap_or_default(),
                field_bool(&line, "enabled"),
            );
            if self.target.as_ref() != Some(&target) {
                self.epoch.fetch_add(1, Ordering::SeqCst);
                self.target = Some(target);
            }
        }
        let _ = self.queue.send((line, self.epoch.load(Ordering::SeqCst)));
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Default)]
pub(super) struct Session {
    path: String,
    enabled: bool,
    repo: Option<Repository>,
    snapshot: Option<Snapshot>,
    error: Option<String>,
}

impl Session {
    #[cfg(test)]
    pub(super) fn answer(&mut self, line: &str) -> String {
        self.answer_with(line, &|| false)
    }

    fn answer_with(&mut self, line: &str, cancelled: &dyn Fn() -> bool) -> String {
        let command = field_str(line, "c").unwrap_or_default();
        let path = field_str(line, "path").unwrap_or_default();
        let id = field_usize(line, "id").unwrap_or(0);
        let kind = match command.as_str() {
            "gitrows" => "gitrows",
            "gitchanges" => "gitchanges",
            _ => "gitstatus",
        };
        let result = match command.as_str() {
            _ if cancelled() => Err("Git status was cancelled.".into()),
            "gitstatus" => self.status(line, &path, cancelled),
            _ if self.enabled && self.path == path && self.snapshot.is_some() => {
                if command == "gitrows" {
                    self.rows(line)
                } else {
                    self.changes(line)
                }
            }
            _ => Err("Git status is unavailable for this directory.".into()),
        };
        match result {
            Ok(fields) => format!(
                "{{\"t\":{},\"id\":{id},\"path\":{},\"ok\":true,{fields}}}",
                quote(kind),
                quote(&path)
            ),
            Err(error) => format!(
                "{{\"t\":{},\"id\":{id},\"path\":{},\"ok\":false,\"error\":{}}}",
                quote(kind),
                quote(&path),
                quote(&error)
            ),
        }
    }

    fn status(
        &mut self,
        line: &str,
        path: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<String, String> {
        let enabled = field_bool(line, "enabled");
        if !enabled {
            *self = Self::default();
            return Ok(
                "\"enabled\":false,\"root\":\"\",\"branch\":\"\",\"label\":\"\",\"changed\":0"
                    .into(),
            );
        }
        if !self.enabled || self.path != path || self.error.is_some() || field_bool(line, "refresh")
        {
            self.enabled = true;
            self.path = path.to_string();
            self.snapshot = None;
            self.repo = None;
            self.error = None;
            let loaded = self.reload(path, cancelled);
            if let Err(error) = loaded {
                self.error = Some(error);
            }
        }
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if let Some(snapshot) = &self.snapshot {
            let root = self.repo.as_ref().unwrap().root.exact_text();
            Ok(format!(
                "\"enabled\":true,\"root\":{},\"branch\":{},\"label\":{},\"changed\":{}",
                quote(&root),
                quote(&snapshot.branch),
                quote(&snapshot.label()),
                snapshot.changes.len()
            ))
        } else if self.repo.is_some() {
            Err("Git status is unavailable for this directory.".into())
        } else {
            Ok(
                "\"enabled\":true,\"root\":\"\",\"branch\":\"\",\"label\":\"\",\"changed\":0"
                    .into(),
            )
        }
    }

    fn reload(&mut self, path: &str, cancelled: &dyn Fn() -> bool) -> Result<(), String> {
        let target = flea::bytename::view(path).to_path_buf();
        let directory = target.clone();
        self.repo = super::iomount::call(
            &target,
            &super::iomount::mount_body(),
            "gitstatus",
            move || gitprobe::discover(&directory),
        )
        .map_err(|error| error.msg)??;
        if let Some(repo) = &self.repo {
            self.snapshot = Some(gitprobe::status_with(repo, cancelled)?);
        }
        Ok(())
    }

    fn rows(&self, line: &str) -> Result<String, String> {
        let paths = field_str_array(line, "paths");
        if paths.len() > MAX_WINDOW {
            return Err("Git status accepts at most 400 visible rows.".into());
        }
        let repo = self.repo.as_ref().unwrap();
        let prefix = repo
            .directory
            .strip_prefix(&repo.root)
            .map_err(|_| "Git status directory left the repository.")?;
        let mut marks = Vec::with_capacity(paths.len());
        for path in paths {
            // A row inside a folder opened in place is a descendant; nothing may climb out of the directory.
            let file = flea::bytename::view(&path);
            let file = &*file;
            let name = file
                .strip_prefix(&self.path)
                .ok()
                .filter(|name| {
                    file.is_absolute()
                        && !name.as_os_str().is_empty()
                        && name
                            .components()
                            .all(|part| matches!(part, std::path::Component::Normal(_)))
                })
                .ok_or("Git status rows must belong to the current directory.")?;
            let (status, folder) = self.snapshot.as_ref().unwrap().mark(&prefix.join(name));
            marks.push(format!(
                "{{\"path\":{},\"status\":{},\"folder\":{folder}}}",
                quote(&path),
                quote(&status.map(|letter| letter.to_string()).unwrap_or_default())
            ));
        }
        Ok(format!("\"marks\":[{}]", marks.join(",")))
    }

    fn changes(&self, line: &str) -> Result<String, String> {
        let snapshot = self.snapshot.as_ref().unwrap();
        let start = field_usize(line, "start")
            .unwrap_or(0)
            .min(snapshot.changes.len());
        let count = field_usize(line, "count")
            .unwrap_or(MAX_WINDOW)
            .min(MAX_WINDOW);
        let entries: Vec<_> = snapshot
            .changes
            .iter()
            .skip(start)
            .take(count)
            .map(|change| {
                let original = change
                    .original
                    .as_ref()
                    .map(|path| path.exact_text().into_owned())
                    .unwrap_or_default();
                format!(
                    "{{\"path\":{},\"status\":{},\"original\":{},\"deleted\":{}}}",
                    quote(&change.path.exact_text()),
                    quote(&change.status.to_string()),
                    quote(&original),
                    change.status == 'D'
                )
            })
            .collect();
        Ok(format!(
            "\"root\":{},\"label\":{},\"total\":{},\"start\":{start},\"entries\":[{}]",
            quote(&self.repo.as_ref().unwrap().root.exact_text()),
            quote(&snapshot.label()),
            snapshot.changes.len(),
            entries.join(",")
        ))
    }
}

#[cfg(test)]
#[path = "gitreq_tests.rs"]
mod tests;

pub(super) fn quote(value: &str) -> String {
    format!("\"{}\"", crate::json::escape(value))
}

#[cfg(test)]
#[path = "git_review_tests.rs"]
mod review_tests;

#[cfg(test)]
#[path = "git_multi_tests.rs"]
mod multi_tests;

#[cfg(test)]
#[path = "gitreq_recovery_tests.rs"]
mod recovery_tests;
