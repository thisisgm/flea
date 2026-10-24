//! The window root: the shell as one cached view, with each frame's pane probes and loading marks painted over it.
use crate::{feedback::spinner::Arm, palette::Theme, pane::states::Kind, state::Shell};
use gpui::{
    deferred, div, prelude::*, relative, size, App, Bounds, Context, Element, ElementId, Entity,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, Position, Render, Style,
    StyleRefinement, Window,
};

pub struct Root {
    pub shell: Entity<Shell>,
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.shell.update(cx, |shell, cx| {
            shell.sync_font_settings(cx);
            shell.observe_operations(window, cx);
            shell.focus_operation_mount(window, cx);
            if shell.card_holds_keyboard(cx) && shell.menu.captured.is_some() {
                shell.retire_menu(window, cx);
            }
            shell.menu_frame(window, cx);
        });
        let menu = crate::g2::mn1::host_input::layer(&self.shell, window, cx);
        let typeface = self.shell.read(cx).active_typeface();
        let foreground = cx
            .try_global::<Theme>()
            .copied()
            .unwrap_or_default()
            .foreground;
        // The modal scrim and card share the highest deferred layer, above menus and pane marks.
        let overlay = self.shell.read(cx).overlay.clone();
        #[cfg(test)]
        super::gcomp_tests::operations::capture(overlay.as_ref().map(Entity::entity_id), cx);
        let overlay = overlay.map(|view| {
            view.update(cx, |overlay, cx| overlay.sync_focus(window, cx));
            let priority = if view.read(cx).modal(cx) {
                usize::MAX
            } else {
                0
            };
            deferred(div().absolute().top_0().left_0().size_full().child(view))
                .with_priority(priority)
        });
        let shell = self.shell.clone();
        div()
            .size_full()
            .capture_any_mouse_down(move |event, window, cx| {
                if event.button == gpui::MouseButton::Right {
                    shell.update(cx, |shell, cx| {
                        if shell.shown_view() != crate::window::startup::View::Columns {
                            for side in 0..2 {
                                shell.menu_pointer_open(side, event, window, cx);
                            }
                        }
                    });
                }
            })
            .font_family(typeface.font.family)
            .text_size(typeface.body)
            .text_color(foreground)
            .child(
                self.shell
                    .clone()
                    .cached(StyleRefinement::default().size_full()),
            )
            // Pane marks paint over the cached shell and below deferred surfaces.
            .child(Overlay(self.shell.clone(), false))
            .children(menu)
            .child(Overlay(self.shell.clone(), true))
            .children(overlay)
    }
}

/// Paints after the shell has laid out, so a mark lands in this frame's slot inside its pane's clip; a crawl frame repaints only this.
struct Overlay(Entity<Shell>, bool);

impl IntoElement for Overlay {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Overlay {
    type RequestLayoutState = ();
    type PrepaintState = ();

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
            position: Position::Absolute,
            size: size(relative(1.).into(), relative(1.).into()),
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.1 {
            let mark = cx.read_entity(&self.0, |shell, _| {
                std::rc::Rc::clone(&shell.menu.presentation.mark)
            });
            mark.prepare(!cx.reduce_motion(), None, window, cx);
            return;
        }
        let empty_marks = cx.read_entity(&self.0, crate::g2::fb1::empty_omi::marks);
        for mark in empty_marks.into_iter().flatten() {
            mark.prepare(!cx.reduce_motion(), None, window, cx);
        }
        let column_marks =
            cx.read_entity(&self.0, |shell, cx| shell.column_loading_marks(window, cx));
        for mark in column_marks.into_iter().flatten() {
            mark.prepare(!cx.reduce_motion(), None, window, cx);
        }
        let (arm, marks) = cx.read_entity(&self.0, |shell, cx| {
            (
                shell.seam.mark(),
                [0, 1].map(|side| {
                    shell
                        .shown_pane(side)
                        .filter(|_| shell.slots.covers[side].is_none())
                        .map(|host| {
                            (
                                std::rc::Rc::clone(&host.pane.swap.mark),
                                host.pane.swap.crawling(cx.reduce_motion()),
                            )
                        })
                }),
            )
        });
        for (mark, moving) in marks.into_iter().flatten() {
            match arm {
                Arm::Crawl => mark.prepare(moving, None, window, cx),
                Arm::Pinned(seconds) => mark.prepare(false, Some(seconds), window, cx),
                Arm::Hidden => {}
            }
        }
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.1 {
            cx.read_entity(&self.0, |shell, _| {
                shell.menu.presentation.mark.paint(window)
            });
            #[cfg(test)]
            super::gcomp_tests::operations::submit(&self.0, window, cx);
            return;
        }
        cx.read_entity(&self.0, |shell, cx| {
            for mark in crate::g2::fb1::empty_omi::marks(shell, cx)
                .into_iter()
                .flatten()
            {
                mark.paint(window);
            }
            shell.paint_column_loading(window, cx);
            let arm = shell.seam.mark();
            for side in 0..2 {
                if shell.slots.covers[side].is_some() {
                    continue;
                }
                let Some(host) = shell.shown_pane(side) else {
                    continue;
                };
                let pane = &host.pane;
                let swap = &pane.swap;
                swap.frames.record(
                    pane.busy(),
                    pane.state.kind == Kind::Loading,
                    swap.fell_back,
                );
                if arm != Arm::Hidden {
                    swap.mark.paint(window);
                }
            }
            let eligible = [0, 1].map(|side| shell.feedback_eligible(side, window, cx));
            for (side, &eligible) in eligible.iter().enumerate() {
                if let Some(host) = &shell.panes[side] {
                    host.pane.swap.observed.borrow_mut().available =
                        eligible && shell.presentation.observer.is_some();
                }
            }
            shell.presentation.painted(window, eligible);
        });
    }
}
