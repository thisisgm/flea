//! Quick Look's keys: the context the open card takes, and each action it runs (PreviewKeys.act).
use super::{Body, Subject};
use crate::{
    state::Shell,
    window::{dispatch::Order, input::ActionEvent},
};
use flea::{
    i18n::I18n,
    jsondoc::Json,
    keys::Context as Keys,
    ui_logic_pv1::{keys::Command, kinds::State, meta::Meta},
};
use gpui::{Context, KeyDownEvent, Window};
use std::path::PathBuf;

/// Focus.js lookup: the open card's kind picks the pdf, media or preview context.
pub(super) fn context(subject: &Subject) -> Keys {
    match subject.state {
        State::Pdf => Keys::Pdf,
        State::Video | State::Audio => Keys::Media,
        _ => Keys::Preview,
    }
}

// Sample input: {"t":"code","token":7,"slot":"quicklook","state":"styled","first":1,"total":2,"lines":[[5,"fn",0," main"],[]]}
/// A code reply's lines as plain text, each line's runs joined without their roles.
pub(super) fn plain_lines(doc: &Json) -> impl Iterator<Item = String> + '_ {
    doc.get("lines")
        .and_then(Json::as_array)
        .unwrap_or(&[])
        .iter()
        .map(|runs| {
            runs.as_array()
                .unwrap_or(&[])
                .iter()
                .filter_map(Json::as_str)
                .collect()
        })
}

/// The picture body a meta reply names: a RAW's embedded picture, a link's target, or the image itself upright.
pub(super) fn picture(meta: &Meta, path: String, link: Option<&str>, catalog: &I18n) -> Body {
    if let Some(raw) = meta.raw.as_ref().filter(|raw| raw.w > 0 && raw.h > 0) {
        let (w, h) = upright(raw.w, raw.h, raw.orient);
        let caption = catalog.tr_args(
            "preview.embedded",
            &[&raw.w.to_string(), &raw.h.to_string()],
        );
        return Body::Raw {
            path: PathBuf::from(&raw.file),
            w,
            h,
            caption,
        };
    }
    if meta.w == 0 || meta.h == 0 {
        return Body::cannot_show(catalog, State::Image, false, true, Vec::new())
            .expect("a failed read has a sentence");
    }
    let (w, h) = upright(meta.w, meta.h, meta.orient);
    match link {
        Some(target) => Body::Link {
            path: path.into(),
            w,
            h,
            target: catalog.tr_args("preview.link_target", &[target]),
        },
        None => Body::Image {
            path: path.into(),
            w,
            h,
        },
    }
}

// EXIF orientations 5 to 8 turn the picture a quarter, so its upright width is the stored height.
fn upright(w: u32, h: u32, orient: u32) -> (f64, f64) {
    if orient >= 5 {
        (h.into(), w.into())
    } else {
        (w.into(), h.into())
    }
}

impl Shell {
    /// Every key while the card is open goes to the card alone, so no family behind it runs; unbound keys are kept.
    pub(crate) fn quick_look_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.quick_look_open(cx) {
            return false;
        }
        let Some(pane) = self.panes[self.focus_side].as_ref() else {
            return true;
        };
        let seat = flea::keys::sequence::Seat {
            listing: pane.pane.held.listing as u64,
            cursor: pane.pane.cursor,
            view: self.view as u64,
            focus: self.focus_side as u64,
            ..Default::default()
        };
        let name = self
            .launch
            .settings
            .data
            .get("keys")
            .and_then(Json::as_str)
            .unwrap_or("default");
        // A tab switch the card does not bind closes it, and the key then runs where it was bound.
        let held = window.modifiers();
        let card = crate::window::input::event_lookup(name, self.keyboard.context, event, held);
        let listing = crate::window::input::event_lookup(name, Keys::Listing, event, held);
        // [ and ] bind only while a workbook of two or more sheets shows; any other card leaves them unbound.
        let sheet_key = matches!(
            card,
            flea::keys::Match::Action("previousSheet" | "nextSheet")
        );
        let card_bound = matches!(card, flea::keys::Match::Action(_))
            && (!sheet_key || self.quick_look_turns_sheet(cx));
        if let (false, flea::keys::Match::Action(action)) = (card_bound, listing) {
            if action == "openTab" || action.starts_with("tab") {
                self.quick_look_tab_switch(window, cx);
                return false;
            }
        }
        let pair_armed = self.keyboard.pair_armed(name, seat);
        let resolved = self.keyboard.resolve(name, event, window.modifiers(), seat);
        if let crate::window::input::Dispatch::Action(action) = resolved {
            let context = self.keyboard.context;
            let event = ActionEvent {
                action,
                context,
                seat,
                pair_armed,
            };
            self.quick_look_action(&event, window, cx);
        }
        true
    }

    /// PreviewKeys.act on the open card; false while no card is open.
    pub(crate) fn quick_look_action(
        &mut self,
        event: &ActionEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self
            .quick_look
            .as_ref()
            .map(|host| host.view.clone())
            .filter(|v| v.read(cx).is_open())
        else {
            return false;
        };
        let command = view.update(cx, |view, cx| view.action(event.action, cx));
        self.quick_look_drain();
        match command {
            Command::Close => self.quick_look_closed(window, cx),
            // PreviewKeys.act cursorDown: the cursor moves under the card and the card follows a file row.
            Command::Follow(step) => {
                let side = self
                    .quick_look
                    .as_ref()
                    .map_or(self.focus_side, |host| host.side);
                self.quick_look_follow(side, step, window, cx);
            }
            _ => {}
        }
        cx.notify();
        true
    }

    /// True while the card shows a workbook with more than one sheet, so [ and ] turn sheets.
    fn quick_look_turns_sheet(&self, cx: &gpui::App) -> bool {
        self.quick_look
            .as_ref()
            .is_some_and(|host| host.view.read(cx).turns_sheet(cx))
    }

    /// QuickLook::close on a tab switch: the card closes and the keyboard goes back to the listing.
    pub(crate) fn quick_look_tab_switch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.quick_look.as_ref().map(|host| host.view.clone()) else {
            return;
        };
        view.update(cx, |view, cx| view.close(cx));
        self.quick_look_closed(window, cx);
    }

    fn quick_look_follow(
        &mut self,
        side: usize,
        step: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let action = if step > 0 { "cursorDown" } else { "cursorUp" };
        let held = self.keyboard.context;
        let focus = self.focus_side;
        self.focus_side = side;
        self.keyboard.context = Keys::Listing;
        // The listing's own move, so a filter, the wrap setting and the selection follow it as they do with no card.
        if self.filter_claims(action, cx) {
            self.dispatch(
                Order::Action(ActionEvent::bare(action, Keys::Listing)),
                window,
                cx,
            );
        } else {
            self.input_action(action, window, cx);
        }
        self.keyboard.context = held;
        self.focus_side = focus;
        // PreviewKeys.follow: a folder row leaves the card on the file it shows.
        let file = self.panes[side]
            .as_ref()
            .and_then(|pane| pane.pane.held.row(pane.pane.cursor))
            .is_some_and(|row| !row.dir);
        let moved = self
            .quick_look
            .as_ref()
            .is_some_and(|host| self.panes[side].as_ref().map(|pane| pane.pane.cursor) != host.row);
        if file && moved {
            self.open_quick_look(side, window, cx);
        }
        cx.notify();
    }
}
