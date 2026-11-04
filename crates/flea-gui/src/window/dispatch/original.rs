//! Show Original resolves the captured link through the backend and reveals only its matching reply.
use super::Order;
use crate::{state::Shell, window::navigate::Step};
use flea::{json::escape, jsondoc::Json};
use gpui::{Context, Window};

#[derive(Default)]
pub(super) struct Pending {
    sequence: usize,
    request: Option<Request>,
}

struct Request {
    id: usize,
    link: String,
    place: String,
    listing: usize,
}

pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    let Order::Menu(chosen) = order else {
        return false;
    };
    if chosen.action != "showOriginal" || chosen.paths.is_empty() || !chosen.claim() {
        return false;
    }
    let side = chosen.captured.side;
    let link = chosen.paths[0].clone();
    shell.retire_menu(window, cx);
    request(shell, side, link, cx);
    true
}

// The backend answers with the target; original_reply reveals it only while this pane still shows the same listing.
pub(super) fn request(shell: &mut Shell, side: usize, link: String, cx: &mut Context<Shell>) {
    let Some(host) = shell.panes[side]
        .as_ref()
        .filter(|host| !host.stopped && !host.pane.busy())
    else {
        return;
    };
    let pending = &mut shell.opener[side].original;
    pending.sequence = pending.sequence.wrapping_add(1).max(1);
    let id = pending.sequence;
    let line = format!(
        r#"{{"c":"linktarget","path":"{}","id":{id}}}"#,
        escape(&link)
    );
    pending.request = Some(Request {
        id,
        link,
        place: host.pane.place().to_owned(),
        listing: host.pane.held.listing,
    });
    shell.menu_send(side, line, cx);
}

impl Shell {
    pub(crate) fn original_reply(
        &mut self,
        side: usize,
        doc: &Json,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = |key| doc.get(key).and_then(Json::as_str);
        let id = match doc.get("id") {
            Some(Json::Num(number)) => number.parse::<usize>().ok(),
            _ => None,
        };
        let pending = &mut self.opener[side].original;
        let Some(request) = pending.request.as_ref().filter(|request| {
            Some(request.id) == id && text("path") == Some(request.link.as_str())
        }) else {
            return;
        };
        let live = self.panes[side].as_ref().is_some_and(|host| {
            !host.stopped
                && !host.pane.busy()
                && host.pane.place() == request.place
                && host.pane.held.listing == request.listing
        });
        pending.request = None;
        if !live {
            return;
        }
        let (Some(directory), Some(name)) = (text("directory"), text("name")) else {
            return;
        };
        if directory.is_empty() || (name.is_empty() && directory != "/") {
            let said = self.locale.catalog.tr("dialogs.file.original.missing");
            self.opener_say(side, said, true, cx);
            return;
        }
        if !directory.starts_with('/') || name.contains('/') || name == "." || name == ".." {
            return;
        }
        if let Some(host) = self.panes[side].as_mut() {
            host.nav.pending_select =
                (!name.is_empty()).then(|| crate::navigation::history::join(directory, name));
        }
        let _ = self.step(side, Step::Open(directory.to_owned()), window, cx);
    }
}
