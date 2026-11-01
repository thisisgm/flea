//! A bounded plugin place route over the pane's existing command pipe.
use super::{Backend, Control, Event};
use flea::jsondoc::{self, Json};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(crate) struct Route {
    state: Arc<Mutex<State>>,
    sender: async_channel::Sender<(u64, String)>,
}

#[derive(Default)]
struct State {
    active: bool,
    epoch: u64,
}

/// Wrapped pane commands waiting for the place's owner, each with its route id.
pub(crate) type RouteRequests = async_channel::Receiver<(u64, String)>;

impl Backend {
    pub(crate) fn plugin_event(line: &str) -> Option<Event> {
        super::transport::parse(line).ok()
    }

    pub(crate) fn plugin_route(
        &self,
    ) -> Option<(Route, RouteRequests, async_channel::Sender<Event>)> {
        if self.plugin_route.get().is_some() {
            return None;
        }
        let (sender, receiver) = async_channel::bounded(64);
        let route = Route {
            state: Default::default(),
            sender,
        };
        self.plugin_route.set(route.clone()).ok()?;
        Some((route, receiver, self.plugin_output.clone()))
    }

    pub(crate) fn plugin_active(&self) -> bool {
        self.plugin_route
            .get()
            .is_some_and(|route| route.state.lock().unwrap().active)
    }

    pub(crate) fn plugin_epoch(&self) -> u64 {
        self.plugin_route
            .get()
            .map_or(0, |route| route.state.lock().unwrap().epoch)
    }
}

impl Route {
    pub(crate) fn current(&self, epoch: u64) -> bool {
        self.state.lock().unwrap().epoch == epoch
    }

    fn line(control: &Control) -> Option<String> {
        Some(match control {
            Control::Line(line) => line.clone(),
            Control::List {
                path,
                select,
                first,
            } => {
                let mut out = Vec::new();
                flea::protocol::Request::List {
                    path,
                    select: select.as_deref(),
                    first: *first,
                }
                .write_to(&mut out)
                .ok()?;
                String::from_utf8(out).ok()?
            }
            Control::Window { start, count } => {
                format!(r#"{{"c":"window","start":{start},"count":{count}}}"#)
            }
            Control::Rows { command, action } => {
                let rows = action
                    .rows
                    .iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    r#"{{"c":"{command}","listing":{},"rows":[{rows}]}}"#,
                    action.listing
                )
            }
            Control::Quit => return None,
        })
    }

    pub(super) fn intercept(&self, control: &Control) -> Result<bool, super::Failure> {
        let Some(line) = Self::line(control) else {
            return Ok(false);
        };
        let Ok(doc) = jsondoc::parse(&line) else {
            return Ok(false);
        };
        let command = doc.get("c").and_then(Json::as_str).unwrap_or("");
        let virtual_path = |key| {
            doc.get(key)
                .and_then(Json::as_str)
                .is_some_and(|s| s.starts_with("plugin://"))
        };
        let virtual_paths = doc
            .get("paths")
            .and_then(Json::as_array)
            .is_some_and(|paths| {
                paths
                    .iter()
                    .any(|p| p.as_str().is_some_and(|s| s.starts_with("plugin://")))
            });
        let mut state = self.state.lock().unwrap();
        if command == "list" {
            state.active = virtual_path("path");
            state.epoch += 1;
        } else if command == "listpaths" {
            state.active = false;
            state.epoch += 1;
        }
        if matches!(
            command,
            "clipGet" | "clipSet" | "clipClear" | "clipWatch" | "plugins"
        ) {
            return Ok(false);
        }
        if !state.active && !virtual_path("path") && !virtual_path("dest") && !virtual_paths {
            return Ok(false);
        }
        let epoch = state.epoch;
        if self.sender.try_send((epoch, line)).is_err() {
            return Err(super::failure(
                "pane.row_request",
                "Plugin request queue is full.",
            ));
        }
        Ok(true)
    }
}
