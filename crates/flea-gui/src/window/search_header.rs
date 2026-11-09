//! The search strip and its pointer scope control.
use crate::{g2::file_search, state::Shell, window::startup::View};
use gpui::{div, prelude::*, Context, IntoElement, MouseButton, Window};
use std::rc::Rc;

impl Shell {
    /// The strip's height over Grid, which has no list header to take it; v0.3.12 GridArea keeps the tiles under it.
    pub(crate) fn grid_search_strip(&self, side: usize) -> f32 {
        self.panes[side]
            .as_ref()
            .filter(|host| self.view == View::Grid && host.search.owner.active())
            .map_or(0., |host| {
                crate::list::Metrics::for_typeface(&host.list.borrow().typeface).header
            })
    }

    pub(crate) fn search_header(
        &mut self,
        side: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<(gpui::AnyElement, f32)> {
        if (side == 1 && self.view != View::Dual) || self.slots.covers[side].is_some() {
            return None;
        }
        let input = self.search_input(side, cx);
        let host = self.panes[side].as_ref()?;
        if !host.search.owner.active() {
            return None;
        }
        let home = std::env::var("HOME").unwrap_or_default();
        let words = Rc::new(host.search.owner.words(
            &self.locale.catalog,
            &host.path,
            &home,
            false,
        ));
        let list = host.list.borrow();
        let mut strip = host.search.strip.borrow_mut();
        if strip.typeface != list.typeface {
            strip.typeface = list.typeface.clone();
            strip.prepared = None;
        }
        drop(strip);
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = file_search::strip::Palette::from_list(&crate::list::Palette::inherited(
            &window.text_style(),
            theme,
        ));
        let mut element =
            file_search::strip::StripElement::new(words, palette, host.search.strip.clone());
        if let Some(input) = input {
            element = element.with_input(input, self.focus.clone());
        }
        let height = crate::list::Metrics::for_typeface(&list.typeface).header;
        Some((
            div()
                .size_full()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |shell, event: &gpui::MouseDownEvent, window, cx| {
                        let Some(host) = &shell.panes[side] else {
                            return;
                        };
                        let strip = host.search.strip.borrow();
                        let Some(prepared) = &strip.prepared else {
                            return;
                        };
                        let x = f32::from(event.position.x) - shell.geometry(window).panes[side].x;
                        let scope = x >= prepared.boxes.scope_x
                            && x < prepared.boxes.scope_x + prepared.boxes.scope_w;
                        drop(strip);
                        shell.focus_pane(side, cx);
                        if scope {
                            shell.search_update(
                                side,
                                None,
                                |search, host| search.click_scope(host),
                                window,
                                cx,
                            );
                        }
                        window.focus(&shell.focus, cx);
                    }),
                )
                .child(element)
                .into_any_element(),
            height,
        ))
    }
}
