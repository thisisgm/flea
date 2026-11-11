//! Tab choices use the shared menu and retain the tab's identity until activation.
use crate::{
    g2::context_menus::menu::{Face, Outcome},
    state::Shell,
};
use flea::ui_logic_context_menus::{Entry, Text};
use gpui::{Context, Pixels, Point, Window};

pub(crate) struct Target {
    pub side: usize,
    pub identity: u64,
}

impl Shell {
    pub(crate) fn tab_menu_entries(&self) -> Option<Vec<Entry>> {
        let Target { side, identity } = *self.strip.menu.as_ref()?;
        let tabs = self.strip.tabs[side].tabs.as_ref()?;
        if !tabs.items.iter().any(|tab| tab.identity == identity) {
            return Some(Vec::new());
        }
        let mut entries = [
            ("tabNew", "tabs.new", "plus"),
            ("tabDuplicate", "tabs.duplicate", "copy"),
            ("tabMoveWindow", "tabs.move_to_window", "app-window"),
            ("tabClose", "tabs.close", "x"),
            ("tabCloseOthers", "tabs.close_others", "copy-x"),
        ]
        .into_iter()
        .map(|(id, label, glyph)| Entry::row(id, Text::Key(label), glyph))
        .collect::<Vec<_>>();
        entries[0].disabled = tabs.items.len() >= super::MAX;
        entries[1].disabled = entries[0].disabled;
        entries[2].disabled = tabs.items.len() <= 1;
        entries[3].disabled = tabs.items.len() <= 1;
        entries[4].disabled = tabs.items.len() <= 1;
        entries.insert(3, Entry::separator());
        Some(entries)
    }

    pub(crate) fn open_tab_menu(
        &mut self,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.input_blocked(cx) {
            return;
        }
        let index = match self.tab_hit(at) {
            super::bar::Hit::Tab(index) | super::bar::Hit::Close(index) => index,
            _ => return,
        };
        let side = self.focus_side;
        let Some(identity) = self.strip.tabs[side]
            .tabs
            .as_ref()
            .and_then(|tabs| tabs.items.get(index))
            .map(|tab| tab.identity)
        else {
            return;
        };
        let Some(bounds) = self.strip.bounds.get() else {
            return;
        };
        let at = (
            f32::from(at.x),
            f32::from(bounds.bottom()) + super::bar::MENU_GAP,
        );
        self.retire_menu(window, cx);
        self.observe_menu(window, cx);
        self.strip.menu = Some(Target { side, identity });
        let rows = self.tab_menu_entries().unwrap_or_default();
        let typeface = self.active_typeface();
        let env = self.menu_env(&typeface);
        let size = window.viewport_size();
        self.menu.menu.borrow_mut().open(
            Face::Tab,
            rows,
            at,
            flea::ui_logic_context_menus::fit::Rect {
                x: 0.,
                y: 0.,
                w: size.width.into(),
                h: size.height.into(),
            },
            "",
            &env,
            window,
        );
        if let Some(host) = self.panes[side].as_ref() {
            self.menu.captured = Some(crate::g2::context_menus::join::Captured {
                side,
                listing: host.pane.held.listing,
                rows: Vec::new(),
                cursor: host.pane.cursor,
                path: host.path.clone(),
                identity: String::new(),
                id: 0,
                ready: true,
                kinds: Vec::new(),
                background: true,
                target: None,
            });
        }
        self.menu.pointer = Some(at);
        self.menu.resting = Some(at);
        self.menu.pointer_armed = false;
        self.keyboard.context = flea::keys::Context::Menu;
        self.keyboard.disarm();
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(crate) fn tab_menu_outcome(
        &mut self,
        outcome: &Outcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(&Target { side, identity }) = self.strip.menu.as_ref() else {
            return false;
        };
        let Outcome::Fire { action, .. } = outcome else {
            return false;
        };
        let index = self.strip.tabs[side]
            .tabs
            .as_ref()
            .and_then(|tabs| tabs.items.iter().position(|tab| tab.identity == identity));
        self.retire_menu(window, cx);
        let Some(index) = index else {
            return true;
        };
        if action == "tabMoveWindow" {
            self.detach_tab(index, window, cx);
        } else {
            self.with_tabs(|_, panes| {
                panes.visit(side, &mut |tabs, pane| match action.as_str() {
                    "tabNew" => tabs.open_new(pane, None),
                    "tabDuplicate" => tabs.duplicate(pane, index),
                    "tabClose" => tabs.close_at(pane, index),
                    "tabCloseOthers" => tabs.close_others(pane, index),
                    _ => {}
                });
            });
            self.tabs_changed(window, cx);
        }
        true
    }
}
