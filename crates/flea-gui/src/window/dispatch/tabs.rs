//! The tabs family: the strip keys, Ctrl+Return and a middle click on a folder row or a rail place, through the strip's model.
use super::Order;
use crate::{g2::rail_places::producer::Event, state::Shell};
use flea::{keys::Context as Keys, ui_logic_rail_places::rail::Kind};
use gpui::{Context, Window};

pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    match order {
        // Focus.js windowAction: the strip's own keys (tab1..tab9, moves, new, close, next, previous) answer from the rail too.
        Order::Action(event)
            if event.context == Keys::Listing
                || (event.context == Keys::Rail && event.action.starts_with("tab")) =>
        {
            // A middle click names its own row and pane and leaves the cursor where it was (Tap.js).
            if event.action == "openTab" && event.context == Keys::Listing {
                if let Some(target) = pointed_folder(shell, event) {
                    shell.open_tab(event.seat.focus as usize, target, window, cx);
                    return true;
                }
            }
            shell.tab_action(event.action, window, cx)
        }
        Order::Place(request) => {
            let Event::Tab(entry) = &request.event else {
                return false;
            };
            place_tab(shell, request.side, entry, window, cx);
            true
        }
        _ => false,
    }
}

// The folder a pointer's openTab names: its seat row on its own pane, when that is not the cursor a key would use.
fn pointed_folder(shell: &Shell, event: &crate::window::input::ActionEvent) -> Option<String> {
    let side = event.seat.focus as usize;
    let host = shell.panes.get(side)?.as_ref()?;
    if event.seat.listing != host.pane.held.listing as u64 || event.seat.cursor == host.pane.cursor
    {
        return None;
    }
    let row = host
        .pane
        .held
        .row(event.seat.cursor)
        .filter(|row| row.dir)?;
    Some(crate::navigation::history::join(
        host.pane.path(),
        &row.name,
    ))
}

// PlaceMenu.js openTabAt: only Home and a favourite open in a new tab, with Places.openEntry's record check and remote refusal.
fn place_tab(
    shell: &mut Shell,
    side: usize,
    entry: &flea::ui_logic_rail_places::rail::RailEntry,
    window: &mut Window,
    cx: &mut Context<Shell>,
) {
    if !matches!(entry.kind, Kind::Home | Kind::Favourite | Kind::Favorite) {
        return;
    }
    let catalog = shell.locale.catalog.clone();
    let label = crate::g2::rail_places::content::row_label(&catalog, entry).to_string();
    let (message, error) = if let Some(error) = &entry.error {
        let error = match error {
            flea::ui_logic_rail_places::rail::FavouriteError::Record(record) => {
                catalog.tr(record.key())
            }
            flea::ui_logic_rail_places::rail::FavouriteError::Status(line) => line.clone(),
        };
        let text = catalog.tr_args("places.favorite.open_failed", &[&label, &error]);
        (text, true)
    } else if entry.path.contains("://") && !entry.path.starts_with("file://") {
        (
            catalog.tr_args("places.favorite.opens_here", &[&label]),
            false,
        )
    } else if let Some(local) = entry.path.strip_prefix("file://") {
        let path = flea::ui_logic_device_rail::mounts::decode_path(local);
        shell.open_tab(side, path, window, cx);
        return;
    } else if entry.path.is_empty() {
        return;
    } else {
        shell.open_tab(side, entry.path.clone(), window, cx);
        return;
    };
    // Places.js openEntry: a bad record is an error, a remote place a notice.
    shell.status_messages(cx, |messages, now| {
        messages.say(
            &message,
            error,
            "",
            flea::ui_logic_status_feedback::status::Hint::None,
            now,
        )
    });
    cx.notify();
}
