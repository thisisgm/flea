//! The List Element: one custom Element for the whole pane, fixed-height rows painted as quads, marks and pre-shaped lines.
pub use super::painted_row::{DropLine, Shown};
pub use super::row_text::drawn_name;
use super::row_text::{bold_name, prepare};
use super::{
    columns::{Column, Layout},
    pane::Held,
    ListState, Metrics, Palette, RowStates, CACHE_SCREENS, CURSOR_WASH, CUT_DIM, GREY_CURSOR,
    HOVER, MATCH_WASH,
};
use crate::palette::Theme;
use crate::row::{
    cache::{Extra, Shaping},
    format,
    marks::ClipMark,
};
use gpui::{
    fill, point, px, relative, size, App, Bounds, ContentMask, Element, ElementId, GlobalElementId,
    GlyphRun, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, SharedString, Style,
    TransformationMatrix, Window,
};
use std::{cell::RefCell, rc::Rc};

// Row.qml's loading bar draws at a fifth of the muted frame, so scrolling reads as loading.
const LOADING: f32 = 0.2;
pub use super::loading::loading_bar;

// The list surface's fixed row height: the scroll model and every reader share it from here.
pub const ROW_HEIGHT: f32 = 31.;

pub struct ListElement {
    held: Rc<Held>,
    scroll_y: f32,
    states: RowStates,
    state: Rc<RefCell<ListState>>,
    search_feedback: Option<crate::window::search::Draw>,
    shown: Option<Rc<[usize]>>,
}

impl ListElement {
    /// A pane with only its cursor, focused, as the window draws it until focus and selection register their states.
    pub fn new(
        held: Rc<Held>,
        scroll_y: f32,
        cursor: usize,
        state: Rc<RefCell<ListState>>,
    ) -> Self {
        let states = RowStates::cursor_only(held.listing, cursor);
        Self::with_states(held, scroll_y, states, state)
    }

    pub fn with_states(
        held: Rc<Held>,
        scroll_y: f32,
        states: RowStates,
        state: Rc<RefCell<ListState>>,
    ) -> Self {
        Self {
            held,
            scroll_y,
            states,
            state,
            search_feedback: None,
            shown: None,
        }
    }

    /// A filter's drawn rows, None for every row; a view position then draws the listing row at that place.
    pub(crate) fn with_filter(mut self, shown: Option<Rc<[usize]>>) -> Self {
        self.shown = shown;
        self
    }

    pub(crate) fn with_search_feedback(
        mut self,
        draw: Option<crate::window::search::Draw>,
    ) -> Self {
        self.search_feedback = draw;
        self
    }
}

/// What prepaint resolved for paint: the palette, the geometry and the drawn columns.
#[derive(Clone, Copy)]
pub struct Frame {
    palette: Palette,
    metrics: Metrics,
    layout: Layout,
    // The row's span left of the scroll lane, and the caption size the loading bar takes its height from.
    row_width: f32,
    caption: f32,
}

// Whole cells of this advance in a width, the unit Format.middleElide() budgets in.
fn cells(width: f32, advance: f32) -> usize {
    if advance > 0. {
        (width / advance).floor().max(0.) as usize
    } else {
        0
    }
}

/// Paints a prepared line from the tiles its glyph run kept, so a row already seen costs no lookup per glyph.
pub(super) fn paint_line(
    run: &GlyphRun,
    x: Pixels,
    top: Pixels,
    height: f32,
    color: Hsla,
    window: &mut Window,
) {
    let line = run.line();
    let baseline = top + (px(height) - line.ascent - line.descent) / 2. + line.ascent;
    let scale = window.scale_factor();
    let baseline = px((f32::from(baseline) * scale).floor() / scale);
    #[cfg(test)]
    crate::bfx_cols_tests::record_line(run, x, baseline);
    let _ = window.paint_glyph_run(point(x, baseline), run, color);
}

pub(super) fn paint_mark(
    key: SharedString,
    data: &'static [u8],
    at: gpui::Point<Pixels>,
    side: f32,
    ink: Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    let bounds = Bounds::new(at, size(px(side), px(side)));
    #[cfg(test)]
    crate::bfx_cols_tests::record_mark(&key, bounds);
    let _ = window.paint_svg(
        bounds,
        key,
        Some(data),
        TransformationMatrix::unit(),
        ink,
        cx,
    );
}

impl IntoElement for ListElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for ListElement {
    type RequestLayoutState = ();
    type PrepaintState = Frame;

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            size: size(relative(1.).into(), relative(1.).into()),
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Frame {
        let mut state = self.state.borrow_mut();
        let (metrics, _, layout) = state.layout(f32::from(bounds.size.width), window);
        let advance = state.advance(window);
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = state.palette(window, theme);
        state.drawn_cursor = self.states.cursor;
        state.drawn_focused = self.states.focused;
        state.drawn_rows = Rc::clone(&self.held);
        state.drawn_filter = self.shown.clone();
        let held = &*self.held;
        let ListState {
            typeface,
            rows,
            visible,
            view,
            drawn,
            blank,
            drop_line,
            pictures,
            ..
        } = &mut *state;
        let shaping = Shaping {
            listing: held.listing,
            font: typeface.font.clone(),
            body: typeface.body,
            caption: typeface.caption,
            name_cells: cells(layout.name.width, advance.body),
            advance: advance.body,
            clip_cells: cells(layout.name.width - metrics.gap - metrics.clip, advance.body),
            cells: Column::ALL.map(|column| {
                layout
                    .cell(column)
                    .map_or(0, |cell| cells(cell.width, advance.caption))
            }),
            scale: window.scale_factor(),
            text: view.text_revision(),
            languages: view.lang_marks,
        };
        let row_width = f32::from(bounds.size.width) - metrics.lane;
        let caption = f32::from(typeface.caption);
        // drag-cut-move: the drop caption takes the cells' place, right-aligned, and narrows the name only where it would cover it.
        let dropping = self
            .states
            .drop_target
            .filter(|_| self.states.listing == held.listing)
            .zip(self.states.drop_label.as_ref());
        let drop_box = dropping.map(|(index, label)| {
            let width = f32::from(
                DropLine::shaped(drop_line, label, &typeface.font, typeface.caption, window)
                    .line()
                    .width,
            );
            let x = row_width - metrics.pad - width;
            let room = x - metrics.gap - layout.name.x;
            // Row.qml narrows a cut name by its scissors and the caption together, so the mark stays clear of the label.
            let clipped = room - metrics.gap - metrics.clip;
            let cells = (room < layout.name.width).then(|| {
                (
                    cells(room.max(0.), advance.body),
                    cells(clipped.max(0.), advance.body),
                )
            });
            (index, x, width, cells)
        });
        let height = f32::from(bounds.size.height);
        let drawn_total = self.shown.as_ref().map_or(held.total, |rows| rows.len());
        // A refused rename grows its row and moves the rows below it down by the band.
        let grow = self.states.grow;
        let (first, last) = grow.span(self.scroll_y, height, metrics.row, drawn_total);
        rows.begin(
            &shaping,
            ((height / metrics.row).ceil() as usize).max(1) * CACHE_SCREENS,
        );
        visible.clear();
        let revision = view.mark_revision(held.listing);
        let mut loading = 0;
        // A thumbnail is filtered to the mark slot's device pixels, the side super::picker::paint_icon draws it at.
        let side = if view.dual {
            metrics.mark
        } else {
            metrics.slot
        };
        let slot_pixels = (side * window.scale_factor()).ceil() as u32;
        for view_at in first..last.max(first) {
            let index = self.shown.as_ref().map_or(view_at, |rows| rows[view_at]);
            let marks = self.states.row(index, held.listing);
            let Some(facts) = held.row(index) else {
                loading += 1;
                visible.push(Shown {
                    index,
                    view: view_at,
                    prepared: None,
                    clip: None,
                    state: marks,
                    bold_match: false,
                    picture: None,
                });
                continue;
            };
            let drop = drop_box.and_then(|(at, _, _, cells)| cells.filter(|_| at == index));
            // A held clipboard or walk is looked up once per change or per entered row, never on a still frame.
            let resolve = || {
                let clip = view.clip.mark_for(&facts.name);
                let walked = if facts.dir {
                    view.walked_for(held.listing, index)
                } else {
                    None
                };
                (
                    Extra {
                        walked,
                        clipped: clip.is_some(),
                        drop: None,
                    },
                    clip,
                )
            };
            let (prepared, _extra, clip) =
                rows.get_or_prepare_marked(index, facts, revision, drop, resolve, |extra| {
                    let shaping = super::outline::shaping(&shaping, facts, &metrics, advance.body);
                    prepare(facts, extra, &shaping, view, &layout, blank, window)
                });
            let bold_match = marks.cursor && (view.searching || !marks.selected);
            if bold_match && prepared.run.is_some() {
                prepared
                    .bold
                    .get_or_init(|| bold_name(&prepared, &shaping, window));
            }
            visible.push(Shown {
                index,
                view: view_at,
                prepared: Some(prepared),
                clip,
                state: marks,
                bold_match,
                // Row.qml dims a cut row's thumbnail with the rest of its mark.
                picture: pictures.lend(index, facts).map(|picture| {
                    let dim = if clip == Some(ClipMark::Cut) {
                        CUT_DIM
                    } else {
                        1.
                    };
                    pictures.slotted(picture, slot_pixels, dim)
                }),
            });
        }
        *drawn = super::Drawn {
            bounds,
            mask: bounds,
            layout,
            metrics,
            palette: Some(palette.text_inks(window, cx)),
            scroll_y: self.scroll_y,
            listing: held.listing,
            frames: drawn.frames + 1,
            mirrored: window.layout_mirrored(),
            grow,
            drop: drop_box.map(|(index, x, width, _)| {
                let top = bounds.origin.y + px(grow.top(index, metrics.row) - self.scroll_y);
                (
                    index,
                    Bounds::new(
                        point(bounds.origin.x + px(x), top),
                        size(px(width), px(metrics.row)),
                    ),
                )
            }),
            loading,
            count: drawn_total,
        };
        // The probe reads this frame's rows, so it runs after the visible rows are rebuilt above.
        crate::visual::geometry::rows(&state, bounds, self.scroll_y, metrics.row, window);
        Frame {
            palette,
            metrics,
            layout,
            row_width,
            caption,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        frame: &mut Frame,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Frame {
            palette,
            metrics,
            layout,
            row_width,
            caption,
        } = *frame;
        let text = palette.text_inks(window, cx);
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let state = self.state.borrow();
        let view = &state.view;
        let left = bounds.origin.x;
        let row = metrics.row;
        let grow = state.drawn.grow;
        let top_of = |at: usize| bounds.origin.y + px(grow.top(at, row) - self.scroll_y);
        let focused = self.states.focused;
        crate::window::glass::producers::paint(
            crate::window::glass::producers::Producer::Listing,
            bounds,
            palette.background,
            window,
            cx,
        );
        window.with_content_mask(
            Some(ContentMask {
                bounds: state.drawn.mask,
            }),
            |window| {
                for shown in &state.visible {
                    let top = top_of(shown.view);
                    let height = px(grow.height(shown.view, row));
                    let ground = Bounds::new(point(left, top), size(bounds.size.width, height));
                    let s = shown.state;
                    // The cursor draws its rung alone on a marked row, so no plate stacks into a brighter band.
                    // drag-cut-move: a drop target takes the cursor's accent wash and edge whichever window holds the keyboard.
                    let wash = if s.drop_target {
                        Some(palette.accent.opacity(CURSOR_WASH))
                    } else if s.cursor {
                        Some(if focused {
                            palette.accent.opacity(CURSOR_WASH)
                        } else {
                            palette.foreground.opacity(GREY_CURSOR)
                        })
                    } else if s.selected {
                        Some(if self.states.other_pane && !focused {
                            palette.foreground.opacity(GREY_CURSOR)
                        } else {
                            palette.marked
                        })
                    } else if s.hovered {
                        Some(palette.foreground.opacity(HOVER))
                    } else {
                        None
                    };
                    if let Some(wash) = wash {
                        window.paint_quad(fill(ground, wash));
                    }
                    if s.drop_target || (s.cursor && focused) {
                        window.paint_quad(fill(
                            Bounds::new(point(left, top), size(px(metrics.edge), px(row))),
                            palette.accent_frame,
                        ));
                    }
                    let Some(prepared) = &shown.prepared else {
                        window.paint_quad(fill(
                            loading_bar(left, top, &metrics, row_width, caption),
                            palette.muted_frame.opacity(LOADING),
                        ));
                        continue;
                    };
                    // At rest a match run sits on the accent wash; the cursor row draws it bold instead.
                    let (Some((start, end)), true) = (prepared.run, s.washes_run(view.searching))
                    else {
                        continue;
                    };
                    let line = prepared.name.line();
                    let step = self
                        .held
                        .row(shown.index)
                        .map_or(0., |row| super::outline::indent(row, &metrics));
                    let x = left + px(layout.name.x + step);
                    let limit = left + px(layout.name.x + layout.name.width);
                    let height = line.ascent + line.descent;
                    let y = top + (px(row) - height) / 2.;
                    let Some((run_start, run_end)) =
                        crate::g2::file_search::run::wash_span(line, Some((start, end)), false)
                    else {
                        continue;
                    };
                    let from = (x + run_start).min(limit);
                    let to = (x + run_end).min(limit);
                    window.paint_quad(fill(
                        Bounds::from_corners(point(from, y), point(to, y + height)),
                        palette.accent.opacity(MATCH_WASH),
                    ));
                }
                // One layer for every mark and glyph, so the scene orders the whole list once rather than once a line.
                let (
                    mut marks_painted,
                    mut cells_painted,
                    mut pictures_painted,
                    mut checks_painted,
                ) = (0, 0, 0, 0);
                window.paint_layer(bounds, |window| {
                    for shown in &state.visible {
                        let lifted = shown.state.lifted();
                        let top = top_of(shown.view);
                        let origin = (left, top, self.held.row(shown.index));
                        checks_painted += super::picker::paint_check(
                            shown, view, metrics, palette, origin, window, cx,
                        );
                        let ink = super::picker::mark_ink(shown, view, palette, &self.states);
                        let (marks, pictures) = super::picker::paint_icon(
                            shown, view, metrics, ink, origin, window, cx,
                        );
                        marks_painted += marks;
                        pictures_painted += pictures;
                        let Some(prepared) = &shown.prepared else {
                            continue;
                        };
                        let meta = if lifted || view.picker.is_some() {
                            text.foreground
                        } else {
                            text.metadata
                        };
                        if shown.state.drop_target {
                            continue;
                        }
                        let recent = view.today.zip(self.held.row(shown.index)).is_some_and(
                            |(today, facts)| {
                                view.date_stamp(facts)
                                    .is_some_and(|stamp| format::is_recent(stamp, today))
                            },
                        );
                        for (column, line) in [
                            (Column::Mode, &prepared.mode),
                            (Column::Size, &prepared.size),
                            (Column::Date, &prepared.date),
                            (Column::Kind, &prepared.kind),
                        ] {
                            let Some(cell) = layout.cell(column) else {
                                continue;
                            };
                            let x = match column {
                                Column::Size | Column::Date | Column::Kind => {
                                    left + px(cell.right()) - line.line().width
                                }
                                Column::Mode => left + px(cell.x),
                            };
                            let ink = if column == Column::Date && recent {
                                text.foreground
                            } else {
                                meta
                            };
                            paint_line(line, x, top, row, ink, window);
                            cells_painted += 1;
                        }
                    }
                    let names = Bounds::new(
                        point(left + px(layout.name.x), bounds.origin.y),
                        size(px(layout.name.width), bounds.size.height),
                    );
                    window.with_content_mask(Some(ContentMask { bounds: names }), |window| {
                        for shown in &state.visible {
                            let Some(prepared) = &shown.prepared else {
                                continue;
                            };
                            if shown.state.renaming {
                                continue;
                            }
                            let names = super::outline::name_slot(
                                names,
                                self.held.row(shown.index),
                                &metrics,
                            );
                            let ink = text.name(prepared.ink, shown.state.lifted());
                            let line = shown.name_line().expect("prepared above");
                            let name_mask = ContentMask {
                                bounds: shown.name_slot(names, &metrics),
                            };
                            window.with_content_mask(Some(name_mask), |window| {
                                paint_line(
                                    line,
                                    names.origin.x,
                                    top_of(shown.view),
                                    row,
                                    ink,
                                    window,
                                );
                            });
                            if view.searching && f32::from(prepared.location.line().width) > 0. {
                                let x =
                                    names.origin.x + prepared.name.line().width + px(metrics.gap);
                                let caption_ink = if shown.state.lifted() {
                                    text.foreground
                                } else {
                                    text.metadata
                                };
                                paint_line(
                                    &prepared.location,
                                    x,
                                    top_of(shown.view),
                                    row,
                                    caption_ink,
                                    window,
                                );
                            }
                        }
                    });
                    super::tags::paint(&state, &self.held, bounds, &theme, window);
                    super::git::paint(&state, &self.held, bounds, &theme, window);
                    // RowDropTarget.qml: the label is an accent caption at the row's right padding, in place of the hidden cells.
                    if let (Some((_, caption)), Some(line)) = (state.drawn.drop, &state.drop_line) {
                        paint_line(
                            &line.run,
                            caption.origin.x,
                            caption.origin.y,
                            row,
                            palette.accent,
                            window,
                        );
                    }
                    // The clip mark occupies the reserved strip, one pixel above the row's centre line.
                    for shown in &state.visible {
                        let Some(clip) = shown.clip else {
                            continue;
                        };
                        if shown.state.renaming || view.searching {
                            continue;
                        }
                        let name = Bounds::new(
                            point(names.origin.x, top_of(shown.view)),
                            size(names.size.width, px(row)),
                        );
                        let name =
                            super::outline::name_slot(name, self.held.row(shown.index), &metrics);
                        let Some(mark) = shown.clip_bounds(name, &metrics) else {
                            continue;
                        };
                        let (key, data) = clip.svg();
                        paint_mark(
                            key,
                            data,
                            mark.origin,
                            metrics.clip,
                            palette.muted_frame,
                            window,
                            cx,
                        );
                    }
                });
                state.painted_marks.set(marks_painted);
                state.painted_cells.set(cells_painted);
                state.painted_pictures.set(pictures_painted);
                state.painted_checks.set(checks_painted);
            },
        );
        if let Some(draw) = &self.search_feedback {
            draw.painted(&state.visible, cx);
        }
    }
}
