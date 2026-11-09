//! Empty-folder Omi slots, painted by the existing root overlay above the cached pane.
use super::hero::EmptyHero;
use crate::{
    columns::{geometry::Slot as TrackSlot, TrackState},
    feedback::spinner::Crawl,
    state::Shell,
    window::startup::View,
};
use gpui::{
    px, size, App, Bounds, Element, ElementId, Entity, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, Style, Window,
};
use std::rc::Rc;

/// Two pane heroes and five possible peek slots per pane, without a per-frame allocation.
pub(crate) fn marks(shell: &Shell, cx: &App) -> [Option<Rc<Crawl>>; 12] {
    heroes(shell, cx).map(|hero| {
        hero.map(|hero| Rc::clone(&hero.read(cx).omi))
            .filter(|mark| mark.shown())
    })
}

/// The heroes this frame draws, in the same slots as their marks; the root overlay paints their captions.
pub(crate) fn heroes(shell: &Shell, cx: &App) -> [Option<Entity<EmptyHero>>; 12] {
    let mut heroes = std::array::from_fn(|_| None);
    for side in 0..2 {
        let Some(host) = shell.shown_pane(side) else {
            continue;
        };
        if shell.slots.covers[side].is_some() {
            continue;
        }
        if let Some(hero) = shell.surfaces[side]
            .hero
            .as_ref()
            .filter(|_| shell.pane_state_view(side, cx).is_some())
        {
            if hero.read(cx).rotation().visible() {
                heroes[side * 6] = Some(hero.clone());
            }
        }
        if shell.shown_view() != View::Columns {
            continue;
        }
        let Some(columns) = &host.columns else {
            continue;
        };
        for track in columns
            .area
            .tracks()
            .iter()
            .filter(|track| track.slot != TrackSlot::Active && track.state == TrackState::Empty)
        {
            let slot = track.slot.index();
            if let Some(hero) = &columns.heroes[slot] {
                if hero.read(cx).rotation().visible() {
                    heroes[side * 6 + slot + 1] = Some(hero.clone());
                }
            }
        }
    }
    heroes
}

pub(super) struct Slot {
    pub mark: Rc<Crawl>,
    pub side: f32,
}

impl IntoElement for Slot {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Slot {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        Some(("omi-empty-slot", Rc::as_ptr(&self.mark) as u64).into())
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn a11y_role(&self) -> Option<gpui::Role> {
        Some(gpui::Role::Image)
    }
    fn write_a11y_info(&self, node: &mut gpui::accesskit::Node) {
        node.set_label(self.mark.label());
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let side = self.side;
        let style = Style {
            size: size(px(side).into(), px(side).into()),
            flex_shrink: 0.,
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
        _: &mut App,
    ) {
        self.mark.place(bounds, window);
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::g2::status_feedback::{
        empty_omi_tests::open,
        hero_tests::{frame, run},
    };

    #[gpui::test]
    fn empty_omi_hidden_and_reduced_lifecycle_does_no_background_work(
        cx: &mut gpui::TestAppContext,
    ) {
        let handle = open(cx, false);
        run(cx, handle, 11000);
        let mark = handle
            .update(cx, |root, _, cx| {
                let shell = root.shell.read(cx);
                Rc::clone(&shell.surfaces[0].hero.as_ref().unwrap().read(cx).omi)
            })
            .unwrap();
        let counts = || {
            (
                mark.counters.frames.get(),
                mark.counters.writes.get(),
                mark.counters.requests.get(),
            )
        };
        cx.simulate_window_visibility_change(handle.into(), gpui::WindowVisibility::Hidden);
        frame(cx, handle);
        let hidden = counts();
        assert_eq!(run(cx, handle, 1024), 0);
        assert_eq!(
            counts(),
            hidden,
            "hidden window must not sample, paint or request"
        );
        cx.update(|cx| cx.set_reduce_motion(true));
        cx.simulate_window_visibility_change(handle.into(), gpui::WindowVisibility::Visible);
        frame(cx, handle);
        let rest = counts();
        assert_eq!(mark.offset(), 0.);
        assert_eq!(run(cx, handle, 1024), 0);
        assert_eq!(
            counts(),
            rest,
            "reduced motion must stay idle after its rest paint"
        );
        handle
            .update(cx, |root, _, cx| {
                root.shell.update(cx, |shell, cx| {
                    shell.panes[0].as_mut().unwrap().pane.state.kind =
                        crate::pane::states::Kind::Ready;
                    cx.notify();
                });
            })
            .unwrap();
        frame(cx, handle);
        assert!(!mark.shown());
        let hidden = counts();
        assert_eq!(run(cx, handle, 1024), 0);
        assert_eq!(
            counts(),
            hidden,
            "hidden surface must not sample, paint or request"
        );
    }
}
