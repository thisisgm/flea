use crate::{
    g2::{
        plugin_settings::{
            seam::{Effect, PluginsWire, Tokens},
            view::PluginsPane,
        },
        settings_pages::Request,
    },
    state::Shell,
};
use gpui::{AppContext, Context, Window};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
pub(super) struct Wire(pub Rc<RefCell<Vec<String>>>);
impl PluginsWire for Wire {
    fn send(&self, line: String) -> Result<(), String> {
        self.0.borrow_mut().push(line);
        Ok(())
    }
}
impl Shell {
    pub(super) fn sync_settings_plugins(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let showing = self.settings.opened
            && self
                .settings
                .panel
                .as_ref()
                .is_some_and(|view| view.read(cx).panel.section == "plugins");
        if showing {
            self.follow_plugins(window, cx);
        }
        // Plugins coming into view, which a kept view does again after every close.
        let shown = showing && !self.settings.plugins.shown;
        self.settings.plugins.shown = showing;
        if showing && self.settings.plugins.pane.is_none() {
            let theme = cx
                .try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default();
            let palette = crate::list::Palette::inherited(&window.text_style(), theme);
            let wire = Rc::new(Wire(self.settings.plugins.queue.clone()));
            let view = cx.new(|cx| {
                PluginsPane::new(
                    self.locale.catalog.clone(),
                    wire,
                    Tokens::on_surface(&palette, theme),
                    self.corner,
                    cx,
                )
            });
            self.settings
                .subscriptions
                .push(cx.observe_in(&view, window, |shell, _, _, cx| {
                    shell.flush_settings_plugins(cx)
                }));
            self.settings.subscriptions.push(cx.subscribe_in(
                &view,
                window,
                |shell, _, effect: &Effect, window, cx| match effect {
                    Effect::Close => shell.settings_request(Request::Close, window, cx),
                    Effect::Open(path) => {
                        shell.settings.opened = false;
                        window.focus(&shell.focus, cx);
                        let _ = shell.step(
                            shell.focus_side,
                            crate::window::navigate::Step::OpenPlace(path.clone()),
                            window,
                            cx,
                        );
                    }
                    Effect::PinFolder => shell.pin_settings_folder(window, cx),
                    Effect::AirdropReceiving(on, folder) => {
                        shell.airdrop_receiving(*on, folder, window, cx)
                    }
                    Effect::AirdropFolder(start) => shell.airdrop_choose_folder(start, window, cx),
                    Effect::None => {}
                },
            ));
            if let Some(panel) = &self.settings.panel {
                panel.update(cx, |panel, cx| {
                    cx.observe(&view, |_, _, cx| cx.notify()).detach();
                    panel.plugins = Some(view.clone());
                    cx.notify();
                });
            }
            self.settings.plugins.pane = Some(view);
        }
        if shown {
            self.airdrop_settings_shown(window, cx);
        }
        if self.settings.opened {
            if let (Some(panel), Some(plugin)) = (&self.settings.panel, &self.settings.plugins.pane)
            {
                let panel = panel.read(cx);
                let owns = showing && panel.panel.side == crate::g2::settings_pages::Side::Pane;
                let parent = panel.focus.clone();
                let focus = plugin.read(cx).focus_handle().clone();
                if owns {
                    window.focus(&focus, cx);
                } else if focus.is_focused(window) {
                    window.focus(&parent, cx);
                }
            }
        }
        self.flush_settings_plugins(cx);
    }
    fn flush_settings_plugins(&mut self, cx: &mut Context<Self>) {
        let lines = std::mem::take(&mut *self.settings.plugins.queue.borrow_mut());
        for line in lines {
            let sent = self
                .plugins
                .host
                .as_ref()
                .is_some_and(|host| host.read(cx).line(line));
            if !sent {
                if let Some(view) = &self.settings.plugins.pane {
                    view.update(cx, |view, cx| view.backend_failed(cx));
                }
            }
        }
    }
    pub(crate) fn settings_plugins_reply(&mut self, line: &str, cx: &mut Context<Self>) {
        if let Some(view) = &self.settings.plugins.pane {
            view.update(cx, |view, cx| view.receive(line, cx));
        }
    }

    pub(crate) fn settings_plugins_failed(&mut self, side: usize, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.settings.plugins.backends[side]) {
            if let Some(view) = &self.settings.plugins.pane {
                view.update(cx, |view, cx| view.backend_failed(cx));
            }
        }
    }

    fn pin_settings_folder(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(path) = self.panes[self.focus_side]
            .as_ref()
            .map(|pane| pane.path.clone())
        else {
            return;
        };
        let binary = crate::window::startup::binary();
        cx.spawn_in(window, async move |this, cx| {
            let pinned = cx
                .background_executor()
                .spawn(async move {
                    std::process::Command::new(binary)
                        .args(["shelf", "pin", &path])
                        .status()
                        .is_ok_and(|status| status.success())
                })
                .await;
            let _ = this.update(cx, |shell, cx| {
                if pinned {
                    if let Some(plugin) = &shell.settings.plugins.pane {
                        plugin.update(cx, |plugin, cx| plugin.refresh(cx));
                    }
                } else {
                    shell.settings_error("places.shelf.pin_failed", cx);
                }
            });
        })
        .detach();
    }
}
