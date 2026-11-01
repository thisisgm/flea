//! Real GPUI surfaces with the repository example running through Runtime and Host.
use super::place_model::*;
use crate::{
    backend::{Backend, Event},
    state::Shell,
};
use flea::{
    jsondoc::{self, Json},
    plugin_runtime::{Config, Runtime},
    ui_logic_st2::{
        manifest,
        wire::{self, Plugin, Source},
    },
};
use gpui::{AnyView, Entity, TestAppContext, VisualTestContext};
use std::{path::PathBuf, time::Duration};

pub(super) struct Fixture {
    pub plugin: Plugin,
    pub runtime: Runtime,
    pub calls: Vec<Json>,
    incoming: async_channel::Sender<Event>,
    requests: Box<dyn FnMut() -> Option<String>>,
}
impl Fixture {
    pub fn open<'a>(
        kind: &str,
        cx: &'a mut TestAppContext,
    ) -> (Self, Entity<Shell>, &'a mut VisualTestContext) {
        cx.executor().allow_parking();
        let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/plugins")
            .join(kind);
        let manifest = manifest::parse(
            &std::fs::read_to_string(folder.join("manifest.json")).unwrap(),
            false,
        )
        .unwrap();
        let runtime = Runtime::new(Config {
            folder: folder.clone(),
            manifest: manifest.clone(),
            hello: object(&[
                ("op", string("hello")),
                ("protocolVersion", number(1)),
                ("language", string("en")),
                ("settings", Json::Obj(Vec::new())),
            ]),
            ready_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(2),
            idle_timeout: Duration::from_secs(30),
        });
        let plugin = Plugin {
            manifest,
            source: Source::Folder(folder.to_string_lossy().into()),
            enabled: true,
            failure: None,
            mark_path: String::new(),
            settings: Vec::new(),
            shelf: None,
        };
        let (plugin_backend, requests) = Backend::test_lines();
        let incoming = plugin_backend.test_output();
        let (backend, receiver) = Backend::test_channel();
        let pane_replies = backend.replies.clone();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let mut shell = crate::state::tests::shell_with(backend, cx);
            cx.on_release(move |_, _| drop(receiver)).detach();
            window.focus(&shell.focus, cx);
            shell.panes[0].as_mut().unwrap().reader =
                Some(cx.spawn_in(window, async move |this, cx| {
                    while let Ok(event) = pane_replies.recv().await {
                        let _ = cx.update(|window, cx| {
                            this.update(cx, |shell, cx| shell.receive(0, event, window, cx))
                        });
                    }
                }));
            let rail = crate::window::rail::mount(
                shell.locale.catalog.clone(),
                &shell.launch.settings.data,
                0,
                cx,
            );
            shell.slots.rail = Some(AnyView::from(rail.host.clone()));
            shell.rail = Some(rail);
            shell.follow_plugins(window, cx);
            shell
                .plugins
                .host
                .as_ref()
                .unwrap()
                .update(cx, |host, cx| host.test_attach(plugin_backend, cx));
            shell
        });
        let mut fixture = Self {
            plugin,
            runtime,
            calls: Vec::new(),
            incoming,
            requests: Box::new(requests),
        };
        fixture.pump(cx);
        frame(cx);
        (fixture, shell, cx)
    }
    pub fn publish(&self) {
        let line = wire::listing_line(&wire::Listing {
            id: 0,
            plugins: vec![self.plugin.clone()],
        });
        self.incoming.try_send(Event::Plugins(line)).unwrap();
    }
    pub fn pump(&mut self, cx: &mut VisualTestContext) {
        for _ in 0..16 {
            cx.run_until_parked();
            let mut work = false;
            while let Some(line) = (self.requests)() {
                work = true;
                let doc = jsondoc::parse(&line).unwrap();
                match text(&doc, "op") {
                    "list" => self.publish(),
                    "runtime" => {
                        let request = doc.get("request").unwrap();
                        self.calls.push(request.clone());
                        let result = if text(request, "op") == "ready" {
                            self.runtime
                                .ensure_ready()
                                .map(|_| object(&[("op", string("ready"))]))
                        } else {
                            self.runtime.request(request.clone()).map(|r| r.message)
                        };
                        let ok = result.is_ok();
                        let message = result.unwrap_or_else(|error| {
                            object(&[("op", string("error")), ("message", string(error))])
                        });
                        let reply = object(&[
                            ("t", string("pluginruntime")),
                            ("id", number(num(&doc, "id"))),
                            ("plugin", string(&self.plugin.manifest.id)),
                            ("ok", Json::Bool(ok)),
                            ("message", message),
                        ]);
                        self.incoming
                            .try_send(Event::Plugins(wire::compact(&reply)))
                            .unwrap();
                    }
                    _ => panic!("unexpected fixture request: {line}"),
                }
            }
            if !work {
                break;
            }
        }
        cx.run_until_parked();
    }
    pub fn count(&self, op: &str) -> usize {
        self.calls.iter().filter(|r| text(r, "op") == op).count()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.runtime.shutdown();
    }
}
pub(super) fn frame(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}
pub(super) fn navigate(
    shell: &Entity<Shell>,
    step: crate::window::navigate::Step,
    cx: &mut VisualTestContext,
) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.step(0, step, window, cx).unwrap();
        })
    });
}
