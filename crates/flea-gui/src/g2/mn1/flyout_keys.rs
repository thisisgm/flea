//! c and P with the menu closed: the cursor row's menu opens and its Copy As or Paste As flyout follows.
use crate::state::Shell;
use gpui::{Context, Window};

impl Shell {
    /// c or P with the menu closed: the cursor row's menu opens and its flyout follows, as v0.3.12 openCopyAs and openPasteAs do.
    pub(crate) fn menu_submenu_key(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu_opened() {
            self.menu_submenu_open(action, window, cx);
            return;
        }
        if action == "copyAs"
            && self
                .panes
                .get(self.focus_side)
                .and_then(Option::as_ref)
                .is_none_or(|host| host.pane.held.total == 0)
        {
            let side = self.focus_side;
            self.menu_notice(side, self.locale.catalog.tr("menu.refusal.nocursor"), cx);
            return;
        }
        self.menu_keyboard_open(window, cx);
        if self.menu_opened() {
            self.menu_submenu_open(action, window, cx);
        } else if self.menu.captured.is_some() {
            self.menu.submenu = Some(action.to_owned());
        }
    }

    /// The open menu's Copy As or Paste As flyout, for the key now or for the card once it stands.
    pub(super) fn menu_submenu_open(&mut self, action: &str, window: &mut Window, cx: &mut Context<Self>) {
        let side = self
            .menu
            .captured
            .as_ref()
            .map_or(self.focus_side, |capture| capture.side);
        let list = self.panes[side]
            .as_ref()
            .expect("the menu's pane")
            .list
            .borrow();
        let env = self.menu_env(&list.typeface);
        let outcome = self.menu.menu.borrow_mut().open_submenu_for(
            action,
            self.menu.context.clipboard_available,
            &env,
            window,
        );
        drop(list);
        self.menu_outcome(outcome, window, cx);
    }
}
