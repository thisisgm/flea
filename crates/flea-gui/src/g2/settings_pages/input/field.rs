//! A shaped text leaf in the existing field chrome, also mounted invisibly while the key sheet awaits its first query.
use super::{session::SessionInput, Host};
use crate::leaf::{fill_parent, Leaf, Painted};
use crate::list::Typeface;
use crate::path::paint::paint_run;
use gpui::{
    div, fill, point, prelude::*, px, size, App, Bounds, ContentMask, CursorStyle, Div, Entity,
    EntityInputHandler, GlyphRun, Hitbox, HitboxBehavior, LayoutId, MouseButton, Pixels, Stateful,
    Window,
};
use std::rc::Rc;

pub(crate) struct Field<T>(Entity<T>, Typeface, bool);

impl<T: Host + EntityInputHandler> Field<T> {
    pub fn mount(view: Entity<T>, typeface: &Typeface, visible: bool) -> Stateful<Div> {
        let down = view.downgrade();
        let moved = view.downgrade();
        let menu = view.downgrade();
        let slot = crate::path::text_menu::Slot::new(
            view.clone(),
            super::editor_menu::<T>,
            super::text_menu_answer::<T>,
        );
        let field = div()
            .id("settings-text-input")
            .flex_1()
            .min_w_0()
            .h_full()
            .child(Painted(Self(view, typeface.clone(), visible)));
        if !visible {
            return field;
        }
        field
            .child(slot)
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                // A right press opens the field's text menu over the line as it stands, keeping the keyboard on the field.
                let _ = menu.update(cx, |view, cx| {
                    if view.draft().is_none() || view.editor().retired {
                        return;
                    }
                    window.focus(view.input_focus(window), cx);
                    view.sync_editor();
                    let catalog = view.input_catalog().clone();
                    let editor = view.editor_mut();
                    if editor.entry.marked.is_none() {
                        editor
                            .menu
                            .open(&editor.entry, &catalog, event.position, window, cx);
                    }
                    cx.stop_propagation();
                    cx.notify();
                });
            })
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                let _ = down.update(cx, |view, cx| {
                    if view.draft().is_none() || view.editor().retired {
                        return;
                    }
                    window.focus(view.input_focus(window), cx);
                    let at = index(view, event.position.x);
                    if event.modifiers.shift {
                        view.editor_mut().entry.select_to(at);
                    } else {
                        view.editor_mut().entry.move_to(at);
                    }
                    cx.notify();
                });
            })
            .on_mouse_move(move |event, _, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    let _ = moved.update(cx, |view, cx| {
                        if !view.editor().retired
                            && view.draft().is_some()
                            && view.editor().entry.marked.is_none()
                        {
                            let at = index(view, event.position.x);
                            view.editor_mut().entry.select_to(at);
                            cx.notify();
                        }
                    });
                }
            })
    }
}

fn index<T: Host>(view: &T, x: Pixels) -> usize {
    let editor = view.editor();
    let drawn = editor.drawn.borrow();
    let x = x - drawn.bounds.map_or(px(0.), |bounds| bounds.left());
    let units = crate::path::field::ime_index(&drawn.line, &editor.entry, x);
    editor.entry.offset_from_utf16(units)
}

impl<T: Host + EntityInputHandler> Leaf for Field<T> {
    type Prepaint = Option<Hitbox>;

    fn layout(&mut self, window: &mut Window, cx: &mut App) -> LayoutId {
        fill_parent(window, cx)
    }

    fn prepaint(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Hitbox> {
        let view = self.0.read(cx);
        let editor = view.editor();
        let entry = &editor.entry;
        let mut drawn = editor.drawn.borrow_mut();
        drawn.bounds = Some(bounds);
        if drawn.line.run.is_none()
            || drawn.line.revision != entry.revision
            || drawn.font.as_ref() != Some(&self.1.font)
            || drawn.pixels != self.1.body
            || drawn
                .catalog
                .as_ref()
                .is_none_or(|catalog| !std::sync::Arc::ptr_eq(catalog, view.input_catalog()))
        {
            let mut run = window.text_style().to_run(entry.text.len());
            run.font = self.1.font.clone();
            drawn.line.run = Some(Rc::new(GlyphRun::new(window.text_system().layout_line(
                &entry.text,
                self.1.body,
                &[run],
                None,
            ))));
            drawn.line.set_text_direction(&entry.text);
            drawn.line.revision = entry.revision;
            drawn.font = Some(self.1.font.clone());
            drawn.pixels = self.1.body;
            drawn.catalog = Some(view.input_catalog().clone());
        }
        let line = &mut drawn.line;
        let caret = line.entry_caret(entry, entry.cursor());
        let room = (bounds.size.width - px(2.)).max(px(0.));
        if caret - line.scroll > room {
            line.scroll = caret - room;
        } else if caret < line.scroll {
            line.scroll = caret;
        }
        let width = line.run.as_ref().map_or(px(0.), |run| run.line().width);
        line.scroll = line.scroll.min((width - room).max(px(0.))).max(px(0.));
        self.2
            .then(|| window.insert_hitbox(bounds, HitboxBehavior::Normal))
    }

    fn paint(
        &mut self,
        bounds: Bounds<Pixels>,
        hitbox: &mut Option<Hitbox>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (run, origin, selection, marked, caret, focus, session) = {
            let view = self.0.read(cx);
            let editor = view.editor();
            let entry = &editor.entry;
            let drawn = editor.drawn.borrow();
            let line = &drawn.line;
            let origin = bounds.left() - line.scroll;
            let band_top = bounds.top() + ((bounds.size.height - px(16.)) / 2.).floor();
            let band = |range: &std::ops::Range<usize>, height: Pixels, top: Pixels| {
                let (from, to) = line.entry_span(entry, range);
                Bounds::new(point(origin + from, top), size(to - from, height))
            };
            let active = !editor.retired
                && view.draft().is_some()
                && view.input_focus(window).is_focused(window);
            let selection = (active && !entry.selected.is_empty())
                .then(|| band(&entry.selected, px(16.), band_top));
            let marked = entry
                .marked
                .as_ref()
                .filter(|_| active)
                .map(|range| band(range, px(1.), band_top + px(15.)));
            let caret = (active && entry.selected.is_empty()).then(|| {
                Bounds::new(
                    point(
                        origin + line.entry_caret(entry, entry.cursor()) + px(1.),
                        band_top,
                    ),
                    size(px(1.), px(16.)),
                )
            });
            (
                line.run.clone(),
                origin,
                selection,
                marked,
                caret,
                view.input_focus(window).clone(),
                active.then(|| entry.session()).flatten(),
            )
        };
        let ink = window.text_style().color;
        if let Some(hitbox) = hitbox {
            window.set_cursor_style(CursorStyle::IBeam, hitbox);
        }
        if self.2 {
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                if let Some(selection) = selection {
                    window.paint_quad(fill(selection, ink.opacity(0.24)));
                }
                if let Some(run) = run {
                    paint_run(&run, origin, bounds.top(), bounds.size.height, ink, window);
                }
                if let Some(marked) = marked {
                    window.paint_quad(fill(marked, ink));
                }
                if let Some(caret) = caret {
                    window.paint_quad(fill(caret, ink));
                }
            });
        }
        if let Some(session) = session {
            window.handle_input(
                &focus,
                SessionInput::new(bounds, &self.0, session, focus.clone()),
                cx,
            );
        }
        let view = self.0.read(cx);
        let mut drawn = view.editor().drawn.borrow_mut();
        drawn.line.registered = session;
        #[cfg(test)]
        {
            drawn.marked = marked.filter(|_| self.2);
        }
    }
}
