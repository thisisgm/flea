//! Per-pane Grid composition joins the existing Element and bounded window policy to the live Shell.
use super::{
    element::{Decor, GridElement, GridState, Overlay, Plain},
    geometry::{Density, Metrics, ThumbSize},
    FocusSnapshot, GridView, Shown,
};
use crate::{backend::Failure, pane::Pane, window::pane::PaneHost};
use flea::jsondoc::Json;
use gpui::{font, AnyElement, IntoElement};
use std::{cell::RefCell, rc::Rc};

#[path = "anchor.rs"]
mod anchor;

pub(crate) struct GridHost {
    pub view: GridView,
    pub state: Rc<RefCell<GridState>>,
    pub decor: Rc<dyn Decor>,
    pub overlay: Overlay,
    followed: Option<(usize, usize)>,
    pub(super) refused: Option<usize>,
    /// The backend's reason for the refused window, shown if the refusal leaves the pane undrawable.
    pub(super) refusal: String,
    failed: Option<(usize, usize, f32)>,
    /// A window send that failed while drawing, handed to the Shell's failure path after the frame.
    pub(super) failure: Option<Failure>,
    pub(super) next_view: Option<super::transition::PendingView>,
}

impl GridHost {
    /// Explicit cursor actions reveal their target even when its identity is unchanged.
    pub(crate) fn refollow(&mut self) {
        self.followed = None;
    }

    pub(crate) fn band_ready(&self) -> bool {
        self.next_view.is_none() && !self.view.landing_wait
    }

    fn new(size: (f32, f32), data: &Json) -> Self {
        let mut view = GridView::new(Metrics::board(), size);
        view.thumb = ThumbSize::named(
            data.get("preview")
                .and_then(|v| v.get("thumbSize"))
                .and_then(Json::as_str)
                .unwrap_or("medium"),
        );
        view.density = Density::named(
            data.get("density")
                .and_then(Json::as_str)
                .unwrap_or("compact"),
        );
        Self {
            view,
            state: Rc::new(RefCell::new(GridState::new(font(
                "JetBrainsMono Nerd Font",
            )))),
            followed: None,
            refused: None,
            refusal: String::new(),
            failed: None,
            failure: None,
            next_view: None,
            decor: Rc::new(Plain),
            overlay: Overlay::default(),
        }
    }

    pub fn focus(pane: &Pane) -> FocusSnapshot {
        FocusSnapshot {
            cursor: pane.cursor,
            ..Default::default()
        }
    }

    fn request(&mut self, pane: &mut Pane, shown: Shown) -> Option<(usize, usize)> {
        if self.settle_refusal(pane) {
            return None;
        }
        let focus = Self::focus(pane);
        let narrowed = shown.narrowed();
        if !narrowed {
            let drawn = self.view.layout(shown).drawn(self.view.drawn_scroll());
            pane.requests
                .keep_sparse_span(&pane.held, drawn.start, drawn.len());
        }
        if narrowed {
            self.view.end_landing(shown, &focus, true);
            let identity = (pane.held.listing, pane.cursor);
            if self.followed != Some(identity) {
                self.followed = Some(identity);
                self.view.follow_cursor(pane, shown, &focus);
            }
            return self.view.wanted_filtered_window(pane, shown);
        }
        // Missing filtered facts still use listing coordinates to fetch their window.
        let listing = Shown::All(pane.held.total);
        let landed = self.view.rows_landed(pane, listing, &focus);
        let identity = (pane.held.listing, pane.cursor);
        if self.failed == Some((identity.0, identity.1, self.view.scroll)) {
            return None;
        }
        self.failed = None;
        let missing = narrowed && !self.view.landing_wait && pane.held.row(pane.cursor).is_none();
        let followed = if self.followed != Some(identity) || missing {
            self.followed = Some(identity);
            self.view.follow_cursor(pane, listing, &focus)
        } else {
            None
        };
        landed
            .or(followed)
            .or_else(|| self.view.wanted_window(pane, listing))
    }

    pub(super) fn settle_refusal(&mut self, pane: &mut Pane) -> bool {
        if self.refused.take() != Some(pane.held.listing) {
            self.refusal.clear();
            return false;
        }
        // Keep the cursor the operator has when no painted tile shows it, never an unseen row.
        pane.cursor = self
            .state
            .borrow()
            .drawn()
            .iter()
            .find(|tile| tile.cursor)
            .map_or(pane.cursor, |tile| tile.listing)
            .min(pane.held.total.saturating_sub(1));
        self.view
            .end_landing(Shown::All(pane.held.total), &Self::focus(pane), false);
        self.failed = Some((pane.held.listing, pane.cursor, self.view.scroll));
        self.followed = Some((pane.held.listing, pane.cursor));
        true
    }

    /// Leaving Grid releases its shaped tiles and frame facts; no hidden renderer or IPC rectangle survives.
    pub fn park(&mut self) {
        if self.view.visible {
            self.view.visible = false;
            self.view.hidden_held = true;
            self.view.landing_wait = false;
            self.view.landing_held = None;
            self.view.hovered = None;
            self.view.pointer = None;
            let font = self.state.borrow().font.clone();
            *self.state.borrow_mut() = GridState::new(font);
            self.followed = None;
            self.failed = None;
            self.failure = None;
            self.refusal.clear();
            self.refused = None;
            self.next_view = None;
        }
    }
}

impl PaneHost {
    pub(crate) fn grid_refusal(&mut self, event: &crate::backend::Event) -> bool {
        let (operation, path, message) = match event {
            crate::backend::Event::Refused {
                operation,
                path,
                message,
                ..
            }
            | crate::backend::Event::Reply(flea::protocol::Reply::Error {
                operation,
                path,
                message,
                ..
            }) => (operation, path, message),
            _ => return false,
        };
        if operation != "window" || path != self.pane.path() || self.pane.busy() {
            return false;
        }
        if let (Some(grid), Some(request)) = (&mut self.grid, self.pane.requests.outstanding) {
            if grid.view.visible && request.listing == self.pane.held.listing {
                self.pane.requests.refused = Some(request);
                grid.refused = Some(request.listing);
                grid.refusal.clone_from(message);
                return true;
            }
        }
        false
    }

    /// Sizes the Grid from the pane's face before any state surface or pane element is built.
    fn grid_ready(
        &mut self,
        size: (f32, f32),
        data: &Json,
        locale: &crate::window::locale::Snapshot,
    ) {
        let grid = self.grid.get_or_insert_with(|| GridHost::new(size, data));
        if self.pane.state.kind != crate::pane::states::Kind::Ready {
            grid.state.borrow_mut().clear_frame();
        }
        let filtered = self.filter.shown.clone();
        let shown = filtered
            .as_deref()
            .map_or(Shown::All(self.pane.held.total), Shown::Only);
        let focus = grid.next_view.as_ref().map_or_else(
            || GridHost::focus(&self.pane),
            |pending| pending.focus.clone(),
        );
        grid.view.set_visible(true, shown, &focus);
        if grid.view.size != size {
            grid.view.reflow(shown, &focus, |view| view.size = size);
        }
        let list = self.list.borrow();
        {
            let mut state = grid.state.borrow_mut();
            state.font = list.typeface.font.clone();
            state.set_catalog(&locale.catalog);
        }
        let tokens = crate::list::Metrics::for_typeface(&list.typeface);
        let metrics = Metrics {
            gap: tokens.gap,
            pad_x: tokens.pad,
            hairline: tokens.hairline,
            min_cell: (125. * f32::from(list.typeface.body) / 12.).round(),
            caption_line: f32::from(list.typeface.caption) * 15. / 11.,
            chrome: tokens.header,
            clip_mark: tokens.clip,
            body_small: f32::from(list.typeface.body_small),
            caption: f32::from(list.typeface.caption),
            file_row: tokens.row,
        };
        if grid.view.metrics != metrics {
            grid.view
                .reflow(shown, &focus, |view| view.metrics = metrics);
        }
        drop(list);
        // keepScreenLine reads where the cursor tile sits when the pane leaves, from Grid's own scroll.
        self.nav.lines.grid = if self.nav.keep_screen_line {
            self.grid_anchor_viewport()
        } else {
            None
        };
        self.restore_grid_anchor();
    }

    /// The Grid window ask, settled before the frame builds its state surfaces so a failed send draws Error now.
    pub(crate) fn settle_grid(
        &mut self,
        size: (f32, f32),
        data: &Json,
        locale: &crate::window::locale::Snapshot,
    ) {
        self.grid_ready(size, data, locale);
        let grid = self.grid.as_mut().expect("a Grid host");
        let filtered = self.filter.shown.clone();
        let shown = filtered
            .as_deref()
            .map_or(Shown::All(self.pane.held.total), Shown::Only);
        if !self.stopped && grid.next_view.is_none() {
            if let (Some((start, count)), Some(backend)) =
                (grid.request(&mut self.pane, shown), &self.backend)
            {
                if let Err(error) = backend.window(start, count) {
                    self.stopped = true;
                    grid.failure = Some(error);
                    grid.view
                        .end_landing(shown, &GridHost::focus(&self.pane), false);
                }
            }
        }
    }

    pub(crate) fn render_grid(
        &mut self,
        size: (f32, f32),
        data: &Json,
        locale: &crate::window::locale::Snapshot,
        renaming: Option<usize>,
        focused: bool,
        dropping: Option<(usize, super::element::DropKind)>,
    ) -> AnyElement {
        self.grid_ready(size, data, locale);
        let grid = self.grid.as_mut().expect("a Grid host");
        grid.overlay.renaming = renaming;
        // The run the captions mark is the filter's while it stands, else the search's, as the list reads it.
        let query = match self.filter.shown {
            Some(_) if !self.pane.state.search => self.filter.owner.query(),
            _ => &self.pane.state.query,
        };
        if grid.overlay.query.as_ref() != query {
            grid.overlay.query = query.to_owned().into();
        }
        grid.overlay.drop = dropping;
        let filtered = self.filter.shown.clone();
        let shown = filtered
            .as_deref()
            .map_or(Shown::All(self.pane.held.total), Shown::Only);
        let mut focus = grid.next_view.as_ref().map_or_else(
            || GridHost::focus(&self.pane),
            |pending| pending.focus.clone(),
        );
        focus.band = self.input.band.is_some();
        focus.selected = grid
            .view
            .layout(shown)
            .drawn(grid.view.drawn_scroll())
            .filter_map(|view| shown.at(view))
            .filter(|row| self.selected.binary_search(row).is_ok())
            .collect();
        if grid.view.landing_wait || grid.next_view.is_some() {
            let previous = grid.state.borrow();
            if let Some(cursor) = previous.drawn().iter().find(|tile| tile.cursor) {
                focus.cursor = cursor.listing;
                focus.selected = previous
                    .drawn()
                    .iter()
                    .filter(|tile| tile.selected)
                    .map(|tile| tile.listing)
                    .collect();
            }
        }
        focus.selected.sort_unstable();
        focus.grey = !focused;
        let element = GridElement::new(
            Rc::clone(
                grid.next_view
                    .as_ref()
                    .map_or(&self.pane.held, |pending| &pending.held),
            ),
            filtered.clone(),
            GridView {
                scroll: grid.view.drawn_scroll(),
                ..grid.view.clone()
            },
            Rc::new(focus),
            Rc::clone(&grid.decor),
            grid.overlay.clone(),
            Rc::clone(&grid.state),
        )
        .into_any_element();
        element
    }
}

impl crate::state::Shell {
    /// A Grid window send that failed while drawing ends on the same Error surface as any backend failure.
    pub(crate) fn finish_grid_failures(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        for side in 0..2 {
            let failure = self.panes[side]
                .as_mut()
                .and_then(|host| host.grid.as_mut())
                .and_then(|grid| grid.failure.take());
            if let Some(failure) = failure {
                self.receive(side, crate::backend::Event::Failed(failure), window, cx);
            }
        }
    }
}
