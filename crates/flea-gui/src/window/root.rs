//! The window root: the shell as one cached view, with each frame's pane probes, loading marks and hero captions painted over it.
use crate::{feedback::spinner::Arm, palette::Theme, pane::states::Kind, state::Shell};
use gpui::{
    deferred, div, point, prelude::*, relative, size, App, Bounds, ContentMask, Context, Element,
    ElementId, Entity, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels,
    Position, Render, Style, StyleRefinement, Window,
};

pub struct Root {
    pub shell: Entity<Shell>,
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        super::frame_feedback::perf_tests::root_built();
        self.shell.update(cx, |shell, cx| {
            // Root renders first in every frame, so the whole frame paints in one direction.
            window.set_layout_mirrored(shell.mirrored());
            shell.presentation.activity.draws += 1;
            shell.sync_font_settings(cx);
            shell.observe_operations(window, cx);
            shell.observe_output(window, cx);
            shell.focus_operation_mount(window, cx);
            // A field that lost focus since the last key lets go of its composition before this frame draws it.
            shell.sync_text_input(window, cx);
            // A plugin action's own input card keeps the menu's validated capture leased until it closes.
            if shell.modal_card_holds_keyboard(cx)
                && shell.plugins.input.is_none()
                && shell.menu.captured.is_some()
            {
                shell.retire_menu(window, cx);
            }
            shell.menu_frame(window, cx);
        });
        let menu = crate::g2::context_menus::host_input::layer(&self.shell, window, cx);
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
        // Quick Look draws over the panes and menus, under the operations card; a closed card mounts nothing.
        let look = self
            .shell
            .read(cx)
            .quick_look
            .as_ref()
            .map(|host| host.view.clone())
            .filter(|view| view.read(cx).visible())
            .map(|view| deferred(div().absolute().top_0().left_0().size_full().child(view)));
        // The output card draws above everything while it shows; a closed card mounts nothing.
        let output = self
            .shell
            .read(cx)
            .output
            .view
            .clone()
            .filter(|view| view.read(cx).model.card.is_some())
            .map(|view| {
                deferred(div().absolute().top_0().left_0().size_full().child(view))
                    .with_priority(usize::MAX)
            });
        // The bulk rename card takes the same highest layer while it shows.
        let bulk = self
            .shell
            .update(cx, |shell, cx| shell.bulk_rename_layer(window, cx))
            .map(|layer| {
                deferred(div().absolute().top_0().left_0().size_full().child(layer))
                    .with_priority(usize::MAX)
            });
        // A plugin action's input card takes the same highest layer while it shows.
        let plugin_input = self
            .shell
            .update(cx, |shell, cx| shell.plugin_input_layer(window, cx))
            .map(|layer| {
                deferred(div().absolute().top_0().left_0().size_full().child(layer))
                    .with_priority(usize::MAX)
            });
        // The Properties card takes the same highest layer while it shows.
        let properties = self
            .shell
            .update(cx, |shell, cx| shell.properties_layer(window, cx))
            .map(|layer| {
                deferred(div().absolute().top_0().left_0().size_full().child(layer))
                    .with_priority(usize::MAX)
            });
        // The run card takes the same highest layer while it shows.
        let run = self
            .shell
            .update(cx, |shell, cx| shell.run_card_layer(window, cx))
            .map(|layer| {
                deferred(div().absolute().top_0().left_0().size_full().child(layer))
                    .with_priority(usize::MAX)
            });
        // The Open With card takes the same highest layer while it shows.
        let open_with = self
            .shell
            .update(cx, |shell, cx| shell.open_with_layer(window, cx))
            .map(|layer| {
                deferred(div().absolute().top_0().left_0().size_full().child(layer))
                    .with_priority(usize::MAX)
            });
        // The Permissions card takes the same highest layer while it shows.
        let permissions = self
            .shell
            .read(cx)
            .permissions
            .view
            .clone()
            .filter(|view| view.read(cx).opened)
            .map(|view| {
                view.update(cx, |view, cx| {
                    view.corner = self.shell.read(cx).corner;
                    view.catalog = self.shell.read(cx).locale.catalog.clone();
                });
                deferred(div().absolute().top_0().left_0().size_full().child(view))
                    .with_priority(usize::MAX)
            });
        // The archive cards and the flash view take the same highest layer while one shows.
        let archive = self
            .shell
            .update(cx, |shell, cx| shell.archive_layer(window, cx))
            .map(|layer| {
                deferred(div().absolute().top_0().left_0().size_full().child(layer))
                    .with_priority(usize::MAX)
            });
        #[cfg(test)]
        let audit = crate::theme_gate::frame::probe(self.shell.clone(), cx);
        #[cfg(not(test))]
        let audit: Option<gpui::AnyElement> = None;
        let shell = self.shell.clone();
        div()
            .size_full()
            .capture_any_mouse_down(move |event, window, cx| {
                if event.button == gpui::MouseButton::Right {
                    shell.update(cx, |shell, cx| {
                        // A typing line takes its own right press before the pane menu captures it.
                        if shell.text_line_at(event.position) {
                            return;
                        }
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
            .child(crate::window::chrome_move::observer(self.shell.clone()))
            // Pane marks paint over the cached shell and below deferred surfaces.
            .child(Overlay(self.shell.clone(), false))
            .children(menu)
            .child(Overlay(self.shell.clone(), true))
            .children(look)
            .children(overlay)
            .children(output)
            .children(bulk)
            .children(plugin_input)
            .children(properties)
            .children(run)
            .children(open_with)
            .children(permissions)
            .children(archive)
            .children(audit)
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
        let empty_marks = cx.read_entity(&self.0, crate::g2::status_feedback::empty_omi::marks);
        for mark in empty_marks.into_iter().flatten() {
            mark.prepare(!cx.reduce_motion(), None, window, cx);
        }
        let heroes = cx.read_entity(&self.0, crate::g2::status_feedback::empty_omi::heroes);
        for hero in heroes.into_iter().flatten() {
            hero.update(cx, |hero, cx| hero.prepare_caption(window, cx));
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
        // PaneRail.qml: an overlay rail hides the rows under it the way a menu does, so pane marks stop at its edge.
        let uncovered = cx
            .read_entity(&self.0, |shell, cx| {
                shell
                    .rail
                    .as_ref()
                    .and_then(|join| join.host.read(cx).cover())
            })
            .map(|cover| ContentMask {
                bounds: Bounds::from_corners(
                    point(cover.right(), Pixels::ZERO),
                    point(window.viewport_size().width, window.viewport_size().height),
                ),
            });
        window.with_content_mask(uncovered, |window| self.paint_marks(window, cx));
    }
}

impl Overlay {
    // The pane marks and the frame feedback bookkeeping, inside whatever mask the caller set.
    fn paint_marks(&self, window: &mut Window, cx: &mut App) {
        cx.read_entity(&self.0, |shell, cx| {
            for mark in crate::g2::status_feedback::empty_omi::marks(shell, cx)
                .into_iter()
                .flatten()
            {
                mark.paint(window);
            }
            for hero in crate::g2::status_feedback::empty_omi::heroes(shell, cx)
                .into_iter()
                .flatten()
            {
                hero.read(cx).paint_caption(window);
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
