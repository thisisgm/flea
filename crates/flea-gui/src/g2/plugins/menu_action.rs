//! Validated action requests and native input continuations enter the same supervised host lane.
use super::{contributions, prompt};
use crate::{g2::mn1::join::Captured, state::Shell};
use flea::{jsondoc::Json, ui_logic_mn2::plugins};
use gpui::{Context, Window};

impl Shell {
    pub(crate) fn deliver_plugin_action(
        &mut self,
        capture: &Captured,
        action: &str,
        paths: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let contribution = self.declared_menu_plugins("menu-action").find_map(|p| {
            contributions::menu_actions(&p.manifest)
                .ok()?
                .into_iter()
                .find(|c| plugins::leaf(c) == action)
                .map(|c| (p.clone(), c))
        });
        let Some((plugin, contribution)) = contribution else {
            return false;
        };
        let Ok(descriptor) = contributions::action_input(&plugin.manifest, &contribution.action)
        else {
            self.retire_menu(window, cx);
            return true;
        };
        let input = match self.action_prompt(capture, action, descriptor, paths, window, cx) {
            prompt::Input::Ready(input) => input,
            prompt::Input::Waiting => return true,
            prompt::Input::Refused => {
                self.retire_menu(window, cx);
                return true;
            }
        };
        let mut request = Json::Obj(vec![
            ("op".into(), Json::Str("run".into())),
            ("action".into(), Json::Str(contribution.action)),
            (
                "paths".into(),
                Json::Arr(paths.iter().cloned().map(Json::Str).collect()),
            ),
            (
                "settings".into(),
                Json::Obj(
                    plugin
                        .settings
                        .iter()
                        .map(|(k, v)| (k.clone(), v.to_json()))
                        .collect(),
                ),
            ),
        ]);
        if plugin
            .manifest
            .access
            .iter()
            .any(|access| access == "folder")
        {
            let folder = self.panes[capture.side]
                .as_ref()
                .map_or(capture.path.as_str(), |host| {
                    host.search.owner.session.base_of(&capture.path)
                })
                .to_owned();
            if let Json::Obj(fields) = &mut request {
                fields.push(("folder".into(), Json::Str(folder)));
            }
        }
        if let (Some(input), Json::Obj(fields)) = (input, &mut request) {
            fields.push(("input".into(), input));
        }
        self.plugin_selection_request(capture.side, &plugin.manifest.id, request, cx);
        self.retire_menu(window, cx);
        true
    }

    pub(super) fn plugin_selection_request(
        &mut self,
        side: usize,
        plugin: &str,
        request: Json,
        cx: &mut Context<Self>,
    ) {
        let error_owner = super::verdict::owner(side, plugin, &request);
        let envelope = flea::ui_logic_st2::wire::request_line(
            u64::MAX,
            "runtime",
            plugin,
            vec![("request", request.clone())],
        );
        if envelope.len() > flea::plugin_runtime::protocol::MAX_MESSAGE {
            self.menu_notice(side, self.locale.catalog.tr("menu.refusal.unavailable"), cx);
            return;
        }
        if self.menu.plugin_runtime.pending.len() >= crate::g2::plugins::limits::PENDING {
            self.menu_notice(side, self.locale.catalog.tr("menu.refusal.unavailable"), cx);
            return;
        }
        #[cfg(test)]
        if let Some(dispatch) = &mut self.menu.plugin_runtime.dispatch {
            if let Err(error) = dispatch(plugin, request) {
                self.menu_notice(side, error, cx);
            }
            return;
        }
        let sent = self.plugins.host.clone().and_then(|host| {
            host.update(cx, |host, cx| {
                let available = host
                    .plugins
                    .iter()
                    .any(|p| p.manifest.id == plugin && host.healthy(p));
                available
                    .then(|| host.request(plugin, request, cx))
                    .flatten()
            })
        });
        if let Some(id) = sent {
            self.plugin_action_sent(error_owner, id, cx);
            self.menu.plugin_runtime.pending.insert(id, side);
        } else {
            self.menu_notice(side, self.locale.catalog.tr("menu.refusal.unavailable"), cx);
        }
    }
}
