//! The window root: the shell as one cached view, with each frame's pane probes and loading marks painted over it.
use crate::{feedback::spinner::Arm, pane::states::Kind, state::Shell};
use gpui::{
    div, prelude::*, relative, size, App, Bounds, Context, Element, ElementId, Entity,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, Position, Render, Style,
    StyleRefinement, Window,
};

pub struct Root {
    pub shell: Entity<Shell>,
}

impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(
                self.shell
                    .clone()
                    .cached(StyleRefinement::default().size_full()),
            )
            .child(Overlay(self.shell.clone()))
    }
}

/// Paints after the shell has laid out, so a mark lands in this frame's slot; a crawl frame repaints only this.
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
