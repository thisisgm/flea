//! Shared-host execution for local plugin places and plugin-owned virtual listings.
use super::{place_model::*, place_ops::Action, Host};
use crate::{
    backend::{Backend, Event},
    state::Shell,
};
use flea::{
    jsondoc::{self, Json},
    ui_logic_plugin_settings::{
        manifest::Manifest,
        wire::{compact, Plugin},
    },
};
use gpui::{Context, Window};

pub(super) fn rail_row(plugin: &Plugin) -> Option<flea::ui_logic_rail_places::rail::RailEntry> {
    let block = plugin.manifest.block("place")?;
    let opens = block
        .get("path")
        .or_else(|| block.get("opens"))
        .and_then(Json::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("plugin://{}/", plugin.manifest.id));
    let message = object(&[
        ("op", string("section")),
        (
            "rows",
            Json::Arr(vec![object(&[
                ("id", string("place")),
                ("label", string(text(block, "label"))),
                ("glyph", string(text(block, "glyph"))),
                ("opens", string(opens)),
            ])]),
        ),
    ]);
    flea::ui_logic_rail_places::plugin::parse_section(&plugin.manifest.id, &compact(&message))?
        .into_iter()
        .next()
}

fn enabled(host: &Host, id: &str) -> Option<Manifest> {
    host.plugins
        .iter()
        .find(|p| p.manifest.id == id && host.healthy(p) && p.manifest.block("place").is_some())
        .map(|p| p.manifest.clone())
}

impl Shell {
    // Every step routes a plugin place's pane first; Up at a place's root has no parent to open.
    pub(crate) fn plugin_step(
        &mut self,
        side: usize,
        step: &crate::window::navigate::Step,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Result<bool, crate::navigation::Refusal>> {
        self.follow_plugins(window, cx);
        self.plugin_place_routes(window, cx);
        let root = self
            .panes
            .get(side)
            .and_then(Option::as_ref)
            .is_some_and(|p| {
                p.pane
                    .path()
                    .strip_prefix("plugin://")
                    .is_some_and(|p| !p.trim_end_matches('/').contains('/'))
            });
        (*step == crate::window::navigate::Step::Up && root).then_some(Ok(false))
    }

    pub(crate) fn plugin_places_changed(&mut self, cx: &mut Context<Self>) {
        let Some(service) = self.plugins.host.as_ref() else {
            return;
        };
        let service = service.read(cx);
        for pane in self.panes.iter_mut().flatten() {
            let Some((id, _)) = path(pane.pane.place()) else {
                continue;
            };
            if enabled(service, id).is_none() && (pane.pane.held.total > 0 || pane.pane.busy()) {
                pane.pane
                    .fail("backend", "", "Plugin is unavailable.", 0, pane.pane.now());
                pane.changed(&self.locale);
            }
        }
    }
    pub(crate) fn plugin_place_routes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.plugins.host.clone() else {
            return;
        };
        for side in 0..2 {
            let Some((route, requests, output)) = self.panes[side]
                .as_ref()
                .and_then(|p| p.backend.as_ref())
                .and_then(Backend::plugin_route)
            else {
                continue;
            };
            let host = host.clone();
            self.plugins.routes[side] = Some(cx.spawn_in(window, async move |_, cx| {
                let mut model = Model::default();
                while let Ok((epoch, line)) = requests.recv().await {
                    let Ok(doc) = jsondoc::parse(&line) else {
                        continue;
                    };
                    let scoped = matches!(
                        text(&doc, "c"),
                        "list" | "window" | "sort" | "locate" | "paths"
                    );
                    if scoped && !route.current(epoch) {
                        continue;
                    }
                    let action = match model.command(&doc) {
                        Ok(action) => action,
                        Err(error) => Action::Replies(vec![refusal(&doc, &model.path, &error)]),
                    };
                    let replies = match action {
                        Action::Replies(replies) => replies,
                        Action::Runtime {
                            plugin,
                            request,
                            completion,
                        } => {
                            let requested = cx
                                .update(|_, cx| {
                                    host.update(cx, |host, cx| {
                                        enabled(host, &plugin)?;
                                        host.request_wait(&plugin, request, cx)
                                    })
                                })
                                .ok()
                                .flatten();
                            let result = match requested {
                                Some(receiver) => receiver.recv().await.ok(),
                                None => None,
                            };
                            if scoped && !route.current(epoch) {
                                continue;
                            }
                            let healthy = cx
                                .update(|_, cx| enabled(host.read(cx), &plugin).is_some())
                                .unwrap_or(false);
                            match result.filter(|r| r.ok && healthy) {
                                Some(response) => {
                                    model.complete(&doc, &response.message, completion)
                                }
                                None => vec![refusal(
                                    &doc,
                                    &model.path,
                                    "Plugin is unavailable. Enable it in Settings, Plugins.",
                                )],
                            }
                        }
                    };
                    route.answered(&line);
                    if scoped && !route.current(epoch) {
                        continue;
                    }
                    for reply in replies {
                        let envelope = object(&[
                            ("t", string("pluginplace")),
                            ("epoch", number(epoch)),
                            ("scoped", Json::Bool(scoped)),
                            ("line", string(compact(&reply))),
                        ]);
                        if output
                            .send(Event::Producer(compact(&envelope)))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }));
        }
    }

    pub(crate) fn plugin_place_event(&self, side: usize, event: Event) -> Option<Event> {
        let backend = self.panes.get(side)?.as_ref()?.backend.as_ref();
        if let Event::Producer(line) = &event {
            if let Ok(doc) = jsondoc::parse(line) {
                if text(&doc, "t") == "pluginplace" {
                    if doc.get("scoped").and_then(Json::as_bool) == Some(true)
                        && backend.is_none_or(|b| b.plugin_epoch() != num(&doc, "epoch"))
                    {
                        return None;
                    }
                    return Backend::plugin_event(text(&doc, "line"));
                }
            }
        }
        if backend.is_some_and(Backend::plugin_active)
            && matches!(
                event,
                Event::Reply(_)
                    | Event::Session { .. }
                    | Event::Filesystem(_)
                    | Event::FilesystemDetails { .. }
                    | Event::FilesystemSystem { .. }
                    | Event::Changed { .. }
                    | Event::ListDetails(_)
                    | Event::Located(_)
            )
        {
            return None;
        }
        Some(event)
    }
}

pub(super) fn refusal(doc: &Json, path: &str, error: &str) -> Json {
    match text(doc, "c") {
        "menuaction" => object(&[
            ("t", string("menuaction")),
            ("op", string(text(doc, "op"))),
            ("id", number(num(doc, "id"))),
            ("ok", Json::Bool(false)),
            ("error", string(error)),
        ]),
        _ => object(&[
            ("t", string("error")),
            (
                "where",
                string(if text(doc, "c") == "list" {
                    "scan"
                } else {
                    text(doc, "c")
                }),
            ),
            (
                "path",
                string(if text(doc, "path").is_empty() {
                    path
                } else {
                    text(doc, "path")
                }),
            ),
            ("msg", string(error)),
        ]),
    }
}
