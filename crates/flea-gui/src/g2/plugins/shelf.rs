use crate::state::Shell;
use flea::{jsondoc::Json, ui_logic_status_feedback::status::Hint};
use gpui::{Context, Window};

impl Shell {
    pub(crate) fn plugin_shelve(
        &mut self,
        side: usize,
        paths: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = paths.len();
        let receiver = self.plugins.host.clone().and_then(|host| {
            host.update(cx, |host, cx| {
                let plugin = host
                    .plugins
                    .iter()
                    .find(|p| host.healthy(p) && p.manifest.block("shelf").is_some())?
                    .manifest
                    .id
                    .clone();
                host.request_wait(
                    &plugin,
                    Json::Obj(vec![
                        ("op".into(), Json::Str("shelf".into())),
                        ("action".into(), Json::Str("add".into())),
                        (
                            "paths".into(),
                            Json::Arr(paths.into_iter().map(Json::Str).collect()),
                        ),
                    ]),
                    cx,
                )
            })
        });
        let Some(receiver) = receiver else {
            let sentence = self.locale.catalog.tr("menu.shelf.refused");
            self.owner_notice(side, &sentence, true, Hint::None, cx);
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            let reply = receiver.recv().await;
            let _ = this.update(cx, |shell, cx| {
                let catalog = &shell.locale.catalog;
                let (sentence, error) = match reply {
                    Ok(reply) if reply.ok && count == 1 => {
                        (catalog.tr("menu.shelf.added_one"), false)
                    }
                    Ok(reply) if reply.ok => (
                        catalog.tr_args("menu.shelf.added_many", &[&count.to_string()]),
                        false,
                    ),
                    Ok(reply) if !reply.error.is_empty() => (reply.error, true),
                    _ => (catalog.tr("menu.shelf.refused"), true),
                };
                shell.owner_notice(side, &sentence, error, Hint::None, cx);
            });
        })
        .detach();
    }
}
