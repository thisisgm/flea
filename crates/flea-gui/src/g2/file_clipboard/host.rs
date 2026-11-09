//! Backend lines in, pastes and picks through, fork ownership bridged; each method returns its status lines and the outbox drains to the wire, sink and owner.
use super::seams::{Captured, MenuAction, Owner, Sink, Wire};
use super::{Clipboard, Landing, Take};
use flea::i18n::I18n;
use flea::ui_logic_context_menus::copyas::Format;
use flea::ui_logic_file_clipboard::{self as core};

impl Clipboard {
    // One backend line: clip replies reach the session, paths replies reach the pending take or Copy As, everything else is unclaimed for its owner.
    #[allow(clippy::too_many_arguments)]
    pub fn line(
        &mut self,
        i18n: &I18n,
        backend: u64,
        text: &str,
        take: Take<'_>,
        landing: Landing<'_>,
        wire: &dyn Wire,
        sink: &mut dyn Sink,
        owner: &mut dyn Owner,
    ) -> (bool, Vec<core::Said>) {
        if let Ok(Some(event)) = core::parse_clip(text) {
            return (
                true,
                self.clip_event(i18n, backend, event, landing, wire, sink, owner),
            );
        }
        if let Ok(Some(list)) = core::parse_paths(text) {
            if self.paths_pending(backend) {
                return (
                    true,
                    self.paths_resolved(i18n, backend, list, take, wire, sink, owner),
                );
            }
            return (false, Vec::new());
        }
        (false, Vec::new())
    }

    #[allow(clippy::too_many_arguments)]
    fn clip_event(
        &mut self,
        i18n: &I18n,
        backend: u64,
        event: core::ClipEvent,
        landing: Landing<'_>,
        wire: &dyn Wire,
        sink: &mut dyn Sink,
        owner: &mut dyn Owner,
    ) -> Vec<core::Said> {
        match event {
            core::ClipEvent::Set { ok, token, error } => {
                let (outs, _validated) = self.session.on_set(i18n, backend, ok, &token, &error);
                // The B prerequisite (validate and mint a token without an owner) is a join seam: until it lands the fork publishes nothing, B's clipSet is the one claim, and the validated clip is what the seam will publish.
                self.drain(outs, wire, sink, owner)
            }
            core::ClipEvent::Changed {
                clip,
                paths,
                token,
                error,
            } => {
                let outs = self
                    .session
                    .on_changed(backend, &clip, paths, token, &error);
                self.drain(outs, wire, sink, owner)
            }
            core::ClipEvent::Get {
                ok,
                clip,
                paths,
                token,
                ..
            } => {
                let (_, pendings) = self.session.on_get(backend, ok, &clip, paths, token);
                let mut said = Vec::new();
                for pending in &pendings {
                    let outs = core::plan(
                        self.session.current(),
                        i18n,
                        pending,
                        landing.path,
                        landing.list_in_flight,
                        landing.recent,
                        landing.stamp,
                    );
                    said.extend(self.drain(outs, wire, sink, owner));
                }
                said
            }
            core::ClipEvent::Clear { ok, error } => {
                let outs = self.session.on_clear(i18n, ok, &error);
                self.drain(outs, wire, sink, owner)
            }
        }
    }

    // Copy As over rows resolves through the backend as clip does; over paths the text goes out at once, and the shared paths slot refuses another asker (refuseWhileBusy). Relative Path reads from take.path, the listed folder, kept on the claim.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_as(
        &mut self,
        i18n: &I18n,
        backend: u64,
        format: Format,
        paths: Option<Vec<String>>,
        rows: Vec<usize>,
        take: Take<'_>,
        wire: &dyn Wire,
        owner: &mut dyn Owner,
    ) -> Vec<core::Said> {
        match paths {
            Some(paths) => match core::copy_as_text(&paths, format, take.path) {
                Some(text) => {
                    owner.publish_text(text);
                    Vec::new()
                }
                None => Vec::new(),
            },
            None => {
                if take.paths_busy || self.paths_pending(backend) {
                    return vec![core::Said {
                        text: core::resolving(i18n),
                        error: false,
                        paste_hint: false,
                    }];
                }
                if rows.is_empty() {
                    return Vec::new();
                }
                self.copy_as.insert(backend, (format, take.path.to_owned()));
                wire.send(core::paths_request_line(&rows));
                Vec::new()
            }
        }
    }

    // A menu pick: Copy As leaves publish text, Copy path publishes the first path, Paste As leaves paste from the file clipboard, never the snapshot; an empty snapshot falls back to the pane's rows (Ops.copyAs).
    #[allow(clippy::too_many_arguments)]
    pub fn menu_action(
        &mut self,
        i18n: &I18n,
        backend: u64,
        action: &str,
        snapshot: &[String],
        cursor_path: &str,
        rows: Vec<usize>,
        take: Take<'_>,
        landing: Landing<'_>,
        wire: &dyn Wire,
        sink: &mut dyn Sink,
        owner: &mut dyn Owner,
    ) -> Vec<core::Said> {
        match MenuAction::parse(action) {
            MenuAction::CopyAs(leaf) => match Format::parse(&leaf) {
                // Recent has no folder of its own and its flyout hides Relative Path, so a pick that still arrives copies nothing.
                Some(Format::Relative) if landing.recent => Vec::new(),
                Some(format) => {
                    let paths = if snapshot.is_empty() {
                        None
                    } else {
                        Some(snapshot.to_vec())
                    };
                    self.copy_as(i18n, backend, format, paths, rows, take, wire, owner)
                }
                None => Vec::new(),
            },
            MenuAction::CopyFirst => {
                let first = snapshot.first().map(String::as_str).unwrap_or(cursor_path);
                if first.is_empty() {
                    return Vec::new();
                }
                owner.publish_text(first.to_string());
                Vec::new()
            }
            MenuAction::PasteAs(kind) => {
                if landing.recent {
                    return vec![core::recent_paste(i18n)];
                }
                self.paste(
                    i18n,
                    backend,
                    core::PendingPaste::link(landing.path, kind),
                    landing,
                    wire,
                    sink,
                    owner,
                )
            }
            MenuAction::Unhandled => Vec::new(),
        }
    }

    // Paste now, or read the system selection first while the watch is down.
    #[allow(clippy::too_many_arguments)]
    pub fn paste(
        &mut self,
        i18n: &I18n,
        backend: u64,
        pending: core::PendingPaste,
        landing: Landing<'_>,
        wire: &dyn Wire,
        sink: &mut dyn Sink,
        owner: &mut dyn Owner,
    ) -> Vec<core::Said> {
        let (outs, read) = self.session.read_for_paste(i18n, backend, pending.clone());
        let mut said = self.drain(outs, wire, sink, owner);
        if read == core::ReadOut::Now {
            let outs = core::plan(
                self.session.current(),
                i18n,
                &pending,
                landing.path,
                landing.list_in_flight,
                landing.recent,
                landing.stamp,
            );
            said.extend(self.drain(outs, wire, sink, owner));
        }
        said
    }

    // A transfer spent the cut it captured: clear exactly that cut, never a newer selection; the fork prerequisite is not at BASE, so only the wire clear goes out.
    pub fn spent(
        &mut self,
        backend: u64,
        captured: &Captured,
        spends_cut: bool,
        wire: &dyn Wire,
        sink: &mut dyn Sink,
        owner: &mut dyn Owner,
    ) -> Vec<core::Said> {
        let clip = core::Clip {
            paths: captured.paths.clone(),
            moving: captured.moving,
            token: captured.token.clone(),
        };
        let outs = self.session.spent(&clip, &captured.entry, spends_cut);
        let _ = backend;
        self.drain(outs, wire, sink, owner)
    }

    // Foreign selections the fork read apply in the fork's own monotonic generation order, so a late old report never clears a new owner.
    pub fn foreign(&mut self, owner: &mut dyn Owner) {
        for foreign in owner.drain_foreign() {
            if self
                .foreign_generation
                .is_some_and(|seen| foreign.generation <= seen)
            {
                continue;
            }
            self.foreign_generation = Some(foreign.generation);
            self.session
                .foreign(&foreign.clip, foreign.paths, foreign.token);
        }
    }

    fn refresh_marks(&mut self) {
        let revision = self.session.revision();
        let current = self.session.current();
        if current.paths.is_empty() {
            if self.marks.cached() {
                self.marks.release();
            }
            return;
        }
        if self.marks.revision() == Some(revision) {
            return;
        }
        let paths = &current.paths;
        let moving = current.moving;
        self.marks.set_for(revision, paths, moving);
        self.builds += 1;
    }

    /// The clipboard slot beside a name: copy, scissors or nothing, following the clipboard; one lookup build per selection, then one key read per row.
    pub fn mark_for(&mut self, path: &str) -> &'static str {
        self.refresh_marks();
        self.marks.mark_for(path)
    }

    /// A row's mark by its name under the pane base; an empty clip returns before any join, and a row probes through one reused buffer.
    pub fn mark_for_row(&mut self, base: &str, name: &str) -> &'static str {
        self.calls += 1;
        if self.session.current().paths.is_empty() {
            self.refresh_marks();
            return "";
        }
        if name.is_empty() {
            return "";
        }
        let mut probe = std::mem::take(&mut self.probe);
        core::row_path_into(&mut probe, base, name);
        self.refresh_marks();
        let mark = self.marks.mark_for(&probe);
        self.probe = probe;
        mark
    }
}
