//! What the service and the card ask of the window: navigation, status, share browsing, peeks and bookmark writes.
use super::*;

// The status lane's owner for a network connection still opening, apart from every pane and job owner.
const NETWORK_OPENING: u64 = 1 << 34;

impl Shell {
    pub(crate) fn network_service(
        &mut self,
        effect: &Effect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(network) = self.network() else {
            return;
        };
        let (card, shares, service) = (
            network.card.clone(),
            network.shares.clone(),
            network.service.clone(),
        );
        match effect {
            Effect::Open {
                path,
                uri,
                label,
                origin,
            }
            | Effect::OpenFile {
                path,
                uri,
                label,
                origin,
            } => {
                self.network_opening(*origin, "", cx);
                let Some(side) = self.network_side(*origin) else {
                    self.drop_rail_network_open(*origin);
                    return;
                };
                self.network_connected(side, cx);
                if matches!(effect, Effect::Open { .. }) {
                    self.remember_path_alias(path, label, uri);
                }
                let folder = if matches!(effect, Effect::OpenFile { .. }) {
                    std::path::Path::new(path)
                        .parent()
                        .map_or_else(|| path.clone(), |dir| dir.to_string_lossy().into_owned())
                } else {
                    path.clone()
                };
                let opened = self.open_rail_place(side, &folder, window, cx);
                let landed = opened && matches!(effect, Effect::Open { .. });
                self.land_rail_network_open(*origin, side, landed, window, cx);
                // A completed card whose Closed event is still queued returns the keyboard to the opened list.
                if landed
                    && self.lifecycle.phase() == crate::window::lifecycle::Phase::Open
                    && side == self.focus_side
                    && self.keyboard.context == flea::keys::Context::Rail
                    && !card.read(cx).model.opened
                    && card.read(cx).holds_focus(window, cx)
                {
                    if let Some(network) = self.network() {
                        if network.return_focus == Some((side, true)) {
                            network.return_focus = Some((side, false));
                        }
                    }
                }
            }
            Effect::RefreshMounts => {
                // The phone rows read the rail source's own listing, so a mount that lands re-lists it too, as v0.3.12's one shared poll did.
                if let Some(join) = &self.rail {
                    let source = join.devices.clone();
                    source.update(cx, |source, cx| source.poll_mounts(cx));
                }
                self.network_list(cx);
            }
            Effect::Shares {
                uri,
                label,
                names,
                origin,
            } => {
                if let Some(side) = self.network_side(*origin) {
                    self.network_connected(side, cx);
                    self.network_remember_focus();
                    let previous = window.focused(cx);
                    let (uri, label, names) = (uri.clone(), label.clone(), names.clone());
                    shares.update(cx, |shares, cx| {
                        shares.show(uri, label, names, *origin, window, cx)
                    });
                    if side != self.focus_side {
                        if let Some(previous) = previous {
                            window.focus(&previous, cx);
                        } else {
                            window.blur(cx);
                        }
                    }
                    cx.notify();
                } else {
                    service.update(cx, |service, _| service.close_browser(*origin));
                }
            }
            Effect::Completed { .. } => {
                card.update(cx, |card, cx| card.completed(effect, cx));
            }
            Effect::Retry {
                uri,
                label,
                origin,
                error,
                args,
                connect_failed,
                password,
            } => {
                self.drop_rail_network_open(*origin);
                self.network_opening(*origin, "", cx);
                if self.network_side(*origin).is_none() {
                    return;
                }
                let retry = Effect::Retry {
                    uri: uri.clone(),
                    label: label.clone(),
                    origin: *origin,
                    error,
                    args: args.clone(),
                    connect_failed: *connect_failed,
                    password: Secret::from_bytes(password.expose()),
                };
                let side = self.network_side(*origin).expect("live network origin");
                let text = self
                    .locale
                    .catalog
                    .tr_args(error, &args.iter().map(String::as_str).collect::<Vec<_>>());
                if *connect_failed {
                    self.network_connect_error(side, &text);
                }
                // The sign-in invitation lives on the card alone; the pane has no failure to report.
                if *error != crate::g2::pl2::mount::GUEST_EMPTY {
                    self.network_say(side, text, *connect_failed, cx);
                }
                self.network_remember_focus();
                card.update(cx, |card, cx| card.retry(retry, window, cx));
            }
            Effect::Say { note, origin } => {
                use crate::g2::pl2::result::{say_role, SayRole};
                let role = say_role(note.key);
                // An opening note precedes the landing, so only a final note ends the rail's open claim.
                if role != SayRole::Opening {
                    self.drop_rail_network_open(*origin);
                }
                if role == SayRole::Error {
                    self.network_opening(*origin, "", cx);
                }
                if let Some(side) = self.network_side(*origin) {
                    let text = crate::g2::pl1::words::say(&self.locale.catalog, note);
                    self.network_say(side, text, role == SayRole::Error, cx);
                }
            }
            Effect::Peek {
                generation,
                path,
                count,
            } => self.network_peek(*generation, path.clone(), *count, cx),
            Effect::Starting { note, origin } if self.network_side(*origin).is_some() => {
                let text = crate::g2::pl1::words::say(&self.locale.catalog, note);
                self.network_opening(*origin, &text, cx);
            }
            Effect::Starting { .. } => {}
            // Settled clears whatever origin it names, so a pane closed mid-wait never strands the line.
            Effect::Settled { origin } => self.network_opening(*origin, "", cx),
            Effect::Run { .. } | Effect::Kill(_) | Effect::EnsureBridge { .. } => {}
        }
    }

    /// The connection's Starting line rides the status lane as an activity, as v0.3.12's stickyStarting does; empty ends it.
    fn network_opening(&mut self, origin: u64, text: &str, cx: &mut Context<Self>) {
        self.status_messages(cx, |messages, now| {
            messages.set_activity(
                NETWORK_OPENING | origin,
                text,
                0,
                false,
                !text.is_empty(),
                now,
            )
        });
    }

    /// A bounded directory read off the UI thread; the service drops it unless its generation and path still match.
    fn network_peek(
        &mut self,
        generation: u64,
        path: String,
        count: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(network) = self.network() else {
            return;
        };
        let service = network.service.clone();
        let reading_path = path.clone();
        let read = cx.background_executor().spawn(async move {
            match std::fs::read_dir(&reading_path) {
                Ok(entries) => (
                    entries
                        .filter_map(Result::ok)
                        .take(count)
                        .map(|entry| entry.file_name().to_string_lossy().into_owned())
                        .collect::<Vec<_>>(),
                    false,
                ),
                Err(_) => (Vec::new(), true),
            }
        });
        let task = cx.spawn(async move |_, cx| {
            let (names, failed) = read.await;
            let login = std::env::var("USER").unwrap_or_default();
            service.update(cx, |service, cx| {
                let now = service.now(cx);
                service.peeked(generation, &path, &names, failed, &login, now, cx);
            });
        });
        if let Some(network) = self.network() {
            network.tasks.retain(|task| !task.is_ready());
            network.tasks.push(task);
        }
    }

    pub(crate) fn network_card(
        &mut self,
        effect: &dialog::Effect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(network) = self.network() else {
            return;
        };
        let (service, runner, shares) = (
            network.service.clone(),
            network.runner.clone(),
            network.shares.clone(),
        );
        match effect {
            dialog::Effect::Save {
                id,
                uri,
                label,
                replace,
            } => {
                if replace.is_none() {
                    self.save_network_favourite(*id, uri, label, window, cx);
                    return;
                }
                let edit = match replace {
                    Some(old) => bookmarks::Edit::Replace {
                        old: old.clone(),
                        uri: uri.clone(),
                        label: label.clone(),
                    },
                    None => bookmarks::Edit::Rename {
                        uri: uri.clone(),
                        label: label.clone(),
                    },
                };
                self.network_save(edit, Some(*id), None, cx);
            }
            dialog::Effect::Closed => {
                // Only a browser on the focused pane keeps the keyboard; one on the other pane must not strand it.
                let browser = &shares.read(cx).model;
                if !browser.open || self.network_side(browser.origin) != Some(self.focus_side) {
                    self.network_restore_focus(window, cx);
                }
            }
            dialog::Effect::InstallDropbox => {
                let argv = flea::ui_logic_pl2::DROPBOX_INSTALL
                    .map(str::to_owned)
                    .to_vec();
                drop(runner.start(crate::g2::pl1::process::Job::plain(
                    &argv.iter().map(String::as_str).collect::<Vec<_>>(),
                )));
            }
            dialog::Effect::Mount {
                id,
                uri,
                label,
                origin,
                password,
            } => {
                let mount = dialog::Effect::Mount {
                    id: *id,
                    uri: uri.clone(),
                    label: label.clone(),
                    origin: *origin,
                    password: Secret::from_bytes(password.expose()),
                };
                service.update(cx, |service, cx| service.dialog_effect(mount, cx));
            }
            dialog::Effect::Cancel(id) => {
                let id = *id;
                service.update(cx, |service, cx| {
                    service.dialog_effect(dialog::Effect::Cancel(id), cx)
                });
            }
            dialog::Effect::CheckDropbox(id) => {
                let id = *id;
                service.update(cx, |service, cx| {
                    service.dialog_effect(dialog::Effect::CheckDropbox(id), cx)
                });
            }
            dialog::Effect::CancelDropbox(id) => {
                let id = *id;
                service.update(cx, |service, cx| {
                    service.dialog_effect(dialog::Effect::CancelDropbox(id), cx)
                });
            }
        }
    }

    /// One bookmarks edit off the UI thread; a card save answers the card, a rail edit answers the pane, with `said` read once the file changed, then once it already lacked the line.
    pub(crate) fn network_save(
        &mut self,
        edit: bookmarks::Edit,
        id: Option<u64>,
        said: Option<[String; 2]>,
        cx: &mut Context<Self>,
    ) {
        let side = self.focus_side;
        let Some(path) = self.network().map(|network| network.bookmarks.clone()) else {
            return;
        };
        let replies = bookmarks::submit(path, edit.clone());
        // The writer's own thread answers a blocking receive on a background task, so no foreign thread wakes the UI executor.
        let written = cx.background_executor().spawn(async move {
            replies
                .recv_blocking()
                .unwrap_or(Err(bookmarks::Failure::Write))
        });
        let task = cx.spawn(async move |this, cx| {
            let result = written.await;
            let _ = this.update(cx, |shell, cx| {
                if result.is_ok() {
                    shell.network_saved(cx);
                }
                let detail = match &result {
                    Ok(_) => String::new(),
                    Err(failure) => shell.locale.catalog.tr(failure.key()),
                };
                match id {
                    Some(id) => {
                        if let Some(network) = shell.network() {
                            let card = network.card.clone();
                            card.update(cx, |card, cx| card.saved(id, result.is_ok(), &detail, cx));
                        }
                    }
                    None => match (result, said) {
                        (Err(_), _) => shell.network_say(side, detail, true, cx),
                        (Ok(changed), Some([written, unchanged])) => {
                            let text = if changed { written } else { unchanged };
                            shell.network_say(side, text, false, cx);
                        }
                        (Ok(_), None) => {}
                    },
                }
                shell.network_list(cx);
            });
        });
        if let Some(network) = self.network() {
            network.tasks.retain(|task| !task.is_ready());
            network.tasks.push(task);
        }
    }
}
