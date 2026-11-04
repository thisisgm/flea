//! The listing and Save entities draw through the G1.06 Element.
use super::listing::{Cause, Listing};
use super::save::SaveField;
use crate::list::header::HeaderElement;
use crate::list::{Density, ListState, Typeface};
use flea::i18n::I18n;
use gpui::{div, prelude::*, AnyView, Context, IntoElement, Render, Window};
use std::{cell::RefCell, rc::Rc};

pub struct ListingView {
    listing: Listing,
    pub(super) surface: super::surface::Surface,
    pub(super) i18n: I18n,
    pub(super) state: Rc<RefCell<ListState>>,
    pub(super) scroll_y: f32,
    // The name filter's drawn rows, None while every row is drawn, and its strip under the column titles.
    pub(super) shown: Option<Rc<[usize]>>,
    strip: Option<AnyView>,
    pub(super) last_tap: String,
    pub(super) picture_policy: crate::grid::element::pictures::Policy,
    pub(super) bars: crate::g2::pv1::scroller::overlay::Overlay,
}

impl ListingView {
    pub fn apply_settings(&mut self, data: &flea::jsondoc::Json, cx: &mut Context<Self>) -> bool {
        let changed = self.listing.apply_settings(data);
        self.picture_policy = crate::grid::element::pictures::Policy::read(data);
        self.state.borrow_mut().view.order = self.listing.order();
        self.state.borrow_mut().view.set_lang_marks(
            data.get("langMarks").and_then(flea::jsondoc::Json::as_bool) != Some(false),
        );
        cx.notify();
        changed
    }

    #[cfg(test)]
    pub(crate) fn drawn_marks(&self) -> Vec<crate::row::marks::Mark> {
        self.state
            .borrow()
            .visible()
            .iter()
            .filter_map(|row| row.prepared.as_ref().map(|row| row.mark))
            .collect()
    }

    pub fn new(i18n: I18n, typeface: Typeface) -> Self {
        let mut state = ListState::new(typeface);
        state.density = Density::Normal;
        Self {
            listing: Listing::default(),
            surface: Default::default(),
            i18n,
            state: Rc::new(RefCell::new(state)),
            scroll_y: 0.,
            shown: None,
            strip: None,
            last_tap: String::new(),
            picture_policy: crate::grid::element::pictures::Policy::read(
                &flea::jsondoc::Json::Null,
            ),
            bars: Default::default(),
        }
    }

    /// The rows a standing name filter draws and its query, whose run each drawn name marks.
    pub fn set_filtered(
        &mut self,
        shown: Option<Rc<[usize]>>,
        query: &str,
        strip: Option<AnyView>,
    ) {
        self.state
            .borrow_mut()
            .view
            .set_query(if shown.is_some() { query } else { "" });
        self.shown = shown;
        self.strip = strip;
    }

    pub fn shown(&self) -> Option<&[usize]> {
        self.shown.as_deref()
    }

    // The cursor's position among the drawn rows.
    fn cursor_view(&self) -> usize {
        let cursor = self.listing.cursor();
        self.shown.as_ref().map_or(cursor, |rows| {
            rows.iter().position(|row| *row == cursor).unwrap_or(0)
        })
    }

    pub fn open(&mut self, path: &str) {
        self.set_scroll_y(0.);
        self.listing.open(path);
        self.state.borrow_mut().view.order = self.listing.order();
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
        if self.surface.mode == "grid" {
            let view = self.grid_view();
            let total = self
                .shown
                .as_ref()
                .map_or(self.listing.total(), |rows| rows.len());
            let layout = view.layout(crate::grid::Shown::All(total));
            self.set_scroll_y(layout.reveal(self.scroll_y, self.cursor_view()));
            return;
        }
        let height = f32::from(self.state.borrow().drawn.bounds.size.height);
        let row_height = self.state.borrow().file_metrics().row;
        let top = self.cursor_view() as f32 * row_height;
        if top < self.scroll_y {
            self.scroll_y = top;
        } else if height > 0. && top + row_height > self.scroll_y + height {
            self.scroll_y = top + row_height - height;
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if cx.try_global::<crate::presentation::Fonts>().is_some() {
            self.state
                .borrow_mut()
                .set_typeface(Typeface::for_window(window, cx));
        }
        self.sync_pictures(window, cx);
        // A dead backend keeps usable rows; the chooser status already names its failure (EY-479).
        let failure = if self.listing.state() == "dead" && !self.listing.held().rows.is_empty() {
            String::new()
        } else {
            self.listing.failure_line(&self.i18n)
        };
        let body = self.surface_element();
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let ink = crate::list::Palette::inherited(&window.text_style(), theme).foreground;
        let bars = crate::g2::pv1::scroller::overlay::surface(
            cx.entity(),
            ink,
            crate::compositor::corner(cx),
        );
        div()
            .size_full()
            .flex()
            .flex_col()
            .when(self.view_mode() == "list", |view| {
                view.child(
                    div()
                        .debug_selector(|| "pk2-header".into())
                        .child(HeaderElement::new(self.state.clone())),
                )
            })
            .children(self.strip.clone().map(|strip| {
                div()
                    .flex_none()
                    .h(gpui::px(crate::g2::sr2::strip::HEIGHT))
                    .child(strip)
            }))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .debug_selector(|| "pk2-listing".into())
                    .child(body)
                    .child(bars),
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
