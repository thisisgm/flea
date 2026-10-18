use crate::{
    backend::Backend,
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
    cache: Rc<RefCell<NameCache>>,
    _reader: Option<Task<()>>,
}

pub fn visible_rows(height: Pixels) -> usize {
    ((f32::from(height) / ROW_HEIGHT).ceil() as usize).saturating_sub(1)
}

impl Shell {
    pub fn new(
        path: String,
        select: Option<String>,
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
            snapshot: Rc::new(Snapshot {
                title,
                total: 0,
                start: 0,
                listing: 0,
                names: Vec::new(),
            }),
            cache: Rc::new(RefCell::new(NameCache::default())),
            _reader: None,
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
            Err(error) => eprintln!("flea-gui: the backend thread could not start: {error}"),
        }
        shell
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
                let total = self.pending_total.take().unwrap_or(self.snapshot.total);
                if names.len() > self.requested || start > total || names.len() > total - start {
                    eprintln!("flea-gui: the backend answered rows outside the requested window");
                    return;
                }
                if listing < self.snapshot.listing {
                    return;
                }
                self.snapshot = Rc::new(Snapshot {
                    title: self.snapshot.title.clone(),
                    total,
                    start,
                    listing,
                    names: names.into_iter().map(SharedString::from).collect(),
                });
                self.waiting = false;
                cx.notify();
            }
            Ok(Reply::Error { message, .. }) | Err(message) => {
                self.waiting = false;
                eprintln!("flea-gui: {message}");
            }
            _ => {}
        }
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let wanted = visible_rows(window.viewport_size().height);
        if !self.waiting && wanted != self.requested {
            if let Some(backend) = &self.backend {
                match backend.window(self.snapshot.start, wanted) {
                    Ok(()) => {
                        self.requested = wanted;
                        self.waiting = true;
                    }
                    Err(error) => eprintln!("flea-gui: {error}"),
                }
            }
        }
        ListElement::new(Rc::clone(&self.snapshot), Rc::clone(&self.cache))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[test]
    fn only_visible_rows_are_requested_including_a_partial_last_row() {
        assert_eq!(visible_rows(px(31.)), 0);
        assert_eq!(visible_rows(px(62.)), 1);
        assert_eq!(visible_rows(px(63.)), 2);
        assert_eq!(visible_rows(px(640.)), 20);
    }
}
