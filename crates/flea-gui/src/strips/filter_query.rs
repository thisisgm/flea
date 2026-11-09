//! The filter query shares cached glyphs and caret geometry with its platform adapter.
use super::*;
use crate::strips::query_input::{menu_press, place, SELECTION};

/// The query's cached glyphs and platform geometry, shared by its painter and input adapter.
#[derive(Default)]
pub(crate) struct QueryLine {
    pub line: Rc<RefCell<crate::path::field::Line>>,
    pub bounds: Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    key: Option<(SharedString, Hsla, Pixels)>,
    face: Option<gpui::Font>,
    scale: u32,
    pub(super) shapings: usize,
    clip: Option<Bounds<Pixels>>,
    pub(crate) marked: Option<Bounds<Pixels>>,
    selected: Option<Bounds<Pixels>>,
}

impl QueryLine {
    pub(super) fn get(
        &mut self,
        text: &SharedString,
        color: Hsla,
        size: Pixels,
        font: &gpui::Font,
        window: &Window,
    ) -> Pixels {
        let key = (text.clone(), color, size);
        if self.key.as_ref() != Some(&key)
            || self.face.as_ref() != Some(font)
            || self.scale != window.scale_factor().to_bits()
        {
            let run = TextRun {
                len: text.len(),
                font: font.clone(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let mut line = self.line.borrow_mut();
            line.run = Some(Rc::new(gpui::GlyphRun::new(
                window.text_system().layout_line(text, size, &[run], None),
            )));
            line.right_to_left = crate::path::entry::right_to_left(text);
            self.key = Some(key);
            self.face = Some(font.clone());
            self.scale = window.scale_factor().to_bits();
            self.shapings += 1;
        }
        self.line
            .borrow()
            .run
            .as_ref()
            .map_or(px(0.), |run| run.line().width)
    }

    pub(super) fn place(
        &mut self,
        query: Bounds<Pixels>,
        room: Pixels,
        entry: &crate::path::entry::Entry,
    ) -> Option<Bounds<Pixels>> {
        let band = Bounds::new(
            point(
                query.left(),
                query.top() + px((HEIGHT - RULE - CARET_H) / 2.),
            ),
            size(room, px(CARET_H)),
        );
        let placed = place(
            &mut self.line.borrow_mut(),
            entry,
            band,
            px(CARET_W),
            px(CARET_GAP),
        );
        self.bounds.set(Some(band));
        self.clip = Some(Bounds::new(query.origin, size(room, query.size.height)));
        self.selected = placed.selection;
        self.marked = placed.marked;
        placed.caret
    }

    /// The room the typed query owns, from its start to the summary's gap; a right press there opens its text menu.
    pub(crate) fn room(&self) -> Option<Bounds<Pixels>> {
        self.clip
    }

    /// The mirror unit: the drawn line from the field's start with its caret, preedit and selection, clipped to the field.
    pub(super) fn unit(&self, laid: Laid, editing: bool) -> Bounds<Pixels> {
        let whole = laid
            .caret
            .map_or(laid.query, |caret| laid.query.union(&caret));
        let Some(clip) = self.clip.filter(|_| editing) else {
            return whole;
        };
        let line = self.line.borrow();
        let width = line.run.as_ref().map_or(px(0.), |run| run.line().width);
        let mut right = laid.query.left() + width - line.scroll;
        for edge in [laid.caret, self.marked, self.selected]
            .into_iter()
            .flatten()
        {
            right = right.max(edge.right());
        }
        let width = (right.min(clip.right()) - laid.query.left()).max(px(0.));
        Bounds::new(laid.query.origin, size(width, laid.query.size.height))
    }

    pub(super) fn paint(
        &self,
        laid: Laid,
        palette: &Palette,
        input: Option<&gpui::Entity<crate::g2::listing_filter::input::FilterInput>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let active = input.is_none_or(|input| input.read(cx).focused(window, cx));
        let line = self.line.borrow();
        let paint = |window: &mut Window| {
            if active && input.is_some() {
                if let Some(selected) = self.selected {
                    window.paint_quad(fill(selected, palette.accent.opacity(SELECTION)));
                }
            }
            if let Some(run) = &line.run {
                let shaped = run.line();
                let y = window.pixel_snap(
                    laid.query.top()
                        + (px(HEIGHT - RULE) - shaped.ascent - shaped.descent) / 2.
                        + shaped.ascent,
                );
                let scroll = if input.is_some() { line.scroll } else { px(0.) };
                let _ = window.paint_glyph_run(
                    point(laid.query.left() - scroll, y),
                    run,
                    palette.foreground,
                );
            }
            if active {
                if input.is_some() {
                    if let Some(marked) = self.marked {
                        window.paint_quad(fill(marked, palette.foreground));
                    }
                }
                if let Some(caret) = laid.caret {
                    window.paint_quad(fill(caret, palette.accent));
                }
            }
        };
        if let Some(clip) = self.clip.filter(|_| input.is_some()) {
            window.with_content_mask(Some(gpui::ContentMask { bounds: clip }), paint);
        } else {
            paint(window);
        }
        drop(line);
        if let Some(input) = input {
            let adapter = input.read(cx);
            self.line.borrow_mut().registered = if active {
                adapter.entry.borrow().session()
            } else {
                None
            };
            if active {
                let (owner, side, session) = (
                    adapter.owner.clone(),
                    adapter.side,
                    adapter.entry.borrow().session(),
                );
                if let Some(shell) = owner.upgrade() {
                    window.handle_input(
                        &shell.read(cx).focus.clone(),
                        gpui::ElementInputHandler::new(laid.query, input.clone()),
                        cx,
                    );
                    crate::strips::query_input::on_press(input.clone(), laid.query, window);
                }
                // The same room Shell::text_line_at reads, so the pane menu and this press never both claim it.
                if let (Some(session), Some(room)) = (session, self.room()) {
                    menu_press(owner, side, session, room, window);
                }
            }
        }
    }
}
