use crate::backend::{Backend, Event};
use flea::jsondoc::{self, Json};
use flea::ui_logic_plugin_settings::wire::{self, Plugin, Reply, Source};
use gpui::{App, AppContext, Context, Entity, EventEmitter, Global, Subscription, Task};
use std::collections::{HashMap, HashSet};

struct Service(Entity<Host>);
impl Global for Service {}

#[derive(Clone)]
pub(crate) struct WireLine(pub String);

pub(crate) struct Closed;

#[derive(Clone, Debug)]
pub(crate) struct Response {
    pub id: u64,
    pub plugin: String,
    pub request: Json,
    pub message: Json,
    pub ok: bool,
    pub error: String,
}

pub(crate) struct Host {
    pub plugins: Vec<Plugin>,
    pub answers: HashMap<(String, String), Json>,
    ready: HashSet<String>,
    pending: HashMap<u64, (String, Json)>,
    waiters: HashMap<u64, async_channel::Sender<Response>>,
    next: u64,
    backend: Option<Backend>,
    reader: Option<Task<()>>,
    quit: Option<Subscription>,
}
impl EventEmitter<Response> for Host {}
impl EventEmitter<WireLine> for Host {}
impl EventEmitter<Closed> for Host {}

pub(crate) fn service(cx: &mut App) -> Entity<Host> {
    if let Some(service) = cx.try_global::<Service>() {
        return service.0.clone();
    }
    let host = cx.new(Host::new);
    cx.set_global(Service(host.clone()));
    host
}

impl Host {
    fn new(cx: &mut Context<Self>) -> Self {
        let mut host = Self {
            plugins: Vec::new(),
            answers: HashMap::new(),
            ready: HashSet::new(),
            pending: HashMap::new(),
            waiters: HashMap::new(),
            next: 1,
            backend: None,
            reader: None,
            quit: None,
        };
        host.quit = Some(cx.on_app_quit(|host, _| {
            if let Some(backend) = host.backend.take() {
                backend.close_stdin();
            }
            std::future::ready(())
        }));
        #[cfg(not(test))]
        {
            let mut command = std::process::Command::new(crate::window::startup::binary());
            command.arg("--plugin-host");
            if let Ok(backend) =
                Backend::start_with(command, wire::request_line(1, "list", "", Vec::new()))
            {
                host.attach(backend, cx);
            }
        }
        host
    }

    fn attach(&mut self, backend: Backend, cx: &mut Context<Self>) {
        let replies = backend.replies.clone();
        self.backend = Some(backend);
        self.reader = Some(cx.spawn(async move |this, cx| {
            while let Ok(event) = replies.recv().await {
                let _ = this.update(cx, |host, cx| match event {
                    Event::Plugins(line) => host.receive(&line, cx),
                    Event::Failed(_) | Event::Drained => host.failed(cx),
                    _ => {}
                });
            }
        }));
    }

    #[cfg(test)]
    pub(crate) fn test_attach(&mut self, backend: Backend, cx: &mut Context<Self>) {
        self.attach(backend, cx);
        self.refresh();
    }

    pub fn healthy(&self, plugin: &Plugin) -> bool {
        plugin.enabled
            && plugin.failure.is_none()
            && (plugin.source == Source::Builtin || self.ready.contains(&plugin.manifest.id))
    }

    pub fn refresh(&self) {
        self.line(wire::request_line(0, "list", "", Vec::new()));
    }

    pub fn line(&self, line: String) -> bool {
        if line.len() > flea::plugin_runtime::protocol::MAX_MESSAGE {
            return false;
        }
        self.backend
            .as_ref()
            .is_some_and(|b| b.plugins(line).is_ok())
    }

    pub fn prepare(&mut self, kind: &str, cx: &mut Context<Self>) {
        let wanted: Vec<_> = self
            .plugins
            .iter()
            .filter(|p| {
                p.enabled
                    && p.failure.is_none()
                    && p.source != Source::Builtin
                    && p.manifest.block(kind).is_some()
                    && !self.ready.contains(&p.manifest.id)
                    && !self.pending.values().any(|(id, _)| id == &p.manifest.id)
            })
            .map(|p| p.manifest.id.clone())
            .collect();
        for plugin in wanted {
            self.request(
                &plugin,
                Json::Obj(vec![("op".into(), Json::Str("ready".into()))]),
                cx,
            );
        }
    }

    pub fn request(&mut self, plugin: &str, request: Json, _cx: &mut Context<Self>) -> Option<u64> {
        if self.pending.len() >= super::limits::PENDING {
            return None;
        }
        self.next = self.next.checked_add(1)?;
        let id = self.next;
        if !self.line(wire::request_line(
            id,
            "runtime",
            plugin,
            vec![("request", request.clone())],
        )) {
            return None;
        }
        self.pending.insert(id, (plugin.into(), request));
        Some(id)
    }

    pub fn request_wait(
        &mut self,
        plugin: &str,
        request: Json,
        cx: &mut Context<Self>,
    ) -> Option<async_channel::Receiver<Response>> {
        let id = self.request(plugin, request, cx)?;
        let (sender, receiver) = async_channel::bounded(1);
        self.waiters.insert(id, sender);
        Some(receiver)
    }

    fn receive(&mut self, line: &str, cx: &mut Context<Self>) {
        cx.emit(WireLine(line.to_owned()));
        if let Ok(Reply::Listing(list)) = wire::parse(line) {
            self.plugins = list.plugins;
            let healthy: HashSet<_> = self
                .plugins
                .iter()
                .filter(|p| p.enabled && p.failure.is_none())
                .map(|p| p.manifest.id.clone())
                .collect();
            self.ready.retain(|p| healthy.contains(p));
            self.answers
                .retain(|(plugin, _), _| healthy.contains(plugin));
            cx.notify();
            return;
        }
        let Ok(doc) = jsondoc::parse(line) else {
            return;
        };
        if doc.get("t").and_then(Json::as_str) == Some("pluginop") {
            self.refresh();
            cx.notify();
            return;
        }
        if doc.get("t").and_then(Json::as_str) == Some("pluginprogress") {
            let Some(id) = doc.get("id").and_then(Json::as_f64).map(|n| n as u64) else {
                return;
            };
            if let Some((plugin, request)) = self.pending.get(&id) {
                cx.emit(Response {
                    id,
                    plugin: plugin.clone(),
                    request: request.clone(),
                    message: doc.get("message").cloned().unwrap_or(Json::Null),
                    ok: true,
                    error: String::new(),
                });
            }
            return;
        }
        if doc.get("t").and_then(Json::as_str) != Some("pluginruntime") {
            return;
        }
        let Some(id) = doc.get("id").and_then(Json::as_f64).map(|n| n as u64) else {
            return;
        };
        let Some((plugin, request)) = self.pending.remove(&id) else {
            return;
        };
        let ok = doc.get("ok").and_then(Json::as_bool) == Some(true);
        let message = doc.get("message").cloned().unwrap_or(Json::Null);
        let error = doc
            .get("error")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_owned();
        let op = request.get("op").and_then(Json::as_str).unwrap_or("");
        if ok {
            self.ready.insert(plugin.clone());
            if matches!(op, "section" | "targets") {
                self.answers
                    .insert((plugin.clone(), op.into()), message.clone());
            }
        } else if !matches!(
            doc.get("code").and_then(Json::as_str),
            Some("busy" | "request" | "operation")
        ) {
            self.ready.remove(&plugin);
            self.answers.retain(|(id, _), _| id != &plugin);
            self.refresh();
        }
        let response = Response {
            id,
            plugin,
            request,
            message,
            ok,
            error,
        };
        if let Some(waiter) = self.waiters.remove(&id) {
            let _ = waiter.try_send(response.clone());
        }
        cx.emit(response);
        cx.notify();
    }

    fn failed(&mut self, cx: &mut Context<Self>) {
        self.ready.clear();
        self.answers.clear();
        self.pending.clear();
        self.waiters.clear();
        self.plugins.clear();
        self.backend = None;
        cx.emit(Closed);
        cx.notify();
    }
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
