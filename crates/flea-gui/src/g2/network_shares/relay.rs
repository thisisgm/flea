//! A submitted password opts into authentication; an empty draft retains anonymous or key-first mounting.
use super::{dialog, mount::Request, service::Service};
use gpui::Context;

pub fn submitted(effect: dialog::Effect) -> Result<Request, dialog::Effect> {
    match effect {
        dialog::Effect::Mount {
            id,
            uri,
            label,
            origin,
            mut password,
        } => {
            if flea::ui_logic_network_shares::protocols::scheme_of(&uri) == "nfs" {
                password.clear();
            }
            Ok(Request {
                id,
                uri,
                label,
                origin,
                mounted: false,
                authenticated: !password.is_empty(),
                password,
            })
        }
        other => Err(other),
    }
}
impl Service {
    pub fn dialog_effect(
        &mut self,
        effect: dialog::Effect,
        cx: &mut Context<Self>,
    ) -> Option<dialog::Effect> {
        let effect = match submitted(effect) {
            Ok(request) => {
                let now = self.now(cx);
                self.open_share(request, now, cx);
                return None;
            }
            Err(effect) => effect,
        };
        match effect {
            dialog::Effect::CheckDropbox(id) => {
                self.check_provider(id, cx);
                None
            }
            dialog::Effect::CancelDropbox(id) => {
                if self.provider_id == Some(id) {
                    self.stop_provider();
                }
                None
            }
            dialog::Effect::Cancel(id) => {
                let now = self.now(cx);
                self.cancel(id, now, cx);
                None
            }
            other => Some(other),
        }
    }
}
