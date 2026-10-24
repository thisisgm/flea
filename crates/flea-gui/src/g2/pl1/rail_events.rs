//! Device replies enter the existing pane and status owners; unsupported hosts retain typed requests.
use super::*;

impl Shell {
    pub(super) fn route_device_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let device_events = self
            .rail
            .as_ref()
            .map(|join| join.devices.read(cx).drain_events())
            .unwrap_or_default();
        for request in device_events {
            use crate::g2::pl1::devices::Effect;
            use crate::g2::pl1::source::Event;
            match &request.event {
                Event::Device(Effect::Opened(path)) => {
                    let _ = self.step(request.side, Step::OpenPlace(path.clone()), window, cx);
                }
                Event::Device(Effect::Message(note, error)) => {
                    let text = crate::g2::pl1::words::say(&self.locale.catalog, note);
                    self.status_messages(cx, |messages, now| {
                        messages.say(
                            &text,
                            *error,
                            "",
                            flea::ui_logic_fb1::status::Hint::None,
                            now,
                        )
                    });
                }
                Event::Device(Effect::Operation { headline, detail }) => {
                    let text = headline
                        .as_ref()
                        .map(|note| crate::g2::pl1::words::say(&self.locale.catalog, note))
                        .unwrap_or_default();
                    self.status_messages(cx, |messages, now| {
                        messages.say(
                            &text,
                            true,
                            detail,
                            flea::ui_logic_fb1::status::Hint::None,
                            now,
                        )
                    });
                }
                _ => cx.emit(request),
            }
        }
    }
}
