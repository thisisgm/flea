//! The Columns window events: watched peek replies and changes, and the one timer behind the landing's deadlines.
use super::{flush, with_area};
use crate::{backend::Event, columns::peeks::Peeked, state::Shell};
use gpui::{Context, Window};

impl Shell {
    pub(super) fn columns_drawn(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(columns) = self.panes[0]
            .as_ref()
            .and_then(|host| host.columns.as_ref())
        {
            columns.paint_pending.set(false);
        }
        if self.shown_view() != super::View::Columns {
            return;
        }
        if let Some(host) = self.panes[0].as_mut() {
            with_area(host, |_, _, _| {});
        }
        self.arm_columns(window, cx);
        cx.notify();
    }

    pub(crate) fn columns_event(
        &mut self,
        side: usize,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return false;
        };
        let now = host.pane.now();
        match event {
            Event::Peeked(document) => {
                if let Some(reply) = Peeked::from_json(document) {
                    match reply {
                        Ok(reply) => {
                            with_area(host, |columns, pane, ports| {
                                let landed = columns.area.peeked(&reply, &pane, ports, now);
                                if landed == crate::columns::peeks::Landed::StaleOrder
                                    && columns.area.visible
                                    && columns.area.drawn.contains(&reply.path)
                                {
                                    columns.area.ask(&pane.lens, &reply.path, false, false);
                                }
                                columns.seen = None;
                            });
                        }
                        Err(key) => host.record_error(
                            crate::backend::Failure {
                                key,
                                detail: String::new(),
                            },
                            &self.locale,
                        ),
                    }
                }
            }
            Event::Changed { path } => {
                with_area(host, |columns, pane, _| {
                    if columns.area.visible {
                        columns.area.changed(path, &pane.lens);
                    }
                });
            }
            Event::Drained | Event::Failed(_) => {
                if let Some(columns) = &mut host.columns {
                    columns.timer = None;
                }
                return false;
            }
            _ => return false,
        }
        flush(host, &self.locale);
        self.arm_columns(window, cx);
        cx.notify();
        matches!(event, Event::Peeked(_))
    }

    pub(super) fn arm_columns(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(host) = self.panes[0].as_mut() else {
            return;
        };
        let Some(columns) = host.columns.as_mut() else {
            return;
        };
        let due = columns
            .area
            .next_deadline()
            .filter(|_| columns.area.visible && !host.stopped);
        let Some(due) = due else {
            columns.timer = None;
            return;
        };
        if columns.timer.as_ref().is_some_and(|(at, _)| *at == due) {
            return;
        }
        let wait = due.saturating_sub(host.pane.now());
        let task = cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(wait))
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |shell, cx| {
                    if let Some(host) = shell.panes[0].as_mut() {
                        let now = host.pane.now();
                        with_area(host, |columns, pane, ports| {
                            columns.timer = None;
                            columns.area.tick(&pane, ports, now);
                        });
                        flush(host, &shell.locale);
                    }
                    shell.arm_columns(window, cx);
                    cx.notify();
                })
            });
        });
        columns.timer = Some((due, task));
    }
}
