//! What the rail does with a key: its cursor, the mount-first focus claim and the release mark.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    List,
    Rail,
    // Any other surface, which a late landing never moves.
    Elsewhere,
}

// The key itself, read raw for the mark because the table resolves Right to opening the row in every preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawKey {
    Right,
    Left,
    Escape,
    Enter,
    Space,
    Other,
}

// What an activation did before it returned: a mount or a gio info can still be out, or it answered at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activation {
    Waiting,
    Opened,
    Failed,
}

// The rail's side of each key: the host owns the rows, the release service and the menus.
pub trait RailHost {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    // Whether the row's mark releases (Eject.releasable, the device unit's verdict).
    fn releasable(&self, index: usize) -> bool;
    fn activate(&mut self, index: usize) -> Activation;
    // The status bar's own Escape, true when it took the key.
    fn status_escape(&mut self) -> bool;
    fn focus(&self) -> Focus;
    fn set_focus(&mut self, focus: Focus);
    fn add_network(&mut self);
    fn start_rename(&mut self, index: usize);
    fn is_trash(&self, index: usize) -> bool;
    fn open_cursor_menu(&mut self);
    // Mounts.raiseMenu: the row's menu, or the sentence saying it has nothing to release.
    fn raise_menu(&mut self, index: usize);
    // Eject.release from the rail's cursor row, picked by name and sent by the row's key.
    fn release(&mut self, index: usize);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RailCursor {
    pub index: usize,
    // An open from the rail claims focus for the folder it lands in, spent when it lands, fails or another key comes.
    pub focus_on_open: bool,
    // A pointer press hid the keyboard highlight until the next rail key.
    pub pointer_focus: bool,
    pub mark_focused: bool,
}

// The keyboard cursor takes the rail's highlight only while the keyboard holds it, never under an open rail menu or after a pointer press.
pub fn cursor_drawn(focused: bool, menu_open: bool, pointer_focus: bool) -> bool {
    focused && !menu_open && !pointer_focus
}

impl RailCursor {
    // A mount-first open landed: focus follows it into the folder unless the user already left the rail.
    pub fn landed(&mut self, focus: &mut Focus) {
        if !self.focus_on_open {
            return;
        }
        self.focus_on_open = false;
        if *focus == Focus::Rail {
            *focus = Focus::List;
        }
    }

    // A mount that failed has no open to land, so its claim goes with the error.
    pub fn messaged(&mut self, is_error: bool) {
        if is_error {
            self.focus_on_open = false;
        }
    }

    // Right from a releasable row moves the keyboard onto its mark; there Left or Escape leave and Enter or Space release.
    fn mark_key(&mut self, key: RawKey, modified: bool, host: &mut impl RailHost) -> bool {
        if modified {
            return false;
        }
        if !self.mark_focused {
            if key != RawKey::Right || self.index >= host.len() || !host.releasable(self.index) {
                return false;
            }
            self.mark_focused = true;
            return true;
        }
        match key {
            RawKey::Left | RawKey::Escape => self.mark_focused = false,
            RawKey::Enter | RawKey::Space => {
                self.mark_focused = false;
                host.release(self.index);
            }
            RawKey::Right => {}
            RawKey::Other => return false,
        }
        true
    }

    // The rail answers ten of the key table's action names; key is the raw key for the ones the table does not name.
    pub fn act(&mut self, action: &str, key: RawKey, modified: bool, host: &mut impl RailHost) {
        self.focus_on_open = false;
        self.pointer_focus = false;
        if self.mark_key(key, modified, host) {
            return;
        }
        if action.starts_with("cursor") {
            self.mark_focused = false;
        }
        let len = host.len();
        match action {
            "cursorDown" if len > 0 => self.index = (self.index + 1).min(len - 1),
            "cursorUp" => self.index = self.index.saturating_sub(1),
            "cursorFirst" => self.index = 0,
            "cursorLast" => self.index = len.saturating_sub(1),
            "open" if len > 0 => {
                self.focus_on_open = true;
                match host.activate(self.index) {
                    Activation::Opened => {
                        let mut focus = host.focus();
                        self.landed(&mut focus);
                        host.set_focus(focus);
                    }
                    Activation::Failed => self.messaged(true),
                    Activation::Waiting => {}
                }
            }
            "escape" if !host.status_escape() => host.set_focus(Focus::List),
            "addNetwork" => host.add_network(),
            "rename" => host.start_rename(self.index),
            "menu" if self.index < len => {
                if host.is_trash(self.index) {
                    host.open_cursor_menu();
                } else {
                    host.raise_menu(self.index);
                }
            }
            "eject" => host.release(self.index),
            _ => {}
        }
    }
}

// A rail open lands in the pane that asked; no pane opens nothing and spends the claim, no rail opens all the same.
pub fn open_from(rail: Option<&mut RailCursor>, pane: Option<&mut Focus>) -> bool {
    match (pane, rail) {
        (None, Some(rail)) => {
            rail.focus_on_open = false;
            false
        }
        (None, None) => false,
        (Some(focus), Some(rail)) => {
            rail.landed(focus);
            true
        }
        (Some(_), None) => true,
    }
}
