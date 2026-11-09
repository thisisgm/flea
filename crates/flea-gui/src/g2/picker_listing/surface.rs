//! The chooser's two views share one held listing, selection snapshot and viewport.
use super::view::ListingView;
use crate::grid::{element::GridState, geometry::Metrics, GridView};

struct PickerFocus {
    cursor: usize,
    selected: Vec<usize>,
    focused: bool,
    selection_fill: Option<gpui::Hsla>,
}
impl crate::grid::FocusFacts for PickerFocus {
    fn cursor(&self) -> usize {
        self.cursor
    }
    fn selected(&self, row: usize) -> bool {
        self.selected.binary_search(&row).is_ok()
    }
    fn focused(&self) -> bool {
        self.focused
    }
    fn selection_fill(&self) -> Option<gpui::Hsla> {
        self.selection_fill
    }
}
use std::{cell::RefCell, collections::HashSet, rc::Rc};

pub struct Surface {
    pub mode: &'static str,
    pub grid: GridView,
    pub state: Rc<RefCell<GridState>>,
    pub marks: Rc<HashSet<String>>,
    pub marks_allowed: bool,
    pub focused: bool,
    pub selection_fill: Option<gpui::Hsla>,
    // The listing whose held rows carried a tag: the folder-wide tag slot stays latched for it, as the window's slot does.
    tagged: Option<usize>,
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            mode: "list",
            grid: GridView::new(Metrics::board(), (720., 290.)),
            state: Rc::new(RefCell::new(GridState::omarchy_default())),
            marks: Rc::default(),
            marks_allowed: false,
            focused: true,
            selection_fill: None,
            tagged: None,
        }
    }
}

impl ListingView {
    pub fn zoom_grid(
        &mut self,
        input: crate::grid::ZoomInput,
    ) -> Option<crate::grid::geometry::ThumbSize> {
        let mut grid = self.grid_view();
        let focus = crate::grid::FocusSnapshot {
            cursor: self.listing().cursor(),
            ..Default::default()
        };
        let shown = self.shown.as_deref().map_or(
            crate::grid::Shown::All(self.listing().total()),
            crate::grid::Shown::Only,
        );
        let next = grid.zoom(shown, &focus, input);
        self.scroll_y = grid.scroll;
        self.surface.grid = grid;
        next
    }

    pub fn set_thumbnail(&mut self, size: crate::grid::geometry::ThumbSize) {
        if self.surface.grid.thumb == size {
            return;
        }
        if self.surface.mode != "grid" {
            self.surface.grid.thumb = size;
            self.surface.grid.zoom_travel = 0.;
            return;
        }
        let mut grid = self.grid_view();
        let focus = crate::grid::FocusSnapshot {
            cursor: self.listing().cursor(),
            ..Default::default()
        };
        let shown = self.shown.as_deref().map_or(
            crate::grid::Shown::All(self.listing().total()),
            crate::grid::Shown::Only,
        );
        grid.reflow(shown, &focus, |grid| grid.thumb = size);
        grid.zoom_travel = 0.;
        self.scroll_y = grid.scroll;
        self.surface.grid = grid;
    }

    pub fn grid_window_request(&self) -> Option<String> {
        // GridArea.qml canAsk: a narrowing filter keeps its own rows, so only the whole listing asks for a window.
        if self.surface.mode != "grid" || self.shown.is_some() {
            return None;
        }
        let grid = self.grid_view();
        let total = self.listing().total();
        let layout = grid.layout(crate::grid::Shown::All(total));
        let (first, last) = layout.viewport(grid.scroll);
        let held = self.listing().held();
        let (start, count) = crate::grid::geometry::drift_ask(
            &grid.plan(&layout),
            first,
            last,
            held.start,
            held.rows.len(),
            total,
        )?;
        self.listing().window_request(start, count)
    }

    /// PickerList.qml's wheel: the List body scrolls within its rows and the cursor stays (EY-377).
    pub fn list_scroll_by(&mut self, delta: f32) {
        let (height, row) = {
            let state = self.state.borrow();
            (
                f32::from(state.drawn.bounds.size.height),
                state.file_metrics().row,
            )
        };
        let rows = self
            .shown
            .as_ref()
            .map_or(self.listing().total(), |rows| rows.len());
        let bottom = (rows as f32 * row - height).max(0.);
        self.set_scroll_y((self.scroll_y - delta).clamp(0., bottom));
    }

    /// The window request for List rows the scrolled body shows but the held window does not hold.
    pub fn list_window_request(&self) -> Option<String> {
        if self.surface.mode != "list" || self.shown.is_some() {
            return None;
        }
        let (height, row) = {
            let state = self.state.borrow();
            (
                f32::from(state.drawn.bounds.size.height),
                state.file_metrics().row,
            )
        };
        let first = (self.scroll_y / row).floor() as usize;
        let end = (((self.scroll_y + height) / row).ceil() as usize).min(self.listing().total());
        let held = self.listing().held();
        if first >= held.start && end <= held.start + held.rows.len() {
            return None;
        }
        let count = self.window_size();
        let start = first.saturating_sub(count.saturating_sub(end - first) / 2);
        self.listing().window_request(start, count)
    }

    pub fn set_selection_fill(&mut self, fill: gpui::Hsla) {
        self.surface.selection_fill = Some(fill);
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.surface.focused = focused;
    }

    pub fn set_view(&mut self, mode: &str) {
        self.surface.mode = if mode == "grid" { "grid" } else { "list" };
        self.reveal();
    }

    pub fn view_mode(&self) -> &'static str {
        self.surface.mode
    }

    pub fn grid_view(&self) -> GridView {
        let mut grid = self.surface.grid.clone();
        let bounds = self.surface.state.borrow().bounds;
        if f32::from(bounds.size.width) > 0. {
            grid.size = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        }
        grid
    }

    pub fn grid_state(&self) -> Rc<RefCell<GridState>> {
        self.surface.state.clone()
    }

    #[cfg(test)]
    pub fn list_state(&self) -> Rc<RefCell<crate::list::ListState>> {
        self.state.clone()
    }

    pub fn set_marks(&mut self, paths: Rc<HashSet<String>>, allowed: bool) {
        self.surface.marks = paths;
        self.surface.marks_allowed = allowed;
        self.state.borrow_mut().view.picker = Some(allowed);
    }

    pub fn set_scroll_y(&mut self, y: f32) {
        self.scroll_y = y.max(0.);
        self.surface.grid.scroll = self.scroll_y;
    }
}

impl ListingView {
    /// PickerList.qml visibleRows: the rows the list body holds, a part row counting as one.
    pub fn list_visible_rows(&self) -> usize {
        let state = self.state.borrow();
        let height = f32::from(state.drawn.bounds.size.height);
        ((height / state.file_metrics().row).ceil() as usize).max(1)
    }

    pub fn window_size(&self) -> usize {
        let bounds = self.state.borrow().drawn.bounds;
        if f32::from(bounds.size.width) == 0. {
            return 128;
        }
        let grid = self.grid_view();
        let layout = grid.layout(crate::grid::Shown::All(self.listing().total()));
        flea::ui_logic_picker_listing::window_size(
            (f32::from(bounds.size.height) / self.state.borrow().file_metrics().row).ceil()
                as usize,
            layout.visible_rows(),
            layout.columns,
        )
    }

    pub fn selected_indices(&self) -> Vec<usize> {
        let held = self.listing().held();
        let location = self.listing().facts().location;
        held.rows
            .iter()
            .enumerate()
            .filter_map(|(at, row)| {
                self.surface
                    .marks
                    .contains(&flea::ui_logic_picker_listing::row_path(
                        &location, &row.name,
                    ))
                    .then_some(held.start + at)
            })
            .collect()
    }

    pub fn configure_picker(&mut self, directory: bool) {
        let mut state = self.state.borrow_mut();
        state.view.picker = Some(self.surface.marks_allowed);
        state.view.picker_directory = directory;
        state.set_words(crate::row::format::Words::from_catalog(&self.i18n));
    }

    pub(super) fn surface_element(&mut self) -> gpui::AnyElement {
        use crate::grid::element::{GridElement, Overlay};
        use crate::list::{ListElement, RowStates};
        use gpui::{canvas, div, point, prelude::*, px, size, Bounds};
        let held = self.listing().held();
        let selected = self.selected_indices();
        let recent = flea::ui_logic_picker_listing::is_recent(&self.listing().facts().location);
        if held.rows.iter().any(|row| row.tags.is_some()) {
            self.surface.tagged = Some(held.listing);
        }
        if self.surface.mode == "list" {
            {
                let mut state = self.state.borrow_mut();
                state.view.set_recent(recent);
                state.view.tags_shown = self.surface.tagged == Some(held.listing);
            }
            let states = RowStates {
                listing: held.listing,
                cursor: Some(self.listing().cursor()),
                focused: self.surface.focused,
                selected: Some(Rc::from(selected)),
                ..Default::default()
            };
            return ListElement::with_states(held, self.scroll_y, states, self.state.clone())
                .with_filter(self.shown.clone())
                .into_any_element();
        }
        let view = self.grid_view();
        let focus = Rc::new(PickerFocus {
            cursor: self.listing().cursor(),
            selected,
            focused: self.surface.focused,
            selection_fill: self.surface.selection_fill,
        });
        let decor = self.state.borrow().pictures.clone();
        let grid = GridElement::new(
            held.clone(),
            self.shown.clone(),
            view,
            focus,
            decor,
            Overlay {
                leaf: recent,
                ..Overlay::default()
            },
            self.surface.state.clone(),
        );
        let state = self.surface.state.clone();
        let allowed = self.surface.marks_allowed;
        let directory = self.state.borrow().view.picker_directory;
        div()
            .relative()
            .size_full()
            .debug_selector(|| "picker-grid".into())
            .child(grid)
            .child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, cx| {
                        if !allowed {
                            return;
                        }
                        let state = state.borrow();
                        let Some(layout) = state.layout else {
                            return;
                        };
                        let theme = cx
                            .try_global::<crate::palette::Theme>()
                            .copied()
                            .unwrap_or_default();
                        let palette = crate::list::Palette::inherited(&window.text_style(), theme);
                        window.with_content_mask(
                            Some(gpui::ContentMask {
                                bounds: state.bounds,
                            }),
                            |window| {
                                for tile in state.drawn() {
                                    let Some(row) = tile
                                        .listing
                                        .checked_sub(held.start)
                                        .and_then(|at| held.rows.get(at))
                                    else {
                                        continue;
                                    };
                                    if flea::ui_logic_picker_listing::directory(
                                        row.dir, row.mode, &row.icon,
                                    ) != directory
                                    {
                                        continue;
                                    }
                                    let cell = layout.cell(state.scroll, tile.view);
                                    let side = crate::list::picker::check_size(
                                        layout.metrics.body_small,
                                        layout.metrics.hairline,
                                    );
                                    let bounds = Bounds::new(
                                        point(
                                            state.bounds.origin.x + px(cell.x + layout.metrics.gap),
                                            state.bounds.origin.y + px(cell.y + layout.metrics.gap),
                                        ),
                                        size(px(side), px(side)),
                                    );
                                    crate::list::picker::checkbox(
                                        bounds,
                                        tile.selected,
                                        &palette,
                                        window,
                                        cx,
                                    );
                                }
                            },
                        );
                    },
                )
                .absolute()
                .size_full(),
            )
            .into_any_element()
    }
}
