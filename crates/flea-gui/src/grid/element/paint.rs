//! Prepaint picks the drawn tiles and their prepared captions; paint lays plates, then pictures, marks and glyphs in one layer.
use super::{Bold, Drawn, DropKind, GridElement, GridState, Ink};
use crate::grid::{
    captions,
    geometry::{Rect, CACHE_ROWS},
    layout::Layout,
    tile::{self, Shaping},
    Shown,
};
use gpui::{
    fill, outline, point, px, relative, size, App, BorderStyle, Bounds, ContentMask, Corners,
    Element, ElementId, GlobalElementId, InspectorElementId, LayoutId, Pixels, Style,
    TransformationMatrix, Window,
};
use std::rc::Rc;

// Board grid.html and Theme.qml: the cursor's 18% wash, the 35% mark plate, an 8% hover, the 14% match wash and a cut tile's 0.55.
const CURSOR_WASH: f32 = 0.18;
const MARK_PLATE: f32 = 0.35;
const HOVER: f32 = 0.08;
const MATCH_WASH: f32 = 0.14;
const CUT: f32 = 0.55;

#[path = "ground.rs"]
mod ground;
pub(super) use ground::bounds_of;
use ground::{baseline, caption_baseline, label_run, plate};

pub(super) fn caption_line<'a>(
    state: &'a GridState,
    drawn: &'a Drawn,
    at: usize,
) -> Option<&'a tile::Line> {
    let plain = drawn.tile.as_ref()?.lines.get(at)?;
    Some(
        state
            .bold
            .as_ref()
            .filter(|_| drawn.bold)
            .and_then(|bold| bold.lines[at].as_ref())
            .unwrap_or(plain),
    )
}

impl GridElement {
    fn prepare(&self, state: &mut GridState, bounds: Bounds<Pixels>, window: &Window) {
        let shown = self
            .shown
            .as_deref()
            .map_or(Shown::All(self.held.total), Shown::Only);
        let view = &self.view;
        let layout = Layout::new(
            view.metrics,
            (f32::from(bounds.size.width), f32::from(bounds.size.height)),
            view.thumb,
            view.density,
            shown.total(),
            view.rename_extra,
        );
        let scroll = layout.clamp(view.scroll);
        let caption_size = px(layout.metrics.body_small);
        let advance = state.advance(caption_size, window);
        let shaping = Shaping {
            listing: self.held.listing,
            font: state.font.clone(),
            size: caption_size,
            scale: window.scale_factor(),
        };
        let range = layout.drawn(scroll);
        let rows = range.len().div_ceil(layout.columns) + 2 * CACHE_ROWS;
        let GridState {
            tiles, bold, drawn, ..
        } = state;
        tiles.begin(&shaping, rows * layout.columns);
        drawn.clear();
        let line = layout.metrics.caption_line;
        let query = &self.overlay.query;
        for at in range {
            let Some(listing) = shown.at(at) else {
                continue;
            };
            let row = self.held.row(listing);
            let cell = layout.cell(scroll, at);
            let slot = layout.slot(cell);
            let clip = self.decor.clip_row(listing, row);
            let drop = self
                .overlay
                .drop
                .filter(|target| target.0 == listing)
                .map(|target| target.1);
            let lines = if drop.is_some() { 1 } else { 2 };
            let mut caption = layout.caption(cell, clip.is_some(), lines);
            let per_line = (caption.width / advance).floor().max(0.) as usize;
            let tile = match row {
                Some(facts) => {
                    Some(tiles.get_or_prepare(listing, facts, (per_line, lines), || {
                        tile::prepare_for(&self.overlay, facts, per_line, lines, &shaping, window)
                    }))
                }
                None => tiles.kept(listing),
            };
            let cursor = self.focus.cursor() == listing;
            let editing = self.overlay.renaming == Some(listing);
            let thumbnail = row.and_then(|row| {
                self.decor
                    .thumbnail(listing)
                    .filter(|thumb| thumb.identity.is(row))
            });
            let decoded = thumbnail.as_ref().map(|thumb| thumb.decoded());
            let marked = !query.is_empty() && !editing && tile.is_some();
            let washes = if marked && !cursor {
                tiles.washes(listing, query, caption.width, |cached| {
                    tile::washes(
                        cached,
                        &captions::caption_marks(&cached.caption, query),
                        caption.width,
                        line,
                    )
                })
            } else {
                [None, None]
            };
            if let Some(tile) = tile.as_ref().filter(|_| marked && cursor) {
                let kept = bold.as_ref().is_some_and(|held| {
                    held.listing == listing && &held.query == query && Rc::ptr_eq(&held.tile, tile)
                });
                if !kept {
                    let marks = captions::caption_marks(&tile.caption, query);
                    *bold = Some(Bold {
                        listing,
                        query: query.clone(),
                        tile: Rc::clone(tile),
                        lines: tile::bold(tile, &marks, &shaping, window),
                    });
                }
            }
            let drawn_lines = tile.as_ref().map_or(0, |tile| tile.lines.len().min(lines));
            let widest = tile
                .as_ref()
                .map_or(0., |tile| tile.widest)
                .min(caption.width);
            let painted_height = tile
                .as_ref()
                .map_or(0., |tile| tile.text_height(drawn_lines, line));
            caption.height = caption.height.max(painted_height.ceil());
            drawn.push(Drawn {
                view: at,
                listing,
                cell,
                slot,
                caption,
                painted: Rect {
                    x: caption.x + (caption.width - widest) / 2.,
                    y: caption.y,
                    width: widest,
                    height: painted_height,
                },
                picture: decoded.map(|decoded| {
                    crate::grid::element::pictures::picture_bounds(
                        self.decor.pictures(),
                        listing,
                        thumbnail.as_ref().map(|thumb| thumb.kind),
                        slot,
                        decoded,
                        window,
                    )
                }),
                decoded,
                kind: thumbnail.as_ref().map(|thumb| thumb.kind),
                cursor,
                selected: self.focus.selected(listing),
                hovered: view.hovered == Some(at) && self.focus.hover_shown(),
                editing,
                drop,
                lines: drawn_lines,
                washes,
                language: row
                    .filter(|_| thumbnail.is_none())
                    .and_then(|row| self.decor.language(row)),
                image: thumbnail.map(|thumb| thumb.image.clone()),
                tile,
                clip,
                bold: marked && cursor,
            });
        }
        if !drawn.iter().any(|drawn| drawn.bold) {
            *bold = None;
        }
        // GridTile.qml nameTip: only on the hovered tile, not editing, not a drop target, and elided.
        self.prepare_hover(state, layout, scroll, bounds);
        let shown_tip = state.drawn.iter().find(|drawn| {
            drawn.hovered && !drawn.editing && drawn.drop.is_none() && drawn.elided()
        });
        let kept = state.tip.take();
        state.tip = shown_tip.and_then(|drawn| {
            let name = self.held.row(drawn.listing).map(|row| row.name.as_str())?;
            Some(super::tip::prepare(
                kept,
                drawn,
                name,
                &layout.metrics,
                &state.font,
                window,
            ))
        });
    }

    fn paint_tile(
        &self,
        state: &GridState,
        drawn: &Drawn,
        origin: gpui::Point<Pixels>,
        ink: &Ink,
        window: &mut Window,
        cx: &App,
    ) {
        let layout = state.layout.as_ref().expect("prepaint laid the grid out");
        let cut = drawn.clip.as_ref().is_some_and(|clip| clip.cut);
        let dim = if cut { CUT } else { 1. };
        if let (Some(image), Some(picture)) = (&drawn.image, drawn.picture) {
            let at = bounds_of(origin, picture);
            let painted = window
                .paint_image(at, at, Corners::default(), image.clone(), 0, false)
                .is_ok();
            if let Some(pictures) = self.decor.pictures() {
                pictures.record_paint(drawn.listing, at, window, painted);
            }
        } else {
            let (key, data) = drawn.language.clone().unwrap_or_else(|| {
                drawn
                    .tile
                    .as_ref()
                    .map_or(crate::row::marks::Mark::File, |tile| tile.mark)
                    .svg()
            });
            let mark_ink = if drawn.cursor || drawn.selected {
                ink.foreground
            } else {
                ink.muted_frame
            };
            let _ = window.paint_svg(
                bounds_of(origin, layout.mark(drawn.slot)),
                key,
                Some(data),
                TransformationMatrix::unit(),
                mark_ink.opacity(dim),
                cx,
            );
        }
        let Some(tile) = drawn.tile.as_ref().filter(|_| !drawn.editing) else {
            return;
        };
        let line = layout.metrics.caption_line;
        let caption = bounds_of(origin, drawn.caption);
        window.with_content_mask(Some(ContentMask { bounds: caption }), |window| {
            for at in 0..tile.lines.len().min(drawn.lines) {
                let shaped = caption_line(state, drawn, at).expect("prepared caption line");
                let (run, centred) = (&shaped.run, shaped.centred);
                let top = caption.origin.y + px(at as f32 * line);
                let x = caption.origin.x + px((drawn.caption.width - centred) / 2.);
                let _ = window.paint_glyph_run(
                    point(x, caption_baseline(run, top, window)),
                    run,
                    crate::window::glass::producers::text_ink(ink.foreground, window, cx),
                );
            }
        });
        super::tags::paint(self.held.row(drawn.listing), drawn, origin, window, cx);
        if let (Some(clip), Some(mark)) = (&drawn.clip, drawn.clip_bounds(state)) {
            #[cfg(test)]
            crate::bfx_cols_tests::record_mark(&clip.key, bounds_of(origin, mark));
            let _ = window.paint_svg(
                bounds_of(origin, mark),
                clip.key.clone(),
                Some(clip.svg),
                TransformationMatrix::unit(),
                ink.muted_frame,
                cx,
            );
        }
        if let (Some(kind), Some(labels)) = (drawn.drop, state.shaped_labels.as_ref()) {
            let run = &labels[kind as usize];
            let top = caption.origin.y + px(line);
            let x = caption.origin.x + (caption.size.width - run.line().width) / 2.;
            let _ =
                window.paint_glyph_run(point(x, baseline(run, top, line, window)), run, ink.accent);
        }
    }
}

impl Element for GridElement {
    type RequestLayoutState = ();
    type PrepaintState = Ink;

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
    ) -> Ink {
        let mut state = self.state.borrow_mut();
        self.prepare(&mut state, bounds, window);
        if state.shaped_labels.is_none() && state.drawn.iter().any(|drawn| drawn.drop.is_some()) {
            let size = px(state
                .layout
                .as_ref()
                .map_or(12., |layout| layout.metrics.caption));
            let [move_, copy, link] = [DropKind::Move, DropKind::Copy, DropKind::Link]
                .map(|kind| label_run(state.label(kind), &state, size, window));
            state.shaped_labels = Some([move_, copy, link]);
        }
        state.frame_held = Some(Rc::clone(&self.held));
        state.record_frame(bounds, Ink::for_window(window, cx))
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        ink: &mut Ink,
        window: &mut Window,
        cx: &mut App,
    ) {
        let ink = *ink;
        let mut state = self.state.borrow_mut();
        state.frames += 1;
        let state = &*state;
        let origin = bounds.origin;
        crate::window::glass::producers::paint(
            crate::window::glass::producers::Producer::Listing,
            bounds,
            ink.background,
            window,
            cx,
        );
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for drawn in &state.drawn {
                let frame = bounds_of(
                    origin,
                    state
                        .layout
                        .as_ref()
                        .map_or(drawn.cell, |layout| layout.frame(drawn.cell)),
                );
                if let Some(ground) = plate(
                    drawn,
                    &ink,
                    self.focus.focused(),
                    self.focus.selection_fill(),
                ) {
                    window.paint_quad(fill(frame, ground));
                }
                if drawn.cursor && self.focus.focused() {
                    window.paint_quad(outline(frame, ink.accent_frame, BorderStyle::Solid));
                }
                if drawn.drop.is_some() {
                    let cell = bounds_of(origin, drawn.cell);
                    super::drop::paint(cell, &ink, window);
                }
                for wash in drawn.washes.iter().flatten() {
                    let at = Rect {
                        x: drawn.caption.x + wash.x,
                        y: drawn.caption.y + wash.y,
                        ..*wash
                    };
                    window.paint_quad(fill(bounds_of(origin, at), ink.accent.opacity(MATCH_WASH)));
                }
            }
            // One layer for every picture, mark and glyph, so the scene orders the whole grid once rather than once a tile.
            window.paint_layer(bounds, |window| {
                for drawn in &state.drawn {
                    self.paint_tile(state, drawn, origin, &ink, window, cx);
                }
            });
            if let Some(tip) = &state.tip {
                let plate = bounds_of(origin, tip.rect);
                window.paint_layer(plate, |window| {
                    window.paint_quad(fill(plate, ink.background));
                    window.paint_quad(outline(plate, ink.muted_frame, BorderStyle::Solid));
                    let gap = state
                        .layout
                        .as_ref()
                        .map_or(0., |layout| layout.metrics.gap);
                    for (at, run) in tip.lines.iter().enumerate() {
                        let top = plate.origin.y + px(gap / 2. + at as f32 * tip.line);
                        let _ = window.paint_glyph_run(
                            point(
                                plate.origin.x + px(gap),
                                baseline(run, top, tip.line, window),
                            ),
                            run,
                            ink.foreground,
                        );
                    }
                });
            }
            // A cut picture fades toward its own ground, which the pane exposes no opacity for; marks and glyphs fade by ink.
            for drawn in state.drawn.iter().filter(|drawn| {
                drawn.picture.is_some() && drawn.clip.as_ref().is_some_and(|clip| clip.cut)
            }) {
                let ground = plate(
                    drawn,
                    &ink,
                    self.focus.focused(),
                    self.focus.selection_fill(),
                )
                .map_or(ink.background, |plate| ink.background.blend(plate));
                let slot = bounds_of(origin, drawn.slot);
                window.paint_layer(slot, |window| {
                    window.paint_quad(fill(slot, ground.opacity(1. - CUT)))
                });
            }
        });
    }
}
