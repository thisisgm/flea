//! What the service and the card ask of the window: navigation, status, share browsing, peeks and bookmark writes.
use super::*;

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
                self.network_list(cx);
                let Some(side) = self.network_side(*origin) else {
                    return;
                };
                if matches!(effect, Effect::Open { .. }) {
                    self.network_remember_alias(path, uri, label);
                }
                let folder = if matches!(effect, Effect::OpenFile { .. }) {
                    std::path::Path::new(path)
                        .parent()
                        .map_or_else(|| path.clone(), |dir| dir.to_string_lossy().into_owned())
                } else {
                    path.clone()
                };
                let _ = self.step(side, Step::OpenPlace(folder), window, cx);
            }
            Effect::RefreshMounts => self.network_list(cx),
            Effect::Shares {
                uri,
                label,
                names,
                origin,
            } => {
                if self.network_side(*origin).is_some() {
                    let (uri, label, names) = (uri.clone(), label.clone(), names.clone());
                    shares.update(cx, |shares, cx| {
                        shares.show(uri, label, names, *origin, window, cx)
                    });
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
                card.update(cx, |card, cx| card.retry(retry, window, cx));
            }
            Effect::Say { note, origin } => {
                if let Some(side) = self.network_side(*origin) {
                    let text = crate::g2::pl1::words::say(&self.locale.catalog, note);
                    self.network_say(side, text, cx);
                }
            }
            Effect::Peek {
                generation,
                path,
                count,
            } => self.network_peek(*generation, path.clone(), *count, cx),
            Effect::Run { .. } | Effect::Kill(_) | Effect::EnsureBridge { .. } => {}
        }
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
        let read = cx.background_spawn({
            let path = path.clone();
            async move {
                match std::fs::read_dir(&path) {
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
        let (card, service, runner) = (
            network.card.clone(),
            network.service.clone(),
            network.runner.clone(),
        );
        let origin = card.read(cx).model.origin;
        match effect {
            dialog::Effect::Wire(line) => {
                let side = self.network_side(origin).unwrap_or(self.focus_side);
                if let Some(backend) = self.panes[side].as_ref().and_then(|h| h.backend.as_ref()) {
                    let _ = backend.line(line.clone());
                }
            }
            dialog::Effect::Save {
                id,
                uri,
                label,
                replace,
            } => {
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
                self.network_save(edit, Some(*id), cx);
            }
            dialog::Effect::Closed => {
                window.focus(&self.focus, cx);
                cx.notify();
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

    /// One bookmarks edit off the UI thread; a card save answers the card, a rail edit answers the pane.
    pub(crate) fn network_save(
        &mut self,
        edit: bookmarks::Edit,
        id: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        let side = self.focus_side;
        let Some(path) = self.network().map(|network| network.bookmarks.clone()) else {
            return;
        };
        let replies = bookmarks::submit(path, edit.clone());
        // The writer's own thread answers a blocking receive on a background task, so no foreign thread wakes the UI executor.
        let written = cx.background_spawn(async move {
            replies
                .recv_blocking()
                .unwrap_or(Err(bookmarks::Failure::Write))
        });
        let task = cx.spawn(async move |this, cx| {
            let result = written.await;
            let _ = this.update(cx, |shell, cx| {
                if result.is_ok() {
                    shell.network_alias_saved(&edit);
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
                    None if result.is_err() => shell.network_say(side, detail, cx),
                    None => {}
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
