use super::{service, Host};
use crate::state::Shell;
use flea::ui_logic_plugin_settings::contributions;
use gpui::{Context, Entity, Subscription, Window};

#[derive(Default)]
pub(crate) struct Join {
    pub host: Option<Entity<Host>>,
    observers: Vec<Subscription>,
    pub(super) rail: super::rail::Rail,
    pub(super) routes: [Option<gpui::Task<()>>; 2],
    /// A plugin action's open input card.
    pub(crate) input: Option<super::input::Open>,
}

/// How long a new window waits before it starts the plugin owner, so the owner's start never runs inside the window's first build.
const START_DELAY: std::time::Duration = std::time::Duration::from_millis(1);

impl Shell {
    /// A new window follows the app-wide owner once its own build has returned.
    pub(crate) fn start_plugins(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(START_DELAY).await;
            let _ = this.update_in(cx, |shell, window, cx| shell.follow_plugins(window, cx));
        })
        .detach();
    }

    pub(crate) fn follow_plugins(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.plugins.host.is_some() {
            return;
        }
        let host = service(cx);
        self.plugins.observers.push(cx.subscribe_in(
            &host,
            window,
            |shell, _, _: &super::Closed, _, cx| {
                if let Some(view) = &shell.settings.plugins.pane {
                    view.update(cx, |view, cx| view.backend_failed(cx));
                }
            },
        ));
        self.plugins.observers.push(cx.subscribe_in(
            &host,
            window,
            |shell, _, line: &super::WireLine, _, cx| {
                shell.user_keys_listed(&line.0);
                shell.settings_plugins_reply(&line.0, cx);
            },
        ));
        self.plugins
            .observers
            .push(cx.observe_in(&host, window, |shell, host, window, cx| {
                let host = host.read(cx);
                let contributions = host
                    .plugins
                    .iter()
                    .filter(|p| host.healthy(p))
                    .flat_map(|p| contributions::menu_actions(&p.manifest).unwrap_or_default())
                    .collect();
                shell.menu.actions.set_contributions(contributions);
                shell.user_keys_changed(window, cx);
                shell.rail_tail(cx);
                shell.plugin_rail_poll(cx);
                shell.plugin_places_changed(cx);
                shell.refresh_menu(window, cx);
                cx.notify();
            }));
        self.plugins.host = Some(host);
        self.user_keys_changed(window, cx);
    }

    /// Saved plugin settings changed: the owner rescans and an open Plugins card relists.
    pub(crate) fn plugins_settings_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(host) = &self.plugins.host {
            host.read(cx).refresh();
        }
        if let Some(view) = &self.settings.plugins.pane {
            view.update(cx, |view, cx| view.refresh(cx));
        }
    }
}
