//! Cache actual compiled-map samples by layout generation and requested modifier/Caps state.
use flea::keys::{
    self,
    user::{Projection, Sample},
};
use gpui::{Capslock, KeyDownEvent, Keystroke, Modifiers};

#[derive(Default)]
pub(crate) struct Cache {
    identity: Option<String>,
    attempted: Vec<u8>,
    pub(crate) projection: Projection,
}

impl Cache {
    pub(crate) fn current(&self, identity: &str) -> bool {
        self.identity.as_deref() == Some(identity)
    }

    pub(crate) fn refresh(
        &mut self,
        identity: &str,
        masks: impl IntoIterator<Item = u8>,
        mut samples: impl FnMut(Modifiers, Capslock) -> Option<Vec<(Keystroke, Modifiers)>>,
    ) {
        if !self.current(identity) {
            self.identity = Some(identity.into());
            self.attempted.clear();
            self.projection = Projection::default();
        }
        for mask in masks {
            if self.attempted.contains(&mask) {
                continue;
            }
            self.attempted.push(mask);
            let modifiers = Modifiers {
                control: mask & keys::CONTROL != 0,
                shift: mask & keys::SHIFT != 0,
                alt: mask & keys::ALT != 0,
                platform: mask & keys::SUPER != 0,
                ..Default::default()
            };
            let states = [false, true].map(|on| samples(modifiers, Capslock { on }));
            let [Some(base), Some(caps)] = states else {
                continue;
            };
            self.projection.modifiers.push(mask);
            for (keystroke, held) in base.into_iter().chain(caps) {
                // These raw candidates analyze conflicts only; native dispatch keeps its composition guards.
                let event = KeyDownEvent {
                    keystroke,
                    is_held: false,
                    prefer_character_input: false,
                };
                if let Some(sample) = Sample::new(super::super::input::event_inputs(&event, held)) {
                    self.projection.samples.push(sample);
                }
            }
        }
    }
}

impl super::Join {
    pub(crate) fn resolve(&mut self, preset: &str) {
        self.facts.resolve(preset, &self.layout.projection);
    }
}

impl crate::state::Shell {
    pub(crate) fn refresh_user_keys_layout(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.user_keys.layout.current(cx.keyboard_layout().id()) {
            self.user_keys_changed(window, cx);
        }
    }

    pub(super) fn project_user_keys(&mut self, cx: &gpui::Context<Self>) {
        let mapper = cx.keyboard_mapper().clone();
        let masks = self
            .user_keys
            .facts
            .keys
            .bindings
            .iter()
            .flat_map(|binding| binding.required_modifiers());
        self.user_keys
            .layout
            .refresh(cx.keyboard_layout().id(), masks, |modifiers, caps| {
                mapper.shortcut_samples(modifiers, caps)
            });
    }
}
