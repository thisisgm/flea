//! The card's effects served by the window: backend sends, the PDF page count, refresh, status and launch.
use super::*;

impl Shell {
    pub(super) fn output_effect(
        &mut self,
        effect: Effect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = self.output.owner;
        match effect {
            Effect::Send(line) => {
                let Some(host) = self.panes[side].as_mut() else {
                    self.output_backend_lost(side, cx);
                    return;
                };
                let sent = host
                    .backend
                    .as_ref()
                    .ok_or(crate::backend::Failure {
                        key: "window.backend_stopped",
                        detail: String::new(),
                    })
                    .and_then(|backend| backend.operation(line));
                if let Err(error) = sent {
                    host.record_error(error, &self.locale);
                    self.output_backend_lost(side, cx);
                }
            }
            Effect::DocumentPages { ticket, path } => self.output_pages(ticket, path, cx),
            Effect::Refresh { paths, select } => {
                for pane_side in 0..2 {
                    let Some(host) = self.panes[pane_side].as_mut() else {
                        continue;
                    };
                    let produced = paths
                        .iter()
                        .find(|path| Path::new(path).parent() == Some(Path::new(&host.path)));
                    let Some(path) = produced else {
                        continue;
                    };
                    if select && pane_side == side && !host.search.owner.active() {
                        // PaneWire.qml onConvertDone refreshes with its first output as the landing target.
                        let step = crate::window::navigate::Step::Refresh(Some(path.clone()));
                        let _ = self.step(pane_side, step, window, cx);
                    } else {
                        let now = host.pane.now();
                        let folder = host.path.clone();
                        host.nav.changed(&host.pane, &folder, now);
                        self.arm_watch(pane_side, window, cx);
                    }
                }
            }
            Effect::Status { error, text } => self.status_messages(cx, |messages, now| {
                messages.say(
                    &text,
                    error,
                    "",
                    flea::ui_logic_status_feedback::status::Hint::None,
                    now,
                )
            }),
            Effect::Progress { name, .. } => self.status_messages(cx, |messages, now| {
                messages.set_activity(ACTIVITY, &name, 0, true, false, now)
            }),
            Effect::ClearProgress => self.status_messages(cx, |messages, now| {
                messages.set_activity(ACTIVITY, "", 0, false, false, now)
            }),
            Effect::Launch(argv) => launch(argv, self.locale.catalog.clone()),
            Effect::RestoreFocus => {
                match self.output.origin.take() {
                    Some(origin) => window.focus(&origin, cx),
                    None => window.focus(&self.focus, cx),
                }
                self.keyboard.context = flea::keys::Context::Listing;
            }
            // The card checks its own outputs off the UI thread and never emits this.
            Effect::CheckOutputs { .. } => {}
        }
        cx.notify();
    }

    // The PDF reader the previews use counts the pages on a worker; only the asking ticket lands.
    fn output_pages(&mut self, ticket: Ticket, path: String, cx: &mut Context<Self>) {
        let Some(view) = self.output.view.clone() else {
            return;
        };
        self.output.pages = Some(cx.spawn(async move |_, cx| {
            let count = cx
                .background_executor()
                .spawn(async move {
                    crate::g2::doc_preview::poppler::Document::open(Path::new(&path))
                        .map(|document| u32::try_from(document.page_count()).unwrap_or(u32::MAX))
                        .map_err(|failure| match failure {
                            crate::g2::doc_preview::poppler::Failure::Library(reason) => reason,
                            crate::g2::doc_preview::poppler::Failure::Unreadable => String::new(),
                        })
                })
                .await;
            view.update(cx, |view, cx| {
                view.model.document_pages(ticket, count, &view.catalog);
                view.flush(cx);
            });
        }));
    }
}

// The no-printer card's setup command runs detached, the way a new window does.
fn launch(argv: Vec<String>, catalog: std::sync::Arc<flea::i18n::I18n>) {
    let Some((program, arguments)) = argv.split_first() else {
        return;
    };
    let mut command = std::process::Command::new(program);
    command.args(arguments);
    let _ = std::thread::Builder::new()
        .name("flea-launch".into())
        .spawn(move || {
            if let Err(error) = command.spawn().and_then(|mut child| child.wait()) {
                eprintln!(
                    "flea-gui: {}",
                    catalog.tr_args("window.open_failed", &[&error.to_string()])
                );
            }
        });
}
