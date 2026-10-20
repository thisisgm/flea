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
    hero: Option<Entity<EmptyHero>>,
    notice: Option<Entity<StateMessage>>,
    refusal: Option<Refusal>,
    shown: Option<(Kind, View, u64)>,
}

impl Surfaces {
    /// A refused listing lands its message; the next sync redraws the state even when the kind stays put.
    pub(crate) fn refuse(&mut self, refusal: Refusal) {
        self.refusal = Some(refusal);
        self.shown = None;
    }
}

impl Shell {
    /// Mounts or drops one pane's state surface to match its state; a pane that shows no state builds nothing.
    pub(crate) fn sync_surfaces(&mut self, side: usize, cx: &mut Context<Self>) {
        let Some(host) = &self.panes[side] else {
            return;
        };
        let kind = host.pane.state.kind;
        let key = (kind, self.view, self.locale.revision);
        let surfaces = &mut self.surfaces[side];
        if surfaces.shown == Some(key) {
            return;
        }
        let entered = surfaces.shown.map(|(previous, _, _)| previous) != Some(kind);
        surfaces.shown = Some(key);
        let catalog = self.locale.catalog.clone();
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
                .get_or_insert_with(|| cx.new(|cx| EmptyHero::new(&catalog, cx)))
                .clone();
            hero.update(cx, |hero, cx| {
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
}
