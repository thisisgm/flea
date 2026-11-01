//! Archive and flash readers over the window's live operation owner.
use super::Adapter;
use crate::state::Shell;
use flea::{ipc::wire::Reply, json::escape};

pub(crate) struct ArchiveAdapter<'a> {
    pub shell: &'a Shell,
}

impl Adapter for ArchiveAdapter<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
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
                // FlashHost.describe: an unbuilt host answers closed alone, a built view its whole state, closed or not.
                if !flash.built() {
                    return Some(Reply::value("{\"opened\":false}"));
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
            // FlashHost.controlItem: flash, cancel or end, centred where the step row painted it, "" while another or none shows.
            "flashControlCentre" => {
                use crate::g2::op2::flash::StepAction;
                let wanted = match arguments.first().map(String::as_str) {
                    Some("flash") => Some(StepAction::Flash),
                    Some("cancel") => Some(StepAction::Cancel),
                    Some("end") => Some(StepAction::End),
                    _ => None,
                };
                archive
                    .flash_action
                    .get()
                    .filter(|(action, _)| archive.flash.shown() && Some(*action) == wanted)
                    .map_or_else(String::new, |(_, b)| {
                        super::rows::centre_of(
                            f32::from(b.origin.x),
                            f32::from(b.origin.y),
                            f32::from(b.size.width),
                            f32::from(b.size.height),
                        )
                    })
            }
            _ => return None,
        };
        Some(Reply::value(value))
    }
}
