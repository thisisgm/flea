//! Listing and thumbnail answers belong to the picker listing, never the checks child.
use super::Picker;
use flea::{
    jsondoc::{self, Json},
    ui_logic_picker_portal as logic,
};
use gpui::Context;

impl Picker {
    // One whole line from the listing child.
    pub(super) fn listing_line(&mut self, text: &str, cx: &mut Context<Self>) {
        self.browse.replied = true;
        // A line that names no answer cannot be stale, so it fails the folder the window stands on.
        let Ok(document) = jsondoc::parse(text) else {
            self.browse.listing.update(cx, |view, _| {
                let generation = view.generation();
                view.receive(generation, text)
            });
            return self.sync_facts(cx);
        };
        if self
            .browse
            .listing
            .update(cx, |view, cx| view.picture_line(&document, cx))
        {
            return;
        }
        let request_generation =
            flea::json::field_usize(text, "pickerGen").map(|value| value as u64);
        if let Some(generation) = request_generation {
            if generation != self.browse.listing.read(cx).generation() {
                return;
            }
        }
        let kind = document
            .get("t")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_owned();
        match kind.as_str() {
            "changed" => {
                if document.get("path").and_then(Json::as_str)
                    == Some(self.listing.location.as_str())
                    && !self.accept_state().submitting()
                    && !self.answered()
                {
                    let path = self.listing.location.clone();
                    self.open_folder(&path, cx);
                }
                return;
            }
            "jumped" => return self.jump_answer(&document, cx),
            "paths" => return self.selection_paths(&document, cx),
            "filtered" => return self.find_line(text, cx),
            "peeked" => return self.peeked(&document, cx),
            "located" => return self.located(&document, cx),
            "listed" => self.landing(&document, request_generation, cx),
            "error" if document.get("where").and_then(Json::as_str) == Some("scan") => {
                self.landing(&document, request_generation, cx)
            }
            _ => {}
        }
        let generation = self.browse.reply_gen;
        let taken = self
            .browse
            .listing
            .update(cx, |view, _| view.receive(generation, text));
        self.sync_facts(cx);
        if taken && kind == "listed" {
            self.navigation_landed();
            self.relocate();
        }
        if taken {
            if let Some(stamp) = (kind == "rows")
                .then(|| flea::json::field_usize(text, "listing"))
                .flatten()
            {
                self.find_stamp(stamp, cx);
            }
            self.take_leaf(cx);
            if kind == "rows" {
                self.refill_grid(cx);
            }
        }
    }

    // A listed or scan answer closes one request; it is current only when it answers the newest for the drawn folder.
    fn landing(&mut self, document: &Json, generation: Option<u64>, cx: &mut Context<Self>) {
        self.browse.in_flight = self.browse.in_flight.saturating_sub(1);
        let view = self.browse.listing.read(cx);
        // Recent's listing stands on the base "/" its paths are named under.
        let listed = if logic::is_recent(&self.listing.location) {
            "/"
        } else {
            self.listing.location.as_str()
        };
        let current = view.listing().is_pending()
            && (generation == Some(view.generation())
                || (generation.is_none() && self.browse.in_flight == 0))
            && document.get("path").and_then(Json::as_str) == Some(listed);
        self.browse.reply_gen = if current { view.generation() } else { 0 };
        if current && generation.is_some() {
            self.browse.in_flight = 0;
        }
    }

    // The listing child left: rows already drawn stay, and the footer says which failure it was.
    pub(super) fn listing_failed(&mut self, cx: &mut Context<Self>) {
        let cause = if self.browse.replied {
            crate::g2::picker_listing::Cause::Gone
        } else {
            crate::g2::picker_listing::Cause::Unstarted
        };
        self.browse.listing.update(cx, |view, _| {
            let generation = view.generation();
            view.fail(generation, cause)
        });
        self.sync_facts(cx);
    }
}
