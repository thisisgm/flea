//! The listing and Save entities draw through the G1.06 Element.
use super::listing::{Cause, Listing};
use super::save::SaveField;
use crate::list::header::HeaderElement;
use crate::list::{element::ROW_HEIGHT, ListElement, ListState, Typeface};
use flea::i18n::I18n;
use gpui::{div, prelude::*, Context, IntoElement, Render, Window};
use std::{cell::RefCell, rc::Rc};

pub struct ListingView {
    listing: Listing,
    i18n: I18n,
    state: Rc<RefCell<ListState>>,
    scroll_y: f32,
}

impl ListingView {
    pub fn new(i18n: I18n, typeface: Typeface) -> Self {
        Self {
            listing: Listing::default(),
            i18n,
            state: Rc::new(RefCell::new(ListState::new(typeface))),
            scroll_y: 0.,
        }
    }

    pub fn open(&mut self, path: &str) {
        self.listing.open(path);
    }

    pub fn generation(&self) -> u64 {
        self.listing.generation()
    }

    pub fn receive(&mut self, generation: u64, line: &str) -> bool {
        self.listing.receive(generation, line)
    }

    pub fn fail(&mut self, generation: u64, cause: Cause) -> bool {
        self.listing.fail(generation, cause)
    }

    pub fn facts(&self) -> super::listing::Facts {
        self.listing.facts()
    }

    pub fn listing(&self) -> &Listing {
        &self.listing
    }

    pub fn listing_mut(&mut self) -> &mut Listing {
        &mut self.listing
    }

    // Scrolls the least distance that keeps the cursor row inside the last drawn box.
    pub fn reveal(&mut self) {
        let height = f32::from(self.state.borrow().drawn.bounds.size.height);
        let top = self.listing.cursor() as f32 * ROW_HEIGHT;
        if top < self.scroll_y {
            self.scroll_y = top;
        } else if height > 0. && top + ROW_HEIGHT > self.scroll_y + height {
            self.scroll_y = top + ROW_HEIGHT - height;
        }
    }

    pub(crate) fn drawn(&self) -> std::cell::Ref<'_, ListState> {
        self.state.borrow()
    }

    pub fn scroll_y(&self) -> f32 {
        self.scroll_y
    }

    // Test hooks into what the frame drew.
    #[cfg(test)]
    pub fn shaped_rows(&self) -> u64 {
        self.state.borrow().rows.misses
    }

    #[cfg(test)]
    pub fn painted_cells(&self) -> usize {
        self.state.borrow().painted_cells.get()
    }

    #[cfg(test)]
    pub fn header_frames(&self) -> u64 {
        self.state.borrow().header.drawn.frames
    }
}

impl Render for ListingView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let failure = self.listing.failure_line(&self.i18n);
        let body = ListElement::new(
            self.listing.held(),
            self.scroll_y,
            self.listing.cursor(),
            self.state.clone(),
        );
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .debug_selector(|| "pk2-header".into())
                    .child(HeaderElement::new(self.state.clone())),
            )
            .child(
                div()
                    .flex_1()
                    .debug_selector(|| "pk2-listing".into())
                    .child(body),
            )
            .when(!failure.is_empty(), |view| {
                view.child(div().debug_selector(|| "pk2-failure".into()).child(failure))
            })
    }
}

// The Save line: words from SaveField; the view draws.
pub struct SaveView {
    field: SaveField,
    i18n: I18n,
}

impl SaveView {
    pub fn new(i18n: I18n, field: SaveField) -> Self {
        Self { field, i18n }
    }

    pub fn field(&mut self) -> &mut SaveField {
        &mut self.field
    }

    pub fn field_ref(&self) -> &SaveField {
        &self.field
    }
}

impl Render for SaveView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let words = self.field.words(&self.i18n);
        let name = self.field.asked_name().to_owned();
        div()
            .debug_selector(|| "pk2-save-field".into())
            .child(name)
            .when(!words.error.is_empty(), |view| {
                view.child(
                    div()
                        .debug_selector(|| "pk2-save-error".into())
                        .child(words.error),
                )
            })
    }
}
