//! The footer count follows the surface covering the focused folder; its disk facts keep their existing owner.
use crate::{
    pane::states::Kind,
    state::Shell,
    strips::status_host::{self, Listing},
    window::pane::PaneHost,
};
use flea::i18n::I18n;
use gpui::App;

pub(super) struct Snapshot {
    pub kind: Kind,
    pub total: usize,
    pub selected: usize,
    pub text: String,
}

pub(super) fn trash(shell: &Shell, cx: &App) -> Option<Snapshot> {
    let host = shell
        .trash
        .as_ref()
        .filter(|host| host.side == shell.focus_side && shell.slots.covers[host.side].is_some())?;
    let pane = host.pane.read(cx);
    if !pane.view().opened {
        return None;
    }
    let total = pane.paint.borrow().total;
    let selected = if pane.presentation_loading {
        0
    } else {
        pane.view().selected_count()
    };
    Some(Snapshot {
        kind: Kind::Ready,
        total,
        selected,
        text: status_host::left(
            &shell.locale.catalog,
            Listing::Ready(total),
            "",
            Some((selected, -1, String::new())),
            None,
        ),
    })
}

/// WindowBody.qml flashCount: the flash pane counts its drives, 2 drives, and selects nothing.
pub(super) fn flash(shell: &Shell) -> Option<Snapshot> {
    if !shell.flash_covers(shell.focus_side) {
        return None;
    }
    let flash = &shell.operations.archive.flash;
    Some(Snapshot {
        kind: Kind::Ready,
        total: flash.drive_count(),
        selected: 0,
        text: flash.drives_line(&shell.locale.catalog),
    })
}

pub(super) fn folder(host: &PaneHost, catalog: &I18n) -> Snapshot {
    let kind = host.pane.state.kind;
    let held = &host.pane.held;
    let total = held.total;
    let selected = host.selected.len();
    let bytes = if selected <= held.rows.len() {
        // A folder counts by its finished walk, as ui/js/Status.js selectionBytes reads DirSizes.
        let mut sizes = Vec::with_capacity(selected);
        let mut walked = Vec::with_capacity(selected);
        let mut dirs = Vec::with_capacity(selected);
        for &index in &host.selected {
            let row = index
                .checked_sub(held.start)
                .and_then(|at| held.rows.get(at));
            let dir = row.is_some_and(|row| row.dir);
            let walk = dir
                .then(|| host.sizes.walked(held.listing, index))
                .flatten();
            sizes.push(if dir { walk } else { row.map(|row| row.size) });
            walked.push(walk.is_some());
            dirs.push(dir);
        }
        status_host::selection_bytes(selected, held.rows.len(), &sizes, &walked, &dirs)
    } else {
        -1
    };
    let listing = match kind {
        Kind::Ready => Listing::Ready(total),
        Kind::Empty => Listing::Empty,
        Kind::Locked => Listing::Locked,
        Kind::Error => Listing::Error,
        Kind::Loading | Kind::Waiting => Listing::Loading,
    };
    // HIG filter.png and grid-filter-matches.png: a standing filter's count, 1 of 10 shown, takes the left.
    let filtered = host
        .filter
        .owner
        .up()
        .then(|| host.filter.owner.counts(held.listing))
        .flatten()
        .map(|counts| crate::g2::listing_filter::present::status(&counts, catalog));
    Snapshot {
        kind,
        total,
        selected,
        text: status_host::left(
            catalog,
            listing,
            "",
            Some((
                selected,
                bytes,
                catalog.locale().format_size(bytes.max(0) as u64),
            )),
            filtered.as_deref(),
        ),
    }
}

/// StatusBar.qml's ElideRight keeps the space before the ellipsis, which GPUI's own truncation trims.
pub(super) fn elided(
    text: &gpui::SharedString,
    room: f32,
    measure: impl Fn(&str) -> f32,
) -> gpui::StyledText {
    if measure(text) <= room {
        return gpui::StyledText::new(text.clone());
    }
    // ends[k] is where the first k + 1 characters end.
    let ends: Vec<usize> = text.char_indices().skip(1).map(|(at, _)| at).collect();
    let (mut low, mut high) = (0, ends.len());
    while low < high {
        let mid = (low + high).div_ceil(2);
        if measure(&format!("{}\u{2026}", &text[..ends[mid - 1]])) <= room {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    let head = if low == 0 { "" } else { &text[..ends[low - 1]] };
    gpui::StyledText::new(format!("{head}\u{2026}"))
}
