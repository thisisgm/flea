//! The Copy To and Send To card's field focus, close, confirm and validate reply on Operations.
use super::*;

impl Operations {
    pub fn copy_enabled(path: &str) -> bool {
        !path.is_empty()
    }

    pub(in super::super) fn walk_copy_focus(&mut self, key: Key) {
        let sending = matches!(self.dialog, Some(Dialog::SendTo { .. }));
        let Some(
            Dialog::CopyTo {
                focus, busy, path, ..
            }
            | Dialog::SendTo {
                focus, busy, path, ..
            },
        ) = &mut self.dialog
        else {
            return;
        };
        let enabled = if sending {
            flea::ui_logic_sd2::folder_check(path).is_ok()
        } else {
            Self::copy_enabled(path)
        };
        let order: &[_] = if *busy {
            &[CopyFocus::Cancel]
        } else if enabled {
            &[CopyFocus::Field, CopyFocus::Cancel, CopyFocus::Confirm]
        } else {
            &[CopyFocus::Field, CopyFocus::Cancel]
        };
        *focus = collide::walk(order, *focus, key, CopyFocus::Field);
    }

    pub(in super::super) fn close_copy(&mut self) {
        if self.close_rclone() {
            return;
        }
        if let Some(Dialog::CopyTo { menu, .. }) = self.dialog.take() {
            self.outbox.push(menu_close(menu));
        }
    }

    // A full path goes to the menu's validate, whose reply names the snapshot; a relative one is refused on the card.
    pub(in super::super) fn confirm_copy(&mut self, catalog: &I18n) {
        if self.submit_rclone() {
            return;
        }
        let Some(Dialog::CopyTo {
            menu,
            moving,
            path,
            busy: false,
            ..
        }) = self.dialog.clone()
        else {
            return;
        };
        if path.is_empty() {
            return;
        }
        if !path.starts_with('/') && flea::plugin_runtime::protocol::place_path(&path).is_none() {
            if let Some(Dialog::CopyTo { error, .. }) = &mut self.dialog {
                *error = catalog.tr("jobs.copyto.relative");
            }
            return;
        }
        if let Some(Dialog::CopyTo {
            busy, focus, error, ..
        }) = &mut self.dialog
        {
            *busy = true;
            *focus = CopyFocus::Cancel;
            error.clear();
        }
        let action = if moving { "moveTo" } else { "copyTo" };
        self.outbox.push(format!("{{\"c\":\"menuaction\",\"op\":\"validate\",\"id\":{menu},\"action\":\"{action}\",\"dest\":{}}}", collide::text(&path)));
    }

    pub(super) fn validated(&mut self, catalog: &I18n, reply: &Reply) {
        let Some(Dialog::CopyTo {
            menu,
            moving,
            busy: true,
            ..
        }) = self.dialog.clone()
        else {
            return;
        };
        if reply.id != menu {
            return;
        }
        self.changed = true;
        if !reply.ok {
            if let Some(Dialog::CopyTo { busy, error, .. }) = &mut self.dialog {
                *busy = false;
                *error = if reply.error.is_empty() {
                    catalog.tr("jobs.delete.failed")
                } else {
                    reply.error.clone()
                };
            }
            return;
        }
        self.dialog = None;
        let mut request = Request::new()
            .with("c", &collide::text("transfer"))
            .with("op", &collide::text(if moving { "move" } else { "copy" }))
            .with("menuId", &menu.to_string())
            .with("dest", &collide::text(&reply.dest));
        if reply.dest.starts_with("plugin://")
            || reply.paths.iter().any(|path| path.starts_with("plugin://"))
        {
            let paths: Vec<_> = reply.paths.iter().map(String::as_str).collect();
            // The validated selection survives closing its source menu and routing to a different owner.
            request = Request::new()
                .with("c", &collide::text("transfer"))
                .with("op", &collide::text(if moving { "move" } else { "copy" }))
                .with("paths", &collide::texts(&paths))
                .with("dest", &collide::text(&reply.dest));
        }
        self.ask(catalog, &request, &[], false);
        self.outbox.push(menu_close(menu));
    }
}
