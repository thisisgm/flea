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
                return Some(Reply::error(3, "FLEA_IPC_ERROR flea.archiveCardsState unavailable fields: retained password-card lifetime, compressCapture.eye, extractCapture.eye (closing Compress discards its model and neither card records painted eye bounds)"));
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
