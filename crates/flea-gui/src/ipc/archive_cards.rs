//! Archive and flash readers over the window's live operation owner.
use super::Adapter;
use crate::state::Shell;
use flea::{ipc::wire::Reply, json::escape};

pub(crate) struct ArchiveAdapter<'a> {
    pub shell: &'a Shell,
}

impl Adapter for ArchiveAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        let archive = &self.shell.operations.archive;
        let value = match function {
            "archiveFormats" => archive.formats[self.shell.focus_side].archive.join("|"),
            "archiveCardsState" => {
                if !archive.card_open() {
                    return Some(Reply::value("{\"opened\":false}"));
                }
                // v0.3.12's shape; the eye centre is where the open card's password eye last painted.
                let eye = archive.eye.get().map_or(String::new(), |b| {
                    let (x, y) = (f32::from(b.origin.x), f32::from(b.origin.y));
                    super::rows::centre_of(x, y, f32::from(b.size.width), f32::from(b.size.height))
                });
                let compress = archive.compress.as_ref().map_or_else(
                    || "\"compressOpened\":false,\"compressCount\":0,\"compressFormat\":\"\",\"compressError\":\"\",\"compressFocus\":-1,\"canCompress\":false,\"compressCapture\":null".to_string(),
                    |card| card.reading(if card.opened() { &eye } else { "" }),
                );
                let extract =
                    archive
                        .extract
                        .reading(if archive.extract.opened() { &eye } else { "" });
                format!("{{\"opened\":true,{compress},{extract}}}")
            }
            "flashState" => {
                let flash = &archive.flash;
                if !flash.shown() && !flash.running() && flash.image_path().is_none() {
                    return Some(Reply::error(3, "FLEA_IPC_ERROR flea.flashState unavailable field: view lifetime (an unbuilt host and an empty closed view share the same model state)"));
                }
                format!(
                    "{{\"opened\":{},\"cursor\":{},\"image\":\"{}\",\"drives\":{},\"selected\":{},\"live\":{},\"running\":{},\"phase\":\"{}\",\"ending\":\"{}\"}}",
                    flash.shown(),
                    flash.cursor_index(),
                    escape(flash.image_path().unwrap_or("")),
                    flash.drive_count(),
                    flash.selected().map_or(-1, |at| at as i64),
                    flash.live(),
                    flash.running(),
                    escape(flash.phase()),
                    escape(flash.ending().map_or("", |ending| ending.headline.as_str())),
                )
            }
            _ => return None,
        };
        Some(Reply::value(value))
    }
}
