use crate::{
    backend::Backend,
    ipc::{server::Event, Surface},
    list::{ListElement, NameCache, Snapshot, ROW_HEIGHT},
};
use flea::protocol::Reply;
use gpui::{Context, IntoElement, Pixels, Render, SharedString, Task, Window};
use std::{cell::RefCell, path::Path, rc::Rc};

pub struct Shell {
    path: String,
    backend: Option<Backend>,
    snapshot: Rc<Snapshot>,
    pending_total: Option<usize>,
    requested: usize,
    waiting: bool,
    backend_error: Option<String>,
    cache: Rc<RefCell<NameCache>>,
    _reader: Option<Task<()>>,
    _ipc_reader: Option<Task<()>>,
    cursor: usize,
    selected: Vec<usize>,
}

pub fn visible_rows(height: Pixels) -> usize {
    ((f32::from(height) / ROW_HEIGHT).ceil() as usize).saturating_sub(1)
}

impl Shell {
    pub fn new(
        path: String,
        select: Option<String>,
        events: async_channel::Receiver<Event>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let requested = visible_rows(window.viewport_size().height);
        let title: SharedString = Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&path)
            .to_owned()
            .into();
        let mut shell = Self {
            path: path.clone(),
            backend: None,
            pending_total: None,
            requested,
            waiting: true,
            backend_error: None,
            snapshot: Rc::new(Snapshot {
                title,
                total: 0,
                start: 0,
                listing: 0,
                names: Vec::new(),
            }),
            cache: Rc::new(RefCell::new(NameCache::default())),
            _reader: None,
            _ipc_reader: Some(Self::ipc_reader(events, cx)),
            cursor: 0,
            selected: Vec::new(),
        };
        match Backend::start(path, select, requested) {
            Ok(backend) => {
                let replies = backend.replies.clone();
                shell._reader = Some(cx.spawn(async move |this, cx| {
                    while let Ok(reply) = replies.recv().await {
                        if this
                            .update(cx, |shell, cx| shell.receive(reply, cx))
                            .is_err()
                        {
                            break;
                        }
                    }
                }));
                shell.backend = Some(backend);
            }
            Err(error) => {
                shell.waiting = false;
                shell.record_error(format!("the backend thread could not start: {error}"));
            }
        }
        shell
    }

    fn ipc_reader(events: async_channel::Receiver<Event>, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                if this.update(cx, |shell, cx| shell.ipc(event, cx)).is_err() {
                    break;
                }
            }
        })
    }

    fn ipc(&self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Call { request, answer } => {
                let surface = Surface {
                    path: &self.path,
                    total: self.snapshot.total,
                    cursor: self.cursor,
                    selected: &self.selected,
                };
                let _ = answer.try_send(surface.dispatch(request));
            }
            Event::Quit => cx.quit(),
        }
    }

    fn receive(&mut self, reply: Result<Reply, String>, cx: &mut Context<Self>) {
        match reply {
            Ok(Reply::Listed { path, total }) if path == self.path => {
                self.pending_total = Some(total)
            }
            Ok(Reply::Rows {
                start,
                names,
                listing,
            }) => {
                if listing < self.snapshot.listing {
                    return;
                }
                self.waiting = false;
                let total = self.pending_total.take().unwrap_or(self.snapshot.total);
                if names.len() > self.requested || start > total || names.len() > total - start {
                    eprintln!("flea-gui: the backend answered rows outside the requested window");
                    return;
                }
                self.snapshot = Rc::new(Snapshot {
                    title: self.snapshot.title.clone(),
                    total,
                    start,
                    listing,
                    names: names.into_iter().map(SharedString::from).collect(),
                });
                self.backend_error = None;
                cx.notify();
            }
            Ok(Reply::Error { path, message, .. }) if path == self.path => {
                self.waiting = false;
                self.record_error(message);
            }
            Ok(Reply::Error { path, message, .. }) => {
                eprintln!("flea-gui: ignored an error for {path}: {message}");
            }
            Err(message) => {
                self.waiting = false;
                self.record_error(message);
            }
            _ => {}
        }
    }

    fn record_error(&mut self, message: String) {
        if self.backend_error.as_deref() != Some(&message) {
            eprintln!("flea-gui: {message}");
            self.backend_error = Some(message);
        }
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let wanted = visible_rows(window.viewport_size().height);
        if !self.waiting && wanted != self.requested {
            if let Some(backend) = &self.backend {
                self.requested = wanted;
                match backend.window(self.snapshot.start, wanted) {
                    Ok(()) => {
                        self.waiting = true;
                    }
                    Err(error) => self.record_error(error),
                }
            }
        }
        ListElement::new(Rc::clone(&self.snapshot), Rc::clone(&self.cache))
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
