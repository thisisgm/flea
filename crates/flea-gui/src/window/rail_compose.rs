//! Compose the accepted U08 producer with this window's existing state and focus owners.
use super::*;

impl Shell {
    pub(crate) fn rail_locale(&mut self, catalog: &Arc<I18n>, cx: &mut Context<Self>) {
        if let Some(join) = &self.rail {
            join.places.update(cx, |producer, cx| {
                producer.set_locale(Arc::clone(catalog), cx);
            });
        }
    }

    /// The same mount is called by production construction and the mounted regression fixture.
    pub(crate) fn mount_rail(&mut self, cx: &mut Context<Self>) {
        if self.rail.is_some() {
            return;
        }
        let join = mount(
            Arc::clone(&self.locale.catalog),
            &self.launch.settings.data,
            self.focus_side,
            cx,
        );
        self.slots.rail = Some(AnyView::from(join.host.clone()));
        self.rail = Some(join);
    }

    pub(crate) fn sync_rail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(join) = &self.rail else {
            return;
        };
        if join.host.read(cx).rail.mode() == Mode::Hidden {
            self.input.remove("rail");
            if self.dual_focus.rail.contains_focused(window, cx) {
                window.focus(&self.focus, cx);
                self.keyboard.context = flea::keys::Context::Listing;
            }
            return;
        }
        self.input.register(crate::input::focus::Stop {
            name: "rail",
            handle: self.dual_focus.rail.clone(),
            context: flea::keys::Context::Rail,
        });
        let held = self.dual_focus.rail.contains_focused(window, cx);
        let changed = join.host.read(cx).rail.wants().focus != held;
        if changed {
            join.host.update(cx, |host, cx| host.focus(held, cx));
        }
        if !join.host.read(cx).rail.shown() {
            return;
        }
        let face = self.active_typeface();
        join.places.update(cx, |producer, cx| {
            producer.side = self.focus_side;
            let mut model = producer.model.borrow_mut();
            let changed = model.focused != held || producer.typeface != face;
            model.focused = held;
            model.corner = self.corner;
            drop(model);
            if !held {
                producer.cancel_rename(cx);
            }
            if changed {
                producer.typeface = face;
                cx.notify();
            }
        });
    }
}

#[cfg(test)]
#[path = "rail_compose_tests.rs"]
mod tests;
