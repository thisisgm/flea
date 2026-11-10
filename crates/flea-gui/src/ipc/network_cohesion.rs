//! The network card reports the actual hit boxes, logical focus and pointer owner.
use super::{path_label, protocol_name, Focus, NetworkAdapter};
use flea::jsondoc::Json;

impl NetworkAdapter<'_> {
    pub(super) fn cohesion(&self) -> Option<String> {
        let card = self.open()?;
        let drawn = card.drawn.borrow();
        let entries = drawn
            .hits
            .iter()
            .filter_map(|(focus, _)| {
                let (x, y, w, h) = self.hit(card, *focus)?;
                let (kind, name, value) = match *focus {
                    Focus::Protocol(at) => (
                        "ProtocolChip",
                        protocol_name(flea::ui_logic_network_shares::form::PROTOCOLS[at])
                            .to_owned(),
                        String::new(),
                    ),
                    Focus::Cancel => ("DialogButton", card.words.cancel.clone(), String::new()),
                    Focus::Connect => ("DialogButton", Self::action(card).into(), String::new()),
                    Focus::Dropbox => ("DialogButton", card.words.install.clone(), String::new()),
                    Focus::Tls => ("CheckBox", "TLS".into(), String::new()),
                    Focus::Eye => return None,
                    field => (
                        "DialogField",
                        match field {
                            Focus::Label => "Label",
                            Focus::Host => "Host",
                            Focus::Port => "Port",
                            Focus::Path => path_label(card.model.form.protocol),
                            Focus::Domain => "Domain",
                            Focus::User => "Username",
                            Focus::Password => "Password",
                            _ => return None,
                        }
                        .into(),
                        String::new(),
                    ),
                };
                let enabled =
                    !card.model.saving && (*focus == Focus::Cancel || !card.model.connecting);
                let source = crate::visual::object([
                    ("name", Json::Str(name)),
                    ("value", Json::Str(value)),
                    ("member", Json::Bool(matches!(focus, Focus::Protocol(_)))),
                    ("primary", Json::Bool(*focus == Focus::Connect)),
                    ("rect", Json::Str(crate::ipc::rows::rect_of(x, y, w, h))),
                    ("centre", Json::Str(crate::ipc::rows::centre_of(x, y, w, h))),
                    ("visible", Json::Bool(true)),
                    ("enabled", Json::Bool(enabled)),
                    (
                        "focused",
                        Json::Bool(self.inside && card.model.focus == *focus),
                    ),
                    ("hovered", Json::Bool(card.hovered == Some(*focus))),
                    ("pressed", Json::Bool(card.pressed == Some(*focus))),
                    ("pressLive", Json::Bool(kind != "DialogField")),
                ]);
                super::super::cohesion_controls::control(&source, "NetworkDialog", kind)
            })
            .collect();
        Some(
            flea::jsondoc::render(&super::super::cohesion_controls::state(entries))
                .trim_end()
                .into(),
        )
    }
}
