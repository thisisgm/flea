//! Delete then Ctrl+Z: whatever order the trash reply, the undo reply, the watch notices and the listings take, the rows a frame draws are the rows the pane counts.
use super::super::key_rows_tests::launched;
use super::super::menus_tests::home;
use crate::{
    backend::{Backend, Event as BackendEvent},
    state::Shell,
};
use flea::protocol::{Reply, RowFacts};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::time::Duration;

#[path = "undo_finish_tests.rs"]
mod finish;

fn without_notes() -> Vec<RowFacts> {
    home()
        .into_iter()
        .filter(|row| row.name != "notes.txt")
        .collect()
}

#[derive(Clone, Copy, Debug)]
enum Step {
    Delete,
    Undo,
    Trashed,
    Undone,
    Changed,
    // The listing out answers with the folder as the disk holds it then.
    Land(bool),
    Wait(u64),
}

struct Run<'a> {
    shell: Entity<Shell>,
    cx: &'a mut VisualTestContext,
    read: Box<dyn FnMut() -> Option<String>>,
    listing: usize,
    sent: Vec<String>,
}

impl Run<'_> {
    // One frame the way the window draws one: no forced refresh, so a cached shell nobody notified stays as it was.
    fn frame(&mut self) {
        self.cx.update(|window, cx| window.draw(cx).clear(cx));
        self.cx.run_until_parked();
        while let Some(line) = (self.read)() {
            self.sent.push(line);
        }
    }

    fn receive(&mut self, event: BackendEvent) {
        let shell = self.shell.clone();
        self.cx
            .update(|window, cx| shell.update(cx, |shell, cx| shell.receive(0, event, window, cx)));
        self.frame();
    }

    fn step(&mut self, step: Step) {
        match step {
            Step::Delete => self.cx.simulate_keystrokes("delete"),
            Step::Undo => self.cx.simulate_keystrokes("ctrl-z"),
            Step::Trashed => self.receive(Backend::test_reply_line(
                r#"{"t":"trashed","ok":1,"failed":0}"#,
            )),
            Step::Undone => self.receive(Backend::test_reply_line(
                r#"{"t":"undone","op":"trash","ok":true}"#,
            )),
            Step::Changed => self.receive(BackendEvent::Changed {
                path: "/files".into(),
            }),
            Step::Land(restored) => {
                let busy = self.shell.read_with(self.cx, |shell, _| {
                    shell.panes[0].as_ref().unwrap().pane.busy()
                });
                if busy {
                    let rows = if restored { home() } else { without_notes() };
                    self.listing += 1;
                    let total = rows.len();
                    self.receive(BackendEvent::Reply(Reply::Listed {
                        path: "/files".into(),
                        total,
                    }));
                    self.receive(BackendEvent::Reply(Reply::Rows {
                        start: 0,
                        rows,
                        listing: self.listing,
                    }));
                }
            }
            Step::Wait(ms) => {
                self.cx.executor().advance_clock(Duration::from_millis(ms));
                self.frame();
            }
        }
        self.frame();
    }

    // Quiet: every listing out answers with the restored folder until none is left.
    fn quiesce(&mut self) {
        for _ in 0..8 {
            self.step(Step::Wait(500));
            self.step(Step::Land(true));
        }
        self.step(Step::Wait(500));
    }

    fn agree(&mut self, label: &str) {
        let (held, drawn, total, drawn_total) = self.shell.read_with(self.cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let list = host.list.borrow();
            let names = |held: &crate::list::pane::Held| {
                held.rows
                    .iter()
                    .map(|row| row.name.clone())
                    .collect::<Vec<_>>()
            };
            (
                names(&host.pane.held),
                names(&list.drawn_rows),
                host.pane.held.total,
                list.drawn_rows.total,
            )
        });
        assert_eq!(
            (total, &held),
            (drawn_total, &drawn),
            "{label}: the pane counts {total} {held:?} but the frame drew {drawn_total} {drawn:?}; sent {:?}",
            self.sent
        );
        assert!(
            held.iter().any(|name| name == "notes.txt"),
            "{label}: the restored file is not listed: {held:?}; sent {:?}",
            self.sent
        );
    }
}

fn story(cx: &mut TestAppContext, label: &str, steps: &[Step]) {
    let (shell, cx) = launched(cx, "list");
    let (backend, read) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    let mut run = Run {
        shell: shell.clone(),
        cx,
        read: Box::new(read),
        listing: 1,
        sent: Vec::new(),
    };
    for step in steps {
        run.step(*step);
    }
    run.quiesce();
    run.agree(label);
    shell.update(run.cx, |shell, _| shell.stop_for_app_quit());
}

use Step::*;

#[gpui::test]
fn in_order(cx: &mut TestAppContext) {
    story(
        cx,
        "in order",
        &[
            Delete,
            Trashed,
            Land(false),
            Changed,
            Wait(450),
            Land(false),
            Wait(1000),
            Undo,
            Undone,
            Changed,
            Wait(450),
            Land(true),
        ],
    );
}

#[gpui::test]
fn notice_before_trash_reply(cx: &mut TestAppContext) {
    story(
        cx,
        "notice first",
        &[
            Delete,
            Changed,
            Trashed,
            Land(false),
            Wait(450),
            Land(false),
            Wait(1000),
            Undo,
            Changed,
            Undone,
            Wait(450),
            Land(true),
        ],
    );
}

#[gpui::test]
fn undo_while_the_watch_reread_is_out(cx: &mut TestAppContext) {
    story(
        cx,
        "undo during watch reread",
        &[
            Delete,
            Trashed,
            Changed,
            Land(false),
            Wait(450),
            Undo,
            Undone,
            Changed,
            Land(false),
            Wait(450),
            Land(true),
        ],
    );
}

#[gpui::test]
fn undo_while_the_trash_reread_is_out(cx: &mut TestAppContext) {
    story(
        cx,
        "undo during trash reread",
        &[
            Delete,
            Trashed,
            Undo,
            Changed,
            Undone,
            Land(false),
            Wait(450),
            Land(true),
        ],
    );
}

#[gpui::test]
fn restore_notice_lands_inside_the_trash_burst(cx: &mut TestAppContext) {
    story(
        cx,
        "one burst",
        &[
            Delete,
            Changed,
            Trashed,
            Wait(100),
            Undo,
            Changed,
            Undone,
            Land(false),
            Wait(400),
            Land(true),
        ],
    );
}

#[gpui::test]
fn stale_listing_after_the_restore_notice(cx: &mut TestAppContext) {
    story(
        cx,
        "stale after notice",
        &[
            Delete,
            Trashed,
            Land(false),
            Changed,
            Wait(450),
            Undo,
            Changed,
            Undone,
            Land(false),
            Wait(450),
            Land(true),
        ],
    );
}
