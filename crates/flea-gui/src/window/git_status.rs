//! The GUI consumes the backend's bounded Git snapshot and requests marks only for painted rows.
use crate::{
    backend::{Backend, Event},
    state::Shell,
    window::startup::View,
};
use flea::jsondoc::{self, Json};
use gpui::{Context, Task};
use std::{collections::HashMap, rc::Rc, time::Duration};

/// How often a drawn folder re-reads Git while Flea sits idle. The backend keeps one
/// snapshot in flight, so a slow `git status` only delays the next tick, never stacks them.
pub(crate) const REFRESH_EVERY: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Default)]
pub(crate) struct Mark {
    pub letter: char,
    pub folder: bool,
}
#[derive(Default)]
pub(crate) struct Paint {
    #[cfg(test)]
    pub painted: std::cell::Cell<usize>,
    pub label: String,
    pub marks: HashMap<String, Mark>,
    pub runs: std::cell::RefCell<Option<(crate::list::Typeface, crate::path::paint::Shapes)>>,
}
#[derive(Default)]
pub(crate) struct Runtime {
    target: Option<(String, usize, bool)>,
    serial: usize,
    status_id: usize,
    rows_id: usize,
    /// The last status reply succeeded and has a repository root, so its rows may be needed.
    ready: bool,
    /// A `gitrows` request is out; its reply fills the marks.
    pending: bool,
    /// A `gitstatus` request is out. Kept separate from `ready` so a failed reply cannot stop
    /// later refreshes: a read being unpaid for and a read that failed are different states.
    status_pending: bool,
    /// Set by the wake task; consumed by the next frame that can send a refresh.
    due: bool,
    /// One idle refresh loop per drawn pane, alive only while Git status is on.
    wake: Option<Task<()>>,
    paths: Vec<String>,
    pub paint: Rc<Paint>,
}

impl Runtime {
    /// Ends this pane's idle refresh loop and forgets any refresh it still owed.
    pub(crate) fn stop(&mut self) {
        self.wake = None;
        self.due = false;
    }

    /// Whether a Git request is still waiting for a reply. Test-only view of the state machine.
    #[cfg(test)]
    pub(crate) fn in_flight(&self) -> bool {
        self.status_pending || self.pending
    }
}

fn text<'a>(doc: &'a Json, key: &str) -> &'a str {
    doc.get(key).and_then(Json::as_str).unwrap_or("")
}
fn quote(text: &str) -> String {
    format!("\"{}\"", flea::json::escape(text))
}

/// Starts a new snapshot read, dropping the cached rows so the reply owes a fresh `gitrows`.
/// Every read carries `refresh:true`, whether it follows a navigation or an idle tick.
fn ask_status(git: &mut Runtime, backend: Option<&Backend>, path: &str, on: bool) {
    git.serial += 1;
    git.status_id = git.serial;
    git.ready = false;
    git.pending = false;
    git.status_pending = false;
    git.due = false;
    git.paths.clear();
    if let Some(backend) = backend {
        let line = format!(
            "{{\"c\":\"gitstatus\",\"id\":{},\"path\":{},\"enabled\":{on},\"refresh\":true}}",
            git.status_id,
            quote(path)
        );
        // Only a request that actually reached the pipe is in flight. A failed write must not
        // leave the pane believing a reply is coming, or the next tick could never refresh it.
        git.status_pending = backend.line(line).is_ok();
    }
}

impl Shell {
    pub(crate) fn sync_git(&mut self, cx: &mut Context<Self>) {
        let enabled = self
            .launch
            .settings
            .data
            .get("showGitStatus")
            .and_then(Json::as_bool)
            == Some(true);
        let view = self.view;
        for (side, host) in self.panes.iter_mut().enumerate() {
            let Some(host) = host else {
                continue;
            };
            // Only side 0 is drawn outside Dual. A hidden pane must not hold a wake or run reads
            // for a folder the user cannot see.
            let shown = side == 0 || view == View::Dual;
            let on = enabled && shown && !host.pane.state.search && !host.recent.on;
            let git = &mut host.git;
            if !on {
                git.stop();
            } else if git.wake.is_none() {
                // One sleeping task per drawn pane. It only marks a refresh owed and asks for a
                // frame; all backend work stays on the frame that already owns the request path.
                git.wake = Some(cx.spawn(async move |this, cx| loop {
                    cx.background_executor().timer(REFRESH_EVERY).await;
                    let live = this.update(cx, |shell, cx| {
                        let Some(host) = shell.panes[side].as_mut() else {
                            return false;
                        };
                        if host.git.wake.is_none() {
                            return false;
                        }
                        // A read already out covers this tick. Owing a refresh here would start a
                        // second read the moment the first one's rows land, turning a slow mount
                        // into back-to-back snapshots instead of delaying the next tick.
                        if !host.git.status_pending && !host.git.pending {
                            host.git.due = true;
                            cx.notify();
                        }
                        true
                    });
                    if !live.unwrap_or(false) {
                        break;
                    }
                }));
            }
            if host.pane.busy() {
                continue;
            }
            let target = (host.path.clone(), host.pane.held.listing, on);
            if git.target.as_ref() != Some(&target) {
                let was_on = git.target.as_ref().is_some_and(|target| target.2);
                git.target = Some(target);
                git.paint = Rc::default();
                host.list.borrow_mut().view.git = git.paint.clone();
                let backend = host.backend.as_ref().filter(|_| on || was_on);
                ask_status(git, backend, &host.path, on);
            }
            // One read at a time. A request already out is advanced by its reply on a later
            // frame, not by another frame here; a failed reply must still clear `status_pending`.
            if git.status_pending || git.pending {
                continue;
            }
            // A finished snapshot owes fresh rows for the current viewport before any owed tick
            // may start another read. Fetching rows first keeps a read slower than REFRESH_EVERY
            // from updating its label while starving its own marks on every later tick.
            if on && git.ready {
                let paths: Vec<_> = host
                    .list
                    .borrow()
                    .visible()
                    .iter()
                    .take(400)
                    .filter_map(|shown| host.pane.held.row(shown.index))
                    .map(|row| crate::navigation::history::join(&host.path, &row.name))
                    .collect();
                if paths != git.paths {
                    if let Some(backend) = &host.backend {
                        git.serial += 1;
                        git.rows_id = git.serial;
                        let line = format!(
                            "{{\"c\":\"gitrows\",\"id\":{},\"path\":{},\"paths\":[{}]}}",
                            git.rows_id,
                            quote(&host.path),
                            paths.iter().map(|p| quote(p)).collect::<Vec<_>>().join(",")
                        );
                        if backend.line(line).is_ok() {
                            git.paths = paths;
                            git.pending = true;
                        }
                    }
                    continue;
                }
            }
            // The guard tracks the in-flight read, not `ready`: a failed reply left `ready` false,
            // but the next tick still owes a fresh read and must be free to start one.
            if git.due && on && host.backend.is_some() {
                ask_status(git, host.backend.as_ref(), &host.path, on);
            }
        }
    }

    pub(crate) fn git_event(&mut self, side: usize, event: &Event, cx: &mut Context<Self>) -> bool {
        let Event::Producer(line) = event else {
            return false;
        };
        if !matches!(
            flea::json::field_str(line, "t").as_deref(),
            Some("gitstatus" | "gitrows")
        ) {
            return false;
        }
        let Ok(doc) = jsondoc::parse(line) else {
            return false;
        };
        let kind = text(&doc, "t");
        if !matches!(kind, "gitstatus" | "gitrows") {
            return false;
        }
        let Some(host) = self.panes[side].as_mut() else {
            return true;
        };
        let git = &mut host.git;
        let id = doc.get("id").and_then(Json::as_f64).unwrap_or(0.) as usize;
        let status = kind == "gitstatus" && id == git.status_id;
        let rows = kind == "gitrows" && git.pending && id == git.rows_id;
        // Settle the request before the path/target guard. A reply carrying the current id means
        // that read is over even when its path no longer matches (a rename, a late reply after
        // navigation); leaving the flag set would block every later refresh for this pane.
        if status {
            git.status_pending = false;
        }
        if rows {
            git.pending = false;
        }
        if text(&doc, "path") != host.path
            || git
                .target
                .as_ref()
                .is_none_or(|t| !t.2 || t.1 != host.pane.held.listing)
        {
            return true;
        }
        let ok = doc.get("ok").and_then(Json::as_bool) == Some(true);
        if status {
            git.ready = ok && !text(&doc, "root").is_empty();
            // An idle refresh keeps the last marks on screen until its rows land. A failed
            // snapshot has nothing truthful left to draw, so it clears them.
            let marks = if git.ready {
                git.paint.marks.clone()
            } else {
                HashMap::new()
            };
            git.paint = Rc::new(Paint {
                label: if git.ready {
                    text(&doc, "label").into()
                } else {
                    String::new()
                },
                marks,
                ..Default::default()
            });
        } else if rows {
            let mut paint = Paint {
                label: git.paint.label.clone(),
                ..Default::default()
            };
            if ok {
                for row in doc
                    .get("marks")
                    .and_then(Json::as_array)
                    .unwrap_or(&[])
                    .iter()
                    .take(400)
                {
                    let path = text(row, "path");
                    if !git.paths.iter().any(|p| p == path) {
                        continue;
                    }
                    // Keyed by the row's own name, which is relative to the listed folder inside an open folder.
                    if let Some(name) = std::path::Path::new(path)
                        .strip_prefix(&host.path)
                        .ok()
                        .and_then(|n| n.to_str())
                    {
                        paint.marks.insert(
                            name.into(),
                            Mark {
                                letter: text(row, "status").chars().next().unwrap_or('\0'),
                                folder: row.get("folder").and_then(Json::as_bool) == Some(true),
                            },
                        );
                    }
                }
            }
            git.paint = Rc::new(paint);
        } else {
            return true;
        }
        host.list.borrow_mut().view.git = git.paint.clone();
        cx.notify();
        true
    }
}
