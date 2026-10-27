//! Shelf additions use the existing CLI on the asynchronous process runner.
use crate::{
    g2::pl1::process::{Event, Handle, Job, Outcome, Runner, ThreadRunner},
    state::Shell,
};
use gpui::{Context, Task, Window};
use std::{collections::VecDeque, rc::Rc};

#[derive(Default)]
pub(crate) struct Shelf {
    queue: VecDeque<(usize, Vec<String>)>,
    child: Option<Handle>,
    task: Option<Task<()>>,
}

impl Drop for Shelf {
    fn drop(&mut self) {
        if let Some(child) = &self.child {
            child.kill();
        }
    }
}

#[cfg(test)]
pub(super) struct Services(pub Rc<dyn Runner>);

#[cfg(test)]
impl gpui::Global for Services {}

fn runner(_cx: &gpui::App) -> Rc<dyn Runner> {
    #[cfg(test)]
    if let Some(services) = _cx.try_global::<Services>() {
        return services.0.clone();
    }
    Rc::new(ThreadRunner)
}

impl Shell {
    pub(crate) fn shelf_add(
        &mut self,
        side: usize,
        paths: Vec<String>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if paths.is_empty() {
            self.shelf_notice(side, self.locale.catalog.tr("menu.shelf.empty"), true, cx);
            return;
        }
        self.menu.shelf.queue.push_back((side, paths));
        if self.menu.shelf.child.is_none() {
            self.shelf_next(window, cx);
        }
    }

    fn shelf_next(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some((side, paths)) = self.menu.shelf.queue.pop_front() else {
            return;
        };
        let count = paths.len();
        let mut argv = vec![
            crate::window::startup::binary()
                .to_string_lossy()
                .into_owned(),
            "shelf".into(),
            "add".into(),
        ];
        argv.extend(paths);
        let handle = runner(cx).start(Job {
            argv,
            ..Job::default()
        });
        let events = handle.events.clone();
        self.menu.shelf.child = Some(handle);
        self.menu.shelf.task = Some(cx.spawn_in(window, async move |this, cx| {
            let outcome = loop {
                match events.recv().await {
                    Ok(Event::Exited(outcome)) => break outcome,
                    Ok(Event::Started) => {}
                    Err(_) => break Outcome::code(-1),
                }
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.menu.shelf.child = None;
                let said = if outcome.code == 0 {
                    if count == 1 {
                        shell.locale.catalog.tr("menu.shelf.one")
                    } else {
                        shell
                            .locale
                            .catalog
                            .tr_args("menu.shelf.many", &[&count.to_string()])
                    }
                } else {
                    let last = outcome.stderr.trim().lines().last().unwrap_or("");
                    let last = last.strip_prefix("flea: ").unwrap_or(last);
                    if last.is_empty() {
                        shell.locale.catalog.tr("menu.shelf.failed")
                    } else {
                        last.to_owned()
                    }
                };
                shell.shelf_notice(side, said, outcome.code != 0, cx);
                shell.shelf_next(window, cx);
            });
        }));
    }

    fn shelf_notice(&mut self, side: usize, sentence: String, error: bool, cx: &mut Context<Self>) {
        if let Some(host) = self.panes[side].as_mut() {
            host.status = sentence.clone().into();
        }
        self.status_messages(cx, |messages, now| {
            messages.say(
                &sentence,
                error,
                "",
                flea::ui_logic_fb1::status::Hint::None,
                now,
            )
        });
        cx.notify();
    }
}
