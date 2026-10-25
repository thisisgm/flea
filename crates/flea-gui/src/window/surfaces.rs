//! Pane state surfaces: the empty hero and the state message mount in a pane's state slot, built on first need.
use super::startup::View;
use crate::{
    g2::fb1::{
        hero::EmptyHero,
        state::{Facts, StateMessage, GRID, LIST},
    },
    pane::states::Kind,
    state::Shell,
};
use gpui::{AnyView, AppContext, Context, Entity};

/// A listing the backend refused, kept raw so its sentence translates again when the language changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub operation: String,
    pub message: String,
    pub mode: u32,
}

/// One pane's state surfaces, built the first time its pane shows the matching state and kept after.
#[derive(Default)]
pub(crate) struct Surfaces {
    pub(crate) footer: Option<Entity<super::status::Footer>>,
    pub(crate) footer_watch: Option<gpui::Subscription>,
    pub(crate) detail_height: f32,
    pub(crate) detail_text: gpui::SharedString,
    pub(crate) filesystem: super::status::Filesystem,
    pub(crate) hero: Option<Entity<EmptyHero>>,
    notice: Option<Entity<StateMessage>>,
    refusal: Option<Refusal>,
    shown: Option<(Kind, View, u64)>,
}

impl Surfaces {
    pub(crate) fn restyle(&self, face: &crate::list::Typeface, cx: &mut Context<Shell>) {
        if let Some(hero) = &self.hero {
            hero.update(cx, |hero, cx| {
                hero.set_typeface(face.clone(), cx);
                cx.notify();
            });
        }
        if let Some(notice) = &self.notice {
            notice.update(cx, |notice, cx| {
                notice.set_typeface(face.clone(), cx);
                cx.notify();
            });
        }
    }
    pub(crate) fn natural_width(&self, window: &gpui::Window, cx: &gpui::App) -> Option<f32> {
        match self.shown?.0 {
            Kind::Empty => self
                .hero
                .as_ref()
                .map(|hero| hero.read(cx).natural_width(window)),
            Kind::Locked | Kind::Error | Kind::Waiting => self
                .notice
                .as_ref()
                .map(|notice| notice.read(cx).natural_width(window)),
            _ => None,
        }
    }

    /// PaneStates' two layers as the last sync mounted them: the empty hero shown, and a state message with facts.
    pub(crate) fn layers(&self, cx: &gpui::App) -> (bool, bool) {
        let kind = self.shown.map(|(kind, _, _)| kind);
        let empty = kind == Some(Kind::Empty)
            && self
                .hero
                .as_ref()
                .is_some_and(|hero| hero.read(cx).rotation().visible());
        let message = matches!(kind, Some(Kind::Locked | Kind::Error | Kind::Waiting))
            && self
                .notice
                .as_ref()
                .is_some_and(|notice| notice.read(cx).facts().is_some());
        (empty, message)
    }

    /// A refused listing lands its message; the next sync redraws the state even when the kind stays put.
    pub(crate) fn refuse(&mut self, refusal: Refusal) {
        self.refusal = Some(refusal);
        self.shown = None;
    }
}

impl Shell {
    /// Cuts a side's hero away and unmounts its state surface.
    fn drop_surfaces(&mut self, side: usize, cx: &mut Context<Self>) {
        let surfaces = &mut self.surfaces[side];
        if let Some(hero) = &surfaces.hero {
            if hero.read(cx).rotation().visible() {
                hero.update(cx, |hero, cx| hero.hide(cx));
            }
        }
        surfaces.shown = None;
        self.slots.states[side] = None;
    }

    /// Mounts or drops one pane's state surface to match its state; a pane that shows no state builds nothing.
    pub(crate) fn sync_surfaces(&mut self, side: usize, cx: &mut Context<Self>) {
        // The pane the renderer draws on this side, so a pane left out of the frame keeps no caption running.
        if self.shown_pane(side).is_none() {
            self.drop_surfaces(side, cx);
            return;
        }
        let Some(host) = &self.panes[side] else {
            return;
        };
        let session = &host.search.owner.session;
        if session.live()
            && !session.running
            && !session.leaving
            && host.pane.held.total == 0
            && host.pane.state.search
        {
            self.drop_surfaces(side, cx);
            return;
        }
        let kind = host.pane.state.kind;
        let key = (kind, self.view, self.locale.revision);
        let surfaces = &mut self.surfaces[side];
        if surfaces.shown == Some(key) {
            return;
        }
        let entered = surfaces.shown.map(|(previous, _, _)| previous) != Some(kind);
        surfaces.shown = Some(key);
        if !matches!(kind, Kind::Locked | Kind::Error) {
            surfaces.refusal = None;
        }
        let catalog = self.locale.catalog.clone();
        let typeface = host.list.borrow().typeface.clone();
        let in_list = self.view == View::List;
        let metrics = if self.view == View::Grid { GRID } else { LIST };
        let facts = match kind {
            Kind::Waiting => Some(Facts::waiting(&catalog)),
            Kind::Locked | Kind::Error => surfaces.refusal.as_ref().map(|refusal| {
                Facts::failed(&refusal.operation, &refusal.message, refusal.mode, &catalog)
            }),
            _ => None,
        };
        let mut view: Option<AnyView> = None;
        if kind == Kind::Empty {
            let hero = surfaces
                .hero
                .get_or_insert_with(|| {
                    cx.new(|cx| {
                        let mut hero = EmptyHero::new(&catalog, cx);
                        hero.set_overlaid();
                        hero
                    })
                })
                .clone();
            hero.update(cx, |hero, cx| {
                hero.set_typeface(typeface.clone(), cx);
                hero.translated(&catalog);
                hero.set_in_list(in_list, cx);
                if entered {
                    hero.show(true, cx);
                }
            });
            view = Some(hero.into());
        } else if let Some(hero) = surfaces.hero.as_ref() {
            if entered {
                hero.update(cx, |hero, cx| hero.hide(cx));
            }
        }
        if let Some(facts) = facts {
            let notice = surfaces
                .notice
                .get_or_insert_with(|| cx.new(|_| StateMessage::new()))
                .clone();
            notice.update(cx, |notice, cx| {
                notice.set_typeface(typeface.clone(), cx);
                notice.set_metrics(metrics, cx);
                notice.set(Some(facts), cx);
            });
            view = Some(notice.into());
        } else if let Some(notice) = surfaces.notice.as_ref() {
            notice.update(cx, |notice, cx| notice.set(None, cx));
        }
        self.slots.states[side] = view;
    }
}

#[cfg(test)]
impl Surfaces {
    /// Entities built so far; a window that has not shown a state builds none.
    pub(crate) fn built(&self) -> usize {
        usize::from(self.hero.is_some()) + usize::from(self.notice.is_some())
    }
    /// The facts the mounted message draws, if it draws any.
    pub(crate) fn facts(&self, cx: &impl AppContext) -> Option<Facts> {
        self.notice
            .as_ref()
            .and_then(|notice| notice.read_with(cx, |notice, _| notice.facts().cloned()))
    }
}
