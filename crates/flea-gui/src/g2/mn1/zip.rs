//! The Flash to drive zip probe, as v0.3.12 Ops.js checkZip and noteZip run it: one flashimage ask per target zip.
use crate::{navigation::history::join, state::Shell, window::pane::PaneHost};
use flea::{
    jsondoc::Json,
    protocol::RowFacts,
    ui_logic_mn1::rows,
    ui_logic_op2::wire::{flash_image_request, parse_flashimage},
};

/// One side's asked zip and the image member its answer named.
#[derive(Default)]
pub(crate) struct ZipProbe {
    // The asked zip's folder, name, size and mtime, so a zip rewritten in place asks again.
    asked: Option<(String, String, u64, i64)>,
    path: String,
    member: String,
}

impl ZipProbe {
    /// The member Flash to drive reads for the row at this path; empty unless the answer was for that path.
    pub(crate) fn member_for(&self, path: &str) -> &str {
        if self.path == path {
            &self.member
        } else {
            ""
        }
    }
}

// Ops.js zipTarget: the one marked row, else the cursor row, when it is a zip file.
fn target(host: &PaneHost) -> Option<(&str, &RowFacts)> {
    let index = match host.selected.as_slice() {
        [] if host.pane.held.total > 0 => host.pane.cursor,
        [one] => *one,
        _ => return None,
    };
    let row = host.pane.held.row(index).filter(|row| !row.dir)?;
    // rows::image_kind names a zip by this suffix alone, compared here without the lowercase copy it makes.
    let zip = row
        .name
        .rsplit_once('.')
        .is_some_and(|(_, suffix)| suffix.eq_ignore_ascii_case("zip"));
    zip.then(|| (host.search.owner.session.base_of(&host.path), row))
}

impl Shell {
    /// Ops.js checkZip: a new target zip asks once, and any change of target forgets the last member at once.
    pub(crate) fn check_menu_zip(&mut self, side: usize) {
        let Some(host) = self.panes[side].as_ref().filter(|host| !host.pane.busy()) else {
            return;
        };
        let Some(backend) = &host.backend else {
            return;
        };
        let found = target(host);
        let probe = &mut self.menu.zip[side];
        let same = match (&probe.asked, found) {
            (None, None) => true,
            (Some((base, name, size, mtime)), Some((at, row))) => {
                base == at && *name == row.name && *size == row.size && *mtime == row.mtime
            }
            _ => false,
        };
        if same {
            return;
        }
        probe.member.clear();
        probe.asked =
            found.map(|(base, row)| (base.to_owned(), row.name.clone(), row.size, row.mtime));
        probe.path = found.map_or_else(String::new, |(base, row)| join(base, &row.name));
        // An ask the backend never took is asked again on the next frame, never left without its Flash row.
        if !probe.path.is_empty() && backend.line(flash_image_request(&probe.path)).is_err() {
            probe.asked = None;
        }
    }

    /// Ops.js noteZip: only the answer for the current target lands, and only a lone image member arms Flash to drive.
    pub(crate) fn note_menu_zip(&mut self, side: usize, reply: &Json) {
        let probe = &mut self.menu.zip[side];
        let Some(image) = parse_flashimage(reply, &probe.path).filter(|_| !probe.path.is_empty())
        else {
            return;
        };
        let leaf = probe.path.rsplit('/').next().unwrap_or_default();
        probe.member = if image.kind == "zip" && rows::is_disk_image(leaf, &image.member) {
            image.member
        } else {
            String::new()
        };
        // A menu still waiting on its snapshot takes the answer; a drawn one keeps its rows, as noteZip never rebuilds one.
        let waiting = self.menu.captured.as_ref().is_some_and(|capture| {
            capture.side == side && !capture.ready && capture.target.is_none()
        });
        // A search row's name is relative to the search base, so the whole path names the row, as facts.rs reads it.
        let row = self.panes[side].as_ref().map(|host| {
            join(
                host.search.owner.session.base_of(&host.path),
                &self.menu.context.row_name,
            )
        });
        if waiting && row.as_deref() == Some(probe.path.as_str()) {
            self.menu.context.flash_zip_member.clone_from(&probe.member);
        }
    }
}
