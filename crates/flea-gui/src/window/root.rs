//! The window root: the shell as one cached view, with each frame's pane probes and loading marks painted over it.
use crate::{feedback::spinner::Arm, pane::states::Kind, state::Shell};
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
        // The modal scrim and card share the highest deferred layer, above menus and pane marks.
        let overlay = self.shell.read(cx).overlay.clone().map(|view| {
            view.update(cx, |overlay, cx| overlay.sync_focus(window, cx));
            let priority = if view.read(cx).model.open() {
                usize::MAX
            } else {
                0
            };
            deferred(div().absolute().top_0().left_0().size_full().child(view))
                .with_priority(priority)
        });
        div()
            .size_full()
            .child(
                self.shell
                    .clone()
                    .cached(StyleRefinement::default().size_full()),
            )
            // Pane marks paint over the cached shell and below deferred surfaces.
            .child(Overlay(self.shell.clone()))
            .children(overlay)
    }
}

/// Paints after the shell has laid out, so a mark lands in this frame's slot inside its pane's clip; a crawl frame repaints only this.
struct Overlay(Entity<Shell>);

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
        _: &mut Window,
        _: &mut App,
    ) {
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
        cx.read_entity(&self.0, |shell, cx| {
            let arm = shell.seam.mark();
            for side in 0..2 {
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
                match arm {
                    Arm::Crawl => {
                        swap.mark
                            .paint(swap.crawling(cx.reduce_motion()), None, window, cx)
                    }
                    Arm::Pinned(offset) => swap.mark.paint(false, Some(offset), window, cx),
                    Arm::Hidden => {}
                }
            }
        });
    }
}
