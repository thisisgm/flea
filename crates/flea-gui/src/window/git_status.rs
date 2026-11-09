//! The GUI consumes the backend's bounded Git snapshot and requests marks only for painted rows.
use crate::{backend::Event, state::Shell};
use flea::jsondoc::{self, Json};
use gpui::Context;
use std::{collections::HashMap, rc::Rc};

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
    ready: bool,
    pending: bool,
    paths: Vec<String>,
    pub paint: Rc<Paint>,
}
fn text<'a>(doc: &'a Json, key: &str) -> &'a str {
    doc.get(key).and_then(Json::as_str).unwrap_or("")
}
fn quote(text: &str) -> String {
    format!("\"{}\"", flea::json::escape(text))
}

impl Shell {
    pub(crate) fn sync_git(&mut self) {
        let enabled = self
            .launch
            .settings
            .data
            .get("showGitStatus")
            .and_then(Json::as_bool)
            == Some(true);
        for host in self.panes.iter_mut().flatten() {
            let on = enabled && !host.pane.state.search && !host.recent.on;
            let target = (host.path.clone(), host.pane.held.listing, on);
            if host.pane.busy() {
                continue;
            }
            let git = &mut host.git;
            if git.target.as_ref() != Some(&target) {
                let was_on = git.target.as_ref().is_some_and(|target| target.2);
                git.target = Some(target);
                git.serial += 1;
                git.status_id = git.serial;
                git.ready = false;
                git.pending = false;
                git.paths.clear();
                git.paint = Rc::default();
                host.list.borrow_mut().view.git = git.paint.clone();
                if let Some(backend) = host.backend.as_ref().filter(|_| on || was_on) {
                    let _ = backend.line(format!("{{\"c\":\"gitstatus\",\"id\":{},\"path\":{},\"enabled\":{on},\"refresh\":true}}", git.status_id, quote(&host.path)));
                }
            }
            if !on || !git.ready || git.pending {
                continue;
            }
            let paths: Vec<_> = host
                .list
                .borrow()
                .visible()
                .iter()
                .take(400)
                .filter_map(|shown| host.pane.held.row(shown.index))
                .map(|row| crate::navigation::history::join(&host.path, &row.name))
                .collect();
            if paths == git.paths {
                continue;
            }
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
        if text(&doc, "path") != host.path
            || git
                .target
                .as_ref()
                .is_none_or(|t| !t.2 || t.1 != host.pane.held.listing)
        {
            return true;
        }
        let ok = doc.get("ok").and_then(Json::as_bool) == Some(true);
        if kind == "gitstatus" && id == git.status_id {
            git.ready = ok && !text(&doc, "root").is_empty();
            git.paint = Rc::new(Paint {
                label: if git.ready {
                    text(&doc, "label").into()
                } else {
                    String::new()
                },
                ..Default::default()
            });
        } else if kind == "gitrows" && git.pending && id == git.rows_id {
            git.pending = false;
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
