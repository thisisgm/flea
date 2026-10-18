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
    backend_error: Option<String>,
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
mod tests {
    use super::*;
    use gpui::{prelude::*, px, size, TestAppContext};

    fn shell_with(backend: Backend) -> Shell {
        Shell {
            path: "/files".into(),
            backend: Some(backend),
            snapshot: Rc::new(Snapshot {
                title: "files".into(),
                total: 20,
                start: 0,
                listing: 2,
                names: Vec::new(),
            }),
            pending_total: None,
            requested: 1,
            waiting: false,
            backend_error: None,
            cache: Rc::new(RefCell::new(NameCache::default())),
            _reader: None,
        }
    }

    #[gpui::test]
    fn adv_e_r1_dead_backend_prints_once_across_many_renders(cx: &mut TestAppContext) {
        const CHILD: &str = "FLEA_TEST_DEAD_BACKEND_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "state::tests::adv_e_r1_dead_backend_prints_once_across_many_renders",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                String::from_utf8_lossy(&output.stderr)
                    .matches("flea-gui: the backend stopped accepting requests")
                    .count(),
                1
            );
            return;
        }
        let (backend, receiver) = Backend::test_channel();
        drop(receiver);
        let cx = cx.add_empty_window();
        let entity = cx.update(|_, cx| cx.new(|_| shell_with(backend)));
        for wanted in 2..66 {
            cx.simulate_resize(size(px(640.), px((wanted + 1) as f32 * ROW_HEIGHT)));
            cx.update(|window, cx| {
                entity.update(cx, |shell, cx| {
                    assert_eq!(visible_rows(window.viewport_size().height), wanted);
                    assert_ne!(shell.requested, wanted);
                    assert!(!shell.waiting);
                    let _ = shell.render(window, cx);
                    assert_eq!(shell.requested, wanted);
                });
            });
        }
    }

    #[gpui::test]
    fn adv_e_r2_unrelated_replies_keep_the_window_request_in_flight(cx: &mut TestAppContext) {
        const CHILD: &str = "FLEA_TEST_UNRELATED_REPLY_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "state::tests::adv_e_r2_unrelated_replies_keep_the_window_request_in_flight",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .filter(|line| *line == "flea-gui: ignored an error for /elsewhere: unrelated")
                    .count(),
                1
            );
            return;
        }
        let (backend, mut requests) = Backend::test_channel();
        let cx = cx.add_empty_window();
        cx.simulate_resize(size(px(640.), px(11. * ROW_HEIGHT)));
        let entity = cx.update(|_, cx| cx.new(|_| shell_with(backend)));
        cx.update(|window, cx| {
            entity.update(cx, |shell, cx| {
                shell.backend_error = Some("existing backend error".into());
                let _ = shell.render(window, cx);
                assert_eq!(requests(), Some((0, 10)));
                assert!(shell.waiting);
            });
        });
        cx.simulate_resize(size(px(640.), px(6. * ROW_HEIGHT)));
        for reply in [
            Reply::Other,
            Reply::Listed {
                path: "/files".into(),
                total: 20,
            },
            Reply::Listed {
                path: "/elsewhere".into(),
                total: 99,
            },
            Reply::Rows {
                start: 0,
                names: vec!["old".into()],
                listing: 1,
            },
            Reply::Error {
                operation: "window".into(),
                path: "/elsewhere".into(),
                message: "unrelated".into(),
            },
        ] {
            let description = format!("{reply:?}");
            cx.update(|window, cx| {
                entity.update(cx, |shell, cx| {
                    let snapshot = Rc::clone(&shell.snapshot);
                    shell.receive(Ok(reply), cx);
                    let _ = shell.render(window, cx);
                    assert_eq!(requests(), None, "{description}");
                    assert!(shell.waiting, "{description}");
                    assert_eq!(shell.requested, 10);
                    assert!(Rc::ptr_eq(&shell.snapshot, &snapshot), "{description}");
                    assert_eq!(
                        shell.backend_error.as_deref(),
                        Some("existing backend error"),
                        "{description}"
                    );
                });
            });
        }
        cx.update(|window, cx| {
            entity.update(cx, |shell, cx| {
                shell.receive(
                    Ok(Reply::Rows {
                        start: 0,
                        names: (0..10).map(|index| format!("file-{index}")).collect(),
                        listing: 2,
                    }),
                    cx,
                );
                assert!(!shell.waiting);
                assert_eq!(shell.snapshot.names.len(), 10);
                let _ = shell.render(window, cx);
                assert_eq!(requests(), Some((0, 5)));
                assert!(shell.waiting);
            });
        });
    }

    #[gpui::test]
    fn adv_e_r2_answered_rows_and_errors_release_the_window_request(cx: &mut TestAppContext) {
        let replies = vec![
            Ok(Reply::Rows {
                start: 0,
                names: vec!["a".into(), "b".into()],
                listing: 3,
            }),
            Ok(Reply::Rows {
                start: 21,
                names: Vec::new(),
                listing: 3,
            }),
            Ok(Reply::Rows {
                start: 0,
                names: vec!["current".into()],
                listing: 2,
            }),
            Ok(Reply::Error {
                operation: "window".into(),
                path: "/files".into(),
                message: "refused".into(),
            }),
            Err("malformed reply".into()),
        ];
        cx.add_empty_window().update(|window, cx| {
            let wanted = visible_rows(window.viewport_size().height);
            assert_ne!(wanted, 1);
            for reply in replies {
                let (backend, mut requests) = Backend::test_channel();
                let entity = cx.new(|_| shell_with(backend));
                entity.update(cx, |shell, cx| {
                    shell.waiting = true;
                    shell.receive(reply, cx);
                    let _ = shell.render(window, cx);
                    assert_eq!(requests(), Some((0, wanted)));
                });
            }
        });
    }

    #[test]
    fn only_visible_rows_are_requested_including_a_partial_last_row() {
        assert_eq!(visible_rows(px(31.)), 0);
        assert_eq!(visible_rows(px(62.)), 1);
        assert_eq!(visible_rows(px(63.)), 2);
        assert_eq!(visible_rows(px(640.)), 20);
    }
}
