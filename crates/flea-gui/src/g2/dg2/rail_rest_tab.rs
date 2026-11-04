//! A held drag resting on another tab selects it after the switch delay (TabBar.qml hoverSwitch).
use super::*;

impl Shell {
    // TabBar.qml hoverSwitch: a drag resting on another tab selects it once the switch delay has passed.
    pub(super) fn rest_on_tab(&mut self, spot: Spot, window: &mut Window, cx: &mut Context<Self>) {
        let Spot::Tab(index) = spot else {
            self.strip.bar.hover_exited();
            return;
        };
        let now = self.tab_now();
        self.strip.bar.hover_entered(index, true, now);
        let wait = std::time::Duration::from_millis(crate::tabs::bar::HOVER_SWITCH_MS);
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let now = shell.tab_now();
                if shell.with_tabs(|bar, panes| bar.hover_tick(panes, now)) {
                    shell.tabs_changed(window, cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
