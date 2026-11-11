//! The rail's focus, Trash count and settings document, applied to its host and producers.
use super::*;

impl Shell {
    pub fn rail_focus(&mut self, focused: bool, cx: &mut Context<Self>) {
        let Some(join) = &self.rail else {
            return;
        };
        if focused && join.host.read(cx).rail.mode() == Mode::Hidden {
            return;
        }
        if let Some(join) = &mut self.rail {
            join.focus_pending = focused.then_some(true);
        }
        let join = self.rail.as_ref().expect("mounted rail");
        join.host.update(cx, |host, cx| host.focus(focused, cx));
        join.places.update(cx, |producer, cx| {
            producer.side = self.focus_side;
            producer.model.borrow_mut().focused = focused;
            if !focused {
                producer.model.borrow_mut().mark_focused = false;
                producer.cancel_rename(cx);
            }
            cx.notify();
        });
    }

    /// U32 owns the monitor and calls this with each accepted count; U08 only paints it.
    pub fn rail_trash_count(&mut self, count: u64, cx: &mut Context<Self>) {
        if let Some(join) = &self.rail {
            join.places.update(cx, |p, cx| p.trash_count(count, cx));
        }
    }

    /// The Trash count the rail holds and the detail text its Trash row draws, empty while the count is hidden.
    pub(crate) fn rail_trash_badge(&self, cx: &gpui::App) -> (u64, String) {
        let Some(join) = &self.rail else {
            return (0, String::new());
        };
        let model = join.places.read(cx).model.borrow();
        let text = if model.trash_count_shown && model.trash_count > 0 {
            model.trash_count.to_string()
        } else {
            String::new()
        };
        (model.trash_count, text)
    }

    /// U11's accepted document updates both placement and content atomically.
    pub fn rail_state(&mut self, state: &Json, cx: &mut Context<Self>) {
        self.launch.settings.data = state.clone();
        self.path_favourites(state, cx);
        if let Some(join) = &self.rail {
            let places = state.get("places").unwrap_or(&Json::Null);
            join.host.update(cx, |host, cx| {
                host.set_width(
                    geometry::width_stop(places.get("sidebarWidth").and_then(Json::as_f64)),
                    cx,
                );
                host.set_mode(Mode::from_places(places), cx);
            });
            join.devices
                .update(cx, |source, cx| source.adopt_state(state, cx));
            join.places
                .update(cx, |producer, cx| producer.adopt_state(state, cx));
        }
        self.rail_tail(cx);
        cx.notify();
    }
}
