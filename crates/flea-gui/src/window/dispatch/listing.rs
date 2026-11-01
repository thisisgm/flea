//! The listing family: "/" and the filter's keys, the sort keys, reveal, the last of Escape and the side buttons on the focused pane.
use super::Order;
use crate::{
    state::Shell,
    window::{
        input::{self, ActionEvent},
        navigate::Step,
    },
};
use flea::keys::Context as Keys;
use gpui::{Context, MouseButton, MouseDownEvent, Window};

pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    let Order::Action(event) = order else {
        return false;
    };
    if shell.search_action(event.action, window, cx)
        || shell.filter_action(event.action, window, cx)
    {
        return true;
    }
    let side = shell.focus_side;
    if event.context == Keys::Listing && matches!(event.action, "openInPlace" | "closeInPlace") {
        shell.outline_action(
            side,
            event.action == "openInPlace",
            crate::window::outline::Input::Key,
            window,
            cx,
        );
        return true;
    }
    // WindowBody.qml: the side buttons belong to the window, decided by Nav.js mouseBack and MouseNav.js forward.
    let pointer = match event.action {
        "backOrParent" => Some(Step::MouseBack),
        "forward" => Some(Step::Forward),
        _ => None,
    };
    if let Some(step) = pointer {
        // WindowBody.qml: over Trash, Back closes it and Forward is refused, as Pane.qml goForward refuses the key.
        let trash = shell.trash.as_ref().is_some_and(|host| host.side == side);
        if trash && shell.trash_opened(cx) {
            if step == Step::MouseBack {
                shell.drop_trash(side, window, cx);
            }
            return true;
        }
        let _ = shell.step(side, step, window, cx);
        return true;
    }
    if event.context != Keys::Listing {
        return false;
    }
    let Some(host) = shell.panes[side].as_ref() else {
        return false;
    };
    let searching = host.search.owner.active();
    // Focus.js:188-190: a filter or a search still standing is the owner Escape and o belong to, so Recent answers them only with none up.
    let owned = searching
        || host.filter.owner.up()
        || host.pane.state.search
        || host.pane.swap.session.open
        || !host.pane.state.query.is_empty();
    let step = match event.action {
        // Focus.js:147-148: both sort keys go quiet while a search owns the header.
        "sortNext" | "sortReverse" if searching => return true,
        "sortNext" => Step::SortNext,
        "sortReverse" => Step::SortReverse,
        // Focus.js:143-144: reveal means something only on a search result or a Recent row; the search runs it there.
        "reveal" if host.recent.on && !owned => {
            shell.recent_reveal(side, window, cx);
            return true;
        }
        "reveal" => return true,
        "escape" if host.recent.on && !owned => {
            let _ = shell.step(side, Step::Back, window, cx);
            return true;
        }
        "escape" if shell.escape_climbs() => Step::Up,
        "escape" if owned => return false,
        // Pane.qml escapePressed: with no mark left, Escape clears the notice; StatusBar.qml took a standing error first.
        "escape" => {
            shell.status_messages(cx, |messages, now| {
                let serial = messages.notice_serial();
                messages.withdraw(serial, now)
            });
            cx.notify();
            return true;
        }
        _ => return false,
    };
    // Shell::step puts a refusal's own sentence in the pane status, so its result is spent here.
    let _ = shell.step(side, step, window, cx);
    true
}

impl Shell {
    /// A mouse side button over the window, refused behind a card, a menu, an overlay or a field holding the keyboard.
    pub(crate) fn side_button(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event.button, MouseButton::Navigate(_))
            || self.input_blocked(cx)
            || self.path_holds_keyboard(window, cx)
        {
            return;
        }
        let Some(binding) = input::pointer("window", "any", event) else {
            return;
        };
        let event = ActionEvent {
            action: binding.effect,
            context: Keys::Listing,
            seat: Default::default(),
            pair_armed: false,
        };
        self.dispatch(Order::Action(event), window, cx);
        // The press is the window's: a listing under it would take it for a row press refused while the folder loads.
        cx.stop_propagation();
    }

    /// The escapeUp setting: with it on, Escape cancels a live arm or pair first and then climbs.
    pub(crate) fn escape_up_on(&self) -> bool {
        flea::ui_logic_st1::pages::is_true(&self.launch.settings.data, &["escapeUp"])
    }

    /// Focus.js escapeUp: Escape climbs only with the setting on and nothing else owning the key; a landed row is no mark.
    pub(crate) fn escape_climbs(&self) -> bool {
        if !self.escape_up_on() {
            return false;
        }
        let Some(host) = self.panes[self.focus_side].as_ref() else {
            return false;
        };
        let landed = host.nav.landed.is_some_and(|(listing, row)| {
            listing == host.pane.held.listing && host.selected == [row]
        });
        !host.pane.busy()
            && !host.search.owner.active()
            && host.pane.state.query.is_empty()
            && !self.menu_opened()
            && (host.selected.is_empty() || landed)
    }
}
