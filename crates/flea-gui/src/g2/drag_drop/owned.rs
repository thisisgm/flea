use super::*;
use flea::ui_logic_file_ops::collide::{text, texts, Request};

// What waits for the facts: this window's own lift or an arrived native drop.
pub(super) enum Owned {
    Own { lift: Lift, ctrl: bool, shift: bool },
    Native(Arrived),
}

impl Owned {
    pub(super) fn sources(&self) -> Vec<String> {
        match self {
            Self::Own { lift, .. } => lift.paths.clone().unwrap_or_default(),
            Self::Native(arrived) => core::file_paths(&arrived.urls),
        }
    }

    /// A shelf drag redeems its token instead of re-reading its URIs (Drag.dropInto rule 4); the URIs only ask op1 first.
    pub(super) fn shelf_ask(&self, dest: &str) -> Option<(Request, Vec<String>)> {
        let Self::Native(arrived) = self else {
            return None;
        };
        let (token, _) = core::shelf(arrived.shelf.as_deref()?).filter(|_| !dest.is_empty())?;
        let request = Request::new()
            .with("c", &text("transfer"))
            .with("op", &text(""))
            .with("paths", &texts(&[]))
            .with("dest", &text(dest))
            .with("shelf", &text(token));
        Some((request, core::file_paths(&arrived.urls)))
    }

    pub(super) fn dropping(&self) -> Dropping<'_> {
        match self {
            Self::Own { lift, ctrl, shift } => Dropping::own(lift, *ctrl, *shift),
            Self::Native(arrived) => Dropping::native(
                arrived.marker.as_deref(),
                &arrived.urls,
                arrived.plain.as_deref(),
                arrived.ctrl,
                arrived.shift,
                arrived.action,
            ),
        }
    }
}

impl Shell {
    /// The window closes: its lift, pending paths and router registration go with it.
    pub fn forget_file_drags(&mut self, window: &Window, cx: &mut App) {
        let key = window_key(window);
        let drags = cx.default_global::<Drags>();
        drags.take(key);
        drags.said.remove(&key);
        drags.clients.remove(&key);
        drags.held.remove(&key);
        drags.aims.remove(&key);
        drags.rests.remove(&key);
        drags.hovers.remove(&key);
        drags.cue_lines.remove(&key);
        drags.asks.retain(|(owner, _), _| *owner != key);
        drags.waiting.retain(|(owner, _), _| *owner != key);
        if drags.cue.as_ref().is_some_and(|(owner, _)| *owner == key) {
            drags.cue = None;
        }
    }
}
