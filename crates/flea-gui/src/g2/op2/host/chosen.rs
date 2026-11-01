//! Menu choices the archive host claims: compress, a password card, extract and flash.
use super::*;

impl Shell {
    /// A captured menu choice this host owns; true once it claimed the snapshot.
    pub(crate) fn archive_chosen(
        &mut self,
        chosen: &Chosen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let action = chosen.action.as_str();
        if !matches!(
            action,
            "compress" | "compressPassword" | "extract" | "flashDrive"
        ) {
            return false;
        }
        let side = chosen.captured.side;
        if !chosen.captured.ready || chosen.captured.background || chosen.paths.is_empty() {
            return false;
        }
        let Some(host) = self.panes.get(side).and_then(Option::as_ref) else {
            return false;
        };
        if host.backend.is_none() || !chosen.claim() {
            return false;
        }
        let base = host.path.clone();
        let menu = chosen.captured.id;
        let catalog = self.locale.catalog.clone();
        let password = action == "compressPassword" || chosen.leaf == "compressPassword";
        if action == "compress" && !password {
            let paths: Vec<&str> = chosen.paths.iter().map(String::as_str).collect();
            let leaves = logic::path_leaves(&paths);
            let stem = archive::archive_stem(
                &leaves.iter().map(String::as_str).collect::<Vec<_>>(),
                &logic::leaf_of(&base),
            );
            let format = chosen.leaf.as_str();
            let line = wire::compress_request(
                &paths,
                &join(&base, &format!("{stem}.{format}")),
                format,
                menu,
            );
            let sticky = logic::compress_sticky(&catalog, paths.len(), format);
            self.archive_effects(
                side,
                vec![
                    Effect::Send(line),
                    Effect::Say {
                        text: sticky,
                        error: false,
                    },
                ],
                window,
                cx,
            );
            self.archive_menu_close(side, menu, cx);
        } else if password {
            if let Some(busy) =
                super::super::card_busy(self.operations.archive.card_open(), false, &catalog)
            {
                self.archive_effects(side, vec![busy], window, cx);
                self.archive_menu_close(side, menu, cx);
                return true;
            }
            let reported: Vec<&str> = self.operations.archive.formats[side]
                .archive
                .iter()
                .map(String::as_str)
                .collect();
            let (card, effects) = Compress::open(
                chosen.paths.clone(),
                &base,
                &logic::leaf_of(&base),
                menu,
                &reported,
            );
            // A new card reads no eye until it paints, never the last card's.
            self.operations.archive.eye.set(None);
            self.operations.archive.confirm_eye.set(None);
            self.operations.archive.compress = Some(card);
            self.operations.archive.card_side = side;
            self.operations.archive.card_menu = Some(menu);
            self.archive_effects(side, effects, window, cx);
        } else if action == "extract" {
            let path = chosen.paths[0].clone();
            let dest = join(parent(&path), &archive::extract_dir(&logic::leaf_of(&path)));
            self.operations
                .archive
                .extracts
                .retain(|(s, p, d)| !(*s == side && *p == path && *d == dest));
            self.operations
                .archive
                .extracts
                .push((side, path.clone(), dest.clone()));
            self.archive_effects(
                side,
                vec![Effect::Send(wire::extract_request(&path, &dest, menu))],
                window,
                cx,
            );
            self.archive_menu_close(side, menu, cx);
        } else {
            let path = chosen.paths[0].clone();
            self.archive_menu_close(side, menu, cx);
            if self.operations.archive.flash.running() && self.operations.archive.flash_side != side
            {
                let busy = flea::ui_logic_op2::flash::already_running(&catalog);
                self.archive_effects(
                    side,
                    vec![Effect::Say {
                        text: busy,
                        error: true,
                    }],
                    window,
                    cx,
                );
                return true;
            }
            self.operations.archive.flash_side = side;
            let effects = self.operations.archive.flash.open_with_image(&path);
            self.archive_effects(side, effects, window, cx);
        }
        cx.notify();
        true
    }
}
