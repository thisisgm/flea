//! A tap on a List column title or s and S sort the chooser as PickerHeader.qml and PickerList.qml do.
use super::Picker;
use crate::list::header::TAP_SLOP;
use crate::navigation::{folder_state::Order, sort::column_order};
use flea::ui_logic_picker_portal as logic;
use gpui::{Context, MouseDownEvent, MouseUpEvent, Window};

impl Picker {
    fn title_at(&self, position: gpui::Point<gpui::Pixels>, cx: &Context<Self>) -> &'static str {
        let view = self.browse.listing.read(cx);
        if view.view_mode() != "list" || logic::is_recent(&self.listing.location) {
            return "";
        }
        let state = view.drawn();
        let header = &state.header.drawn;
        if header.frames == 0 || !header.bounds.contains(&position) {
            return "";
        }
        let x = f32::from(position.x - header.bounds.left());
        crate::list::header::title_key(&header.layout, &state.drawn.metrics, x, false)
    }

    // A press on a title is remembered for its release; it never moves the cursor.
    pub(super) fn header_press(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) -> bool {
        let key = self.title_at(event.position, cx);
        self.header_tap = (!key.is_empty()).then_some((key, event.position));
        !key.is_empty()
    }

    pub(super) fn header_release(
        &mut self,
        event: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((key, at)) = self.header_tap.take() else {
            return;
        };
        let travel = f32::from(event.position.x - at.x).hypot(f32::from(event.position.y - at.y));
        if travel > TAP_SLOP
            || self.title_at(event.position, cx) != key
            || self.answered()
            || self.backend_unavailable()
            || self.accept_state().submitting()
            || self.listing.state != "ready"
        {
            return;
        }
        let current = self.browse.listing.read(cx).listing().order();
        if let Some(order) = column_order(current, key) {
            self.request_sort(order, cx);
        }
    }

    // PickerWindow.qml requestSort: Recent and an unchanged order ask nothing; a new order re-lists the folder.
    pub(super) fn request_sort(&mut self, order: Order, cx: &mut Context<Self>) {
        let current = self.browse.listing.read(cx).listing().order();
        if order == current
            || logic::is_recent(&self.listing.location)
            || self.answered()
            || self.backend_unavailable()
            || self.accept_state().submitting()
        {
            return;
        }
        let patch = self
            .browse
            .listing
            .update(cx, |view, _| view.listing_mut().choose_order(order));
        self.persist_sort(patch, cx);
        let path = self.listing.location.clone();
        self.open_folder(&path, cx);
    }
}
