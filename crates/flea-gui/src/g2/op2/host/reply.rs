//! Backend lines the archive host takes: formats, archive ends, flash replies and a stopped backend.
use super::*;

impl Shell {
    /// The archive and flash reply lines; true when this host took the line.
    pub(crate) fn archive_line(
        &mut self,
        side: usize,
        line: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !line.contains("\"t\":\"formats\"")
            && !line.contains("\"t\":\"archivedone\"")
            && !line.contains("\"t\":\"flash")
        {
            return false;
        }
        let Ok(reply) = jsondoc::parse(line) else {
            return false;
        };
        let catalog = self.locale.catalog.clone();
        let kind = text(&reply, "t").to_owned();
        let flash_side = self.operations.archive.flash_side == side;
        let effects = match kind.as_str() {
            // Sample input: {"t":"formats","archive":["zip","tar","7z"],"extract":{"archive":true,"sevenZip":true,"zip":true}}.
            "formats" => {
                let extract = reply.get("extract");
                let flag = |key: &str| {
                    extract.and_then(|e| e.get(key)).and_then(Json::as_bool) == Some(true)
                };
                let formats = &mut self.operations.archive.formats[side];
                formats.known = true;
                formats.archive = reply
                    .get("archive")
                    .and_then(Json::as_array)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(Json::as_str)
                    .map(str::to_owned)
                    .collect();
                formats.caps = archive::Caps {
                    seven_zip: flag("sevenZip"),
                    zip: flag("zip"),
                    archive: flag("archive"),
                };
                if self.menu.captured.as_ref().is_some_and(|c| c.side == side) {
                    self.archive_menu_context(side);
                    self.refresh_menu(window, cx);
                }
                return true;
            }
            "archivedone" => {
                let verified = reply.get("verified").and_then(Json::as_bool) != Some(false);
                self.archive_done(side, &wire::parse_archivedone(&reply), verified, window, cx)
            }
            "flashimage" => {
                // The menu's zip probe answers on any side, before the Flash card's own match.
                self.note_menu_zip(side, &reply);
                if !flash_side {
                    return true;
                }
                let path = self
                    .operations
                    .archive
                    .flash
                    .image_path()
                    .unwrap_or_default()
                    .to_owned();
                if let Some(image) = wire::parse_flashimage(&reply, &path) {
                    self.operations.archive.flash.receive_image(&path, &image);
                }
                Vec::new()
            }
            "flashtargets" if flash_side => {
                self.operations
                    .archive
                    .flash
                    .receive_targets(wire::parse_flashtargets(&reply));
                Vec::new()
            }
            "flashstarted" if flash_side => {
                let started = wire::parse_flashstarted(&reply);
                let now = self.archive_now(cx);
                self.operations.archive.flash.start_transfer(
                    &catalog,
                    started.id,
                    started.total,
                    started.known,
                    now,
                );
                Vec::new()
            }
            "flashprogress" if flash_side => {
                let now = self.archive_now(cx);
                self.operations.archive.flash.progress_transfer(
                    &catalog,
                    &wire::parse_flashprogress(&reply),
                    now,
                )
            }
            "flashdone" if flash_side => self
                .operations
                .archive
                .flash
                .finish_transfer(&catalog, &wire::parse_flashdone(&reply)),
            "flasheject" if flash_side => self
                .operations
                .archive
                .flash
                .finish_eject(&catalog, &wire::parse_flasheject(&reply)),
            _ => return kind.starts_with("flash"),
        };
        self.archive_effects(side, effects, window, cx);
        true
    }

    /// An extract this window asked for opens the password card on encrypted or wrong-password; every other end is said.
    fn archive_done(
        &mut self,
        side: usize,
        done: &wire::ArchiveDone,
        verified: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Effect> {
        let catalog = self.locale.catalog.clone();
        if let Some(view) = self.operation_view(side) {
            let id = done.id;
            view.update(cx, |overlay, cx| {
                if overlay.model.extract_done(id) {
                    cx.notify();
                }
            });
            self.flush_operations(side, cx);
        }
        let action = archive::done_action(done.ok, &done.err);
        if !action.is_empty() {
            let asked = self
                .operations
                .archive
                .extracts
                .iter()
                .any(|(s, p, d)| *s == side && *p == done.path && *d == done.dest);
            if !asked {
                return Vec::new();
            }
            if self.operations.archive.card_open() {
                return vec![Effect::Say {
                    text: super::super::busy_sentence(&catalog),
                    error: true,
                }];
            }
            self.operations.archive.card_side = side;
            let err = if action == "wrong" {
                done.err.as_str()
            } else {
                ""
            };
            self.operations.archive.eye.set(None);
            self.operations.archive.confirm_eye.set(None);
            let effects = self
                .operations
                .archive
                .extract
                .open(&done.path, &done.dest, err, &catalog);
            window.refresh();
            return effects;
        }
        self.operations.archive.extracts.retain(|(s, p, d)| {
            !(*s == side && (done.path.is_empty() || (*p == done.path && *d == done.dest)))
        });
        if done.ok {
            // PaneWire.qml onArchiveDone re-reads in place and lands at the refreshed list's first row.
            let _ = self.step(side, crate::window::navigate::Step::Fresh, window, cx);
        }
        let text = if done.ok {
            catalog.tr(if verified {
                "archive.done.written"
            } else {
                "archive.done.unverified"
            })
        } else if done.err == "cancelled" {
            catalog.tr("archive.done.cancelled")
        } else if let Some(refusal) = flea::ui_logic_fb1::errors::extract_refusal(&done.err) {
            refusal.render(&catalog)
        } else {
            capitalised(&done.err)
        };
        vec![Effect::Say {
            text,
            error: !done.ok && done.err != "cancelled",
        }]
    }

    /// A backend that stopped ends the cards it owned with their sentence and forgets what it reported.
    pub(crate) fn archive_failed(&mut self, side: usize, cx: &mut Context<Self>) {
        let archive = &mut self.operations.archive;
        archive.formats[side] = Formats::default();
        archive.extracts.retain(|(s, _, _)| *s != side);
        self.archive_flash_failed(side, cx);
        let archive = &mut self.operations.archive;
        if !archive.card_open() || archive.card_side != side {
            cx.notify();
            return;
        }
        let name = archive
            .extract
            .opened()
            .then(|| archive.extract.leaf().to_owned());
        archive.compress = None;
        archive.extract.close();
        archive.card_menu = None;
        let line = archive::backend_stopped_line(&self.locale.catalog, name.as_deref());
        self.archive_say(&line, true, cx);
        cx.notify();
    }
}
