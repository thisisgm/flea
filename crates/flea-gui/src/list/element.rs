//! The List Element: one custom Element for the whole pane, fixed-height rows painted as quads, marks and pre-shaped lines.
use super::{
    columns::{Column, Layout},
    pane::Held,
    ListState, ListView, Metrics, Palette, RowState, RowStates, CACHE_SCREENS, CURSOR_WASH,
    CUT_DIM, GREY_CURSOR, HOVER, MATCH_WASH, TRACK,
};
use crate::palette::Theme;
use crate::row::{
    self,
    cache::{Extra, Prepared, Shaping},
    format,
    marks::{ClipMark, Mark},
};
use flea::protocol::RowFacts;
use gpui::{
    fill, point, px, relative, size, App, Bounds, ContentMask, Element, ElementId, Font,
    GlobalElementId, GlyphRun, Hsla, InspectorElementId, IntoElement, LayoutId, LineLayout, Pixels,
    SharedString, Style, TextRun, TransformationMatrix, Window,
};
use std::{borrow::Cow, cell::OnceCell, cell::RefCell, rc::Rc, sync::Arc};

// Row.qml's loading bar draws at a fifth of the muted frame, so scrolling reads as loading.
const LOADING: f32 = 0.2;
const LOADING_WIDTH: f32 = 0.3;

/// The drag owner's caption for a drop target row, shaped once per label, face and size.
pub struct DropLine {
    label: SharedString,
    font: Font,
    size: Pixels,
    run: GlyphRun,
}

impl DropLine {
    fn shaped<'a>(
        slot: &'a mut Option<DropLine>,
        label: &SharedString,
        font: &Font,
        size: Pixels,
        window: &Window,
    ) -> &'a GlyphRun {
        if !slot
            .as_ref()
            .is_some_and(|line| line.label == *label && line.font == *font && line.size == size)
        {
            let run = GlyphRun::new(window.text_system().layout_line(
                label,
                size,
                &[run_of(label.len(), font)],
                None,
            ));
            *slot = Some(DropLine {
                label: label.clone(),
                font: font.clone(),
                size,
                run,
            });
        }
        &slot.as_ref().expect("shaped above").run
    }
}

/// One visible row this frame: its index, its shaped lines once held, its clipboard mark and its states.
#[derive(Clone)]
pub struct Shown {
    pub index: usize,
    pub prepared: Option<Rc<Prepared>>,
    pub clip: Option<ClipMark>,
    pub state: RowState,
}

pub struct ListElement {
    held: Rc<Held>,
    scroll_y: f32,
    states: RowStates,
    state: Rc<RefCell<ListState>>,
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
        }
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

fn run_of(len: usize, font: &Font) -> TextRun {
    TextRun {
        len,
        font: font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

/// The name as drawn: middle-elided to its budget, or right-elided around a match run so the run stays in its head.
pub fn drawn_name(
    facts: &RowFacts,
    extra: Extra,
    shaping: &Shaping,
    query: &str,
) -> (String, Option<(usize, usize)>) {
    let name = row::decorated_name(facts);
    let budget = if extra.clipped {
        shaping.clip_cells
    } else {
        shaping.name_cells
    };
    let budget = extra.drop.map_or(budget, |drop| drop.min(budget));
    let Some((start, end)) = format::match_run(&facts.name, query) else {
        return (row::middle_elide(&name, budget).into_owned(), None);
    };
    let (drawn, kept) = match row::right_elide(&name, budget) {
        Cow::Borrowed(_) => (name.clone(), name.len()),
        Cow::Owned(text) => {
            let kept = text.len() - '\u{2026}'.len_utf8();
            (text, kept)
        }
    };
    (drawn, (start < kept).then_some((start, end.min(kept))))
}

/// Shapes one row's cells once, each elided to its own budget, so no long name is shaped whole and no hidden column at all.
fn prepare(
    facts: &RowFacts,
    extra: Extra,
    shaping: &Shaping,
    view: &ListView,
    layout: &Layout,
    blank: &Arc<LineLayout>,
    window: &Window,
) -> Prepared {
    let text = window.text_system();
    let shape = |value: &str, size: Pixels| {
        GlyphRun::new(text.layout_line(value, size, &[run_of(value.len(), &shaping.font)], None))
    };
    let cell = |column: Column, value: &dyn Fn() -> String| {
        if layout.shown(column) {
            shape(
                &row::right_elide(&value(), shaping.cells[column.slot()]),
                shaping.caption,
            )
        } else {
            GlyphRun::new(Arc::clone(blank))
        }
    };
    let (name, run) = drawn_name(facts, extra, shaping, &view.query);
    let words = &view.words;
    Prepared {
        label: name.clone(),
        size_text: format::size_text(facts, extra.walked, words),
        date_text: format::date_text(facts.mtime, words),
        name: shape(&name, shaping.body),
        size: cell(Column::Size, &|| {
            format::size_text(facts, extra.walked, words)
        }),
        date: cell(Column::Date, &|| format::date_text(facts.mtime, words)),
        mode: cell(Column::Mode, &|| format::permissions(facts.mode)),
        // The rows reply carries no kind names on this protocol yet, so the cell is empty, as Row.kindText() is without row.k.
        kind: cell(Column::Kind, &String::new),
        mark: Mark::for_facts(facts, shaping.languages),
        ink: row::name_ink(facts),
        run,
        bold: OnceCell::new(),
    }
}

/// The cursor row's name with its match run in bold, the board's cursor treatment for a marked name.
fn bold_name(
    facts: &RowFacts,
    extra: Extra,
    shaping: &Shaping,
    query: &str,
    window: &Window,
) -> GlyphRun {
    let (name, run) = drawn_name(facts, extra, shaping, query);
    let (start, end) = run.unwrap_or((name.len(), name.len()));
    let heavy = shaping.font.clone().bold();
    let runs: Vec<TextRun> = [
        (start, &shaping.font),
        (end - start, &heavy),
        (name.len() - end, &shaping.font),
    ]
    .into_iter()
    .filter(|(len, _)| *len > 0)
    .map(|(len, font)| run_of(len, font))
    .collect();
    GlyphRun::new(
        window
            .text_system()
            .layout_line(&name, shaping.body, &runs, None),
    )
}

/// Row.qml's loading bar for a row the pane does not hold yet: 30 percent of the row, caption high, right of the mark slot.
pub fn loading_bar(
    left: Pixels,
    top: Pixels,
    metrics: &Metrics,
    row_width: f32,
    caption: f32,
) -> Bounds<Pixels> {
    Bounds::new(
        point(
            left + px(metrics.label_x()),
            top + px((metrics.row - caption) / 2.),
        ),
        size(px(row_width * LOADING_WIDTH), px(caption)),
    )
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
    let baseline =
        window.pixel_snap(top + (px(height) - line.ascent - line.descent) / 2. + line.ascent);
    let _ = window.paint_glyph_run(point(x, baseline), run, color);
}

fn paint_mark(
    key: SharedString,
    data: &'static [u8],
    at: gpui::Point<Pixels>,
    side: f32,
    ink: Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    let bounds = Bounds::new(at, size(px(side), px(side)));
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
        crate::visual::geometry::rows(&state, bounds, self.scroll_y, metrics.row, window);
        let held = &*self.held;
        let ListState {
            typeface,
            rows,
            visible,
            view,
            drawn,
            blank,
            drop_line,
            ..
        } = &mut *state;
        let shaping = Shaping {
            listing: held.listing,
            font: typeface.font.clone(),
            body: typeface.body,
            caption: typeface.caption,
            name_cells: cells(layout.name.width, advance.body),
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
        let first = (self.scroll_y / metrics.row).floor().max(0.) as usize;
        let last =
            (((self.scroll_y + height) / metrics.row).ceil().max(0.) as usize).min(held.total);
        rows.begin(
            &shaping,
            ((height / metrics.row).ceil() as usize).max(1) * CACHE_SCREENS,
        );
        visible.clear();
        let revision = view.mark_revision(held.listing);
        let mut loading = 0;
        for index in first..last.max(first) {
            let marks = self.states.row(index, held.listing);
            let Some(facts) = held.row(index) else {
                loading += 1;
                visible.push(Shown {
                    index,
                    prepared: None,
                    clip: None,
                    state: marks,
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
            let (prepared, extra, clip) =
                rows.get_or_prepare_marked(index, facts, revision, drop, resolve, |extra| {
                    prepare(facts, extra, &shaping, view, &layout, blank, window)
                });
            if marks.cursor && prepared.run.is_some() {
                prepared
                    .bold
                    .get_or_init(|| bold_name(facts, extra, &shaping, &view.query, window));
            }
            visible.push(Shown {
                index,
                prepared: Some(prepared),
                clip,
                state: marks,
            });
        }
        *drawn = super::Drawn {
            bounds,
            mask: bounds,
            layout,
            metrics,
            palette: Some(palette),
            scroll_y: self.scroll_y,
            listing: held.listing,
            frames: drawn.frames + 1,
            drop: drop_box.map(|(index, x, width, _)| {
                let top = bounds.origin.y + px(index as f32 * metrics.row - self.scroll_y);
                (
                    index,
                    Bounds::new(
                        point(bounds.origin.x + px(x), top),
                        size(px(width), px(metrics.row)),
                    ),
                )
            }),
            loading,
        };
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
        let state = self.state.borrow();
        let view = &state.view;
        let left = bounds.origin.x;
        let row = metrics.row;
        let top_of = |index: usize| bounds.origin.y + px(index as f32 * row - self.scroll_y);
        let focused = self.states.focused;
        window.paint_quad(fill(bounds, palette.background));
        window.with_content_mask(
            Some(ContentMask {
                bounds: state.drawn.mask,
            }),
            |window| {
                for shown in &state.visible {
                    let top = top_of(shown.index);
                    let ground = Bounds::new(point(left, top), size(bounds.size.width, px(row)));
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
                        Some(if focused {
                            palette.marked
                        } else {
                            palette.foreground.opacity(TRACK)
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
                    let (Some((start, end)), false) = (prepared.run, s.cursor) else {
                        continue;
                    };
                    let line = prepared.name.line();
                    let x = left + px(layout.name.x);
                    let limit = x + px(layout.name.width);
                    let height = line.ascent + line.descent;
                    let y = top + (px(row) - height) / 2.;
                    let from = (x + line.x_for_index(start)).min(limit);
                    let to = (x + line.x_for_index(end)).min(limit);
                    window.paint_quad(fill(
                        Bounds::from_corners(point(from, y), point(to, y + height)),
                        palette.accent.opacity(MATCH_WASH),
                    ));
                }
                // One layer for every mark and glyph, so the scene orders the whole list once rather than once a line.
                let (mut marks_painted, mut cells_painted) = (0, 0);
                window.paint_layer(bounds, |window| {
                    for shown in &state.visible {
                        let lifted = shown.state.lifted();
                        // drag-cut-move: a cut row ghosts its mark alone, so its name and cells keep their contrast.
                        let dim = if shown.clip == Some(ClipMark::Cut) {
                            CUT_DIM
                        } else {
                            1.
                        };
                        let top = top_of(shown.index);
                        let at = point(
                            left + px(metrics.pad + (metrics.slot - metrics.mark) / 2.),
                            top + px((row - metrics.mark) / 2.),
                        );
                        // Row.qml keeps Icons.FALLBACK, the file mark, beside a loading row's bar.
                        let mark = shown
                            .prepared
                            .as_ref()
                            .map_or(Mark::File, |prepared| prepared.mark);
                        let (key, data) = mark.svg();
                        let ink = if lifted {
                            palette.foreground
                        } else {
                            palette.metadata_frame
                        };
                        paint_mark(key, data, at, metrics.mark, ink.opacity(dim), window, cx);
                        marks_painted += 1;
                        let Some(prepared) = &shown.prepared else {
                            continue;
                        };
                        let meta = if lifted {
                            palette.foreground
                        } else {
                            palette.metadata
                        };
                        if shown.state.drop_target {
                            continue;
                        }
                        let recent = view
                            .today
                            .zip(self.held.row(shown.index))
                            .is_some_and(|(today, facts)| format::is_recent(facts.mtime, today));
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
                                Column::Size | Column::Date => {
                                    left + px(cell.right()) - line.line().width
                                }
                                Column::Mode | Column::Kind => left + px(cell.x),
                            };
                            let ink = if column == Column::Date && recent {
                                palette.foreground
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
                            let ink = palette.name(prepared.ink, shown.state.lifted());
                            let line = prepared
                                .bold
                                .get()
                                .filter(|_| shown.state.cursor)
                                .unwrap_or(&prepared.name);
                            paint_line(line, names.origin.x, top_of(shown.index), row, ink, window);
                        }
                    });
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
                    // RowDropTarget.qml: the clip mark sits a gap past the drawn name, one pixel above the row's centre line.
                    for shown in &state.visible {
                        let (Some(prepared), Some(clip)) = (&shown.prepared, shown.clip) else {
                            continue;
                        };
                        if shown.state.renaming || view.searching {
                            continue;
                        }
                        let drawn = prepared.name.line().width.min(px(layout.name.width));
                        let x = left + px(layout.name.x) + drawn + px(metrics.gap);
                        let y = top_of(shown.index) + px((row - metrics.clip) / 2. - 1.);
                        let (key, data) = clip.svg();
                        paint_mark(
                            key,
                            data,
                            point(x, y),
                            metrics.clip,
                            palette.muted_frame,
                            window,
                            cx,
                        );
                    }
                });
                state.painted_marks.set(marks_painted);
                state.painted_cells.set(cells_painted);
            },
        );
    }
}
