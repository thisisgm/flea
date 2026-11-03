//! The source window's compositor rectangle, asked once per lift off the UI thread.
use super::transfer::Rect;
use crate::state::Shell;
use gpui::Context;
use std::sync::Arc;

/// Answers the compositor's client list as JSON, or an empty string when it cannot.
pub(crate) type Clients = Arc<dyn Fn() -> String + Send + Sync>;

#[cfg(not(test))]
pub(crate) fn hyprland() -> Clients {
    Arc::new(|| {
        std::process::Command::new("hyprctl")
            .args(["clients", "-j"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .unwrap_or_default()
    })
}

#[cfg(test)]
pub(crate) fn hyprland() -> Clients {
    Arc::new(String::new)
}

impl Shell {
    /// A lift records its strip band and asks for the window rectangle; a late answer for an older lift is dropped.
    pub(crate) fn begin_tab_geometry(&mut self, cx: &mut Context<Self>) {
        let Some(bounds) = self.strip.bounds.get() else {
            return;
        };
        let strip = Rect {
            x: f64::from(bounds.origin.x) + f64::from(super::bar::PAD_X),
            y: f64::from(bounds.origin.y),
            width: f64::from(bounds.size.width) - f64::from(super::bar::PAD_X),
            height: f64::from(bounds.size.height),
        };
        let token = self.strip.bar.out_token.clone();
        if self.strip.geometry.begin(&token, strip) {
            self.strip.lifts += 1;
            self.ask_tab_geometry(cx);
        }
    }

    fn ask_tab_geometry(&mut self, cx: &mut Context<Self>) {
        self.strip.client_queries += 1;
        let clients = self.strip.clients.clone();
        cx.spawn(async move |this, cx| {
            let text = cx
                .background_executor()
                .spawn(async move { clients() })
                .await;
            let _ = this.update(cx, |shell, cx| {
                if shell.strip.geometry.finished(&text, std::process::id()) {
                    shell.ask_tab_geometry(cx);
                }
            });
        })
        .detach();
    }
}
