//! Plugin contributions use the built-in rail's sections, rows and polling gate.
use super::Host;
use crate::state::Shell;
use flea::jsondoc::Json;
use flea::ui_logic_pl4::{
    order::{Heading, RailSection},
    plugin,
    rail::RailEntry,
};
use gpui::{Context, Task};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct Rail {
    due: HashMap<String, Instant>,
    pub timer: Option<Task<()>>,
    armed: Option<Instant>,
}

pub(crate) fn sections(host: &Host) -> Vec<RailSection> {
    let mut sections = Vec::new();
    for p in host.plugins.iter().filter(|p| host.healthy(p)) {
        if let Some(block) = p.manifest.block("rail-section") {
            let title = block
                .get("title")
                .and_then(Json::as_str)
                .unwrap_or(&p.manifest.name);
            let rows = host
                .answers
                .get(&(p.manifest.id.clone(), "section".into()))
                .and_then(|answer| {
                    plugin::parse_section(&p.manifest.id, &flea::jsondoc::render(answer))
                });
            if let Some(rows) = rows.filter(|rows| !rows.is_empty()) {
                sections.push(RailSection {
                    heading: Heading::Plugin(title.into()),
                    rows,
                });
            }
        }
        if p.manifest.id != "flea.dropbox" {
            if let Some(row) = super::place::rail_row(p) {
                sections.push(RailSection {
                    heading: Heading::None,
                    rows: vec![row],
                });
            }
        }
    }
    sections
}

pub(crate) fn dropbox_enabled(host: &Host) -> bool {
    host.plugins
        .iter()
        .find(|p| p.manifest.id == "flea.dropbox")
        .is_some_and(|p| host.healthy(p))
}

impl Shell {
    pub(crate) fn plugin_rail_poll(&mut self, cx: &mut Context<Self>) {
        if !self
            .rail
            .as_ref()
            .is_some_and(|r| r.places.read(cx).polling)
        {
            self.plugins.rail.timer = None;
            self.plugins.rail.armed = None;
            return;
        }
        let Some(host) = self.plugins.host.clone() else {
            return;
        };
        host.update(cx, |host, cx| {
            host.prepare("rail-section", cx);
            host.prepare("place", cx);
        });
        let now = cx.background_executor().now();
        let wanted: Vec<_> = host
            .read(cx)
            .plugins
            .iter()
            .filter(|p| host.read(cx).healthy(p))
            .filter_map(|p| {
                p.manifest.block("rail-section").map(|block| {
                    (
                        p.manifest.id.clone(),
                        plugin::clamp_refresh(block.get("refreshSec").and_then(Json::as_f64)),
                    )
                })
            })
            .collect();
        self.plugins
            .rail
            .due
            .retain(|id, _| wanted.iter().any(|(p, _)| p == id));
        for (id, seconds) in wanted {
            let due = self.plugins.rail.due.entry(id.clone()).or_insert(now);
            if *due <= now {
                if host
                    .update(cx, |host, cx| {
                        host.request(
                            &id,
                            Json::Obj(vec![("op".into(), Json::Str("section".into()))]),
                            cx,
                        )
                    })
                    .is_some()
                {
                    *due = now + Duration::from_secs(seconds as u64);
                } else {
                    *due = now + super::limits::RETRY;
                }
            }
        }
        let due = self.plugins.rail.due.values().copied().min();
        if self.plugins.rail.armed == due {
            return;
        }
        self.plugins.rail.armed = due;
        self.plugins.rail.timer = None;
        if let Some(due) = due {
            self.plugins.rail.timer = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(due.saturating_duration_since(now))
                    .await;
                let _ = this.update(cx, |shell, cx| {
                    shell.plugins.rail.armed = None;
                    shell.plugin_rail_poll(cx)
                });
            }));
        }
    }

    pub(crate) fn plugin_rail_activate(
        &mut self,
        side: usize,
        row: &RailEntry,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let current = self.plugins.host.as_ref().and_then(|host| {
            sections(host.read(cx))
                .into_iter()
                .flat_map(|s| s.rows)
                .find(|entry| entry.identity() == row.identity())
        });
        let Some(row) = current else { return };
        let path = if !row.path.is_empty() {
            row.path.clone()
        } else {
            row.uri.clone()
        };
        if path.starts_with("plugin://") || path.starts_with('/') {
            self.plugin_place_routes(window, cx);
            let _ = self.step(
                side,
                crate::window::navigate::Step::OpenPlace(path),
                window,
                cx,
            );
        } else {
            let mut remote = row;
            remote.path = path;
            self.favourite_remote_activate(side, &remote, window, cx);
        }
    }
}
