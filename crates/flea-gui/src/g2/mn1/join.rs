//! The window's menu mount and the selection captured before asynchronous producer replies.
use super::{
    lines::Env,
    menu::{Menu, Outcome},
};
use crate::{
    state::Shell,
    window::{input::Dispatch, pane::PaneHost},
};
use flea::{
    jsondoc::Json,
    ui_logic_mn1::{
        build::{self, Context as MenuContext},
        inventory, Entry,
    },
};
use gpui::{Context, KeyDownEvent, Window};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, Debug)]
pub struct Captured {
    pub side: usize,
    pub listing: usize,
    pub rows: Vec<usize>,
    pub cursor: usize,
    pub path: String,
    pub identity: String,
    pub id: u64,
    pub ready: bool,
    pub kinds: Vec<u32>,
    pub background: bool,
}

/// A checked choice handed to the producer that owns the action; every target remains captured.
#[derive(Clone, Debug)]
pub struct Chosen {
    pub captured: Captured,
    pub action: String,
    pub leaf: String,
    pub paths: Vec<String>,
    pub(crate) claimed: Rc<Cell<bool>>,
}

impl Chosen {
    /// A mounted producer claims the snapshot before routing it through its own effect lane.
    pub fn claim(&self) -> bool {
        !self.claimed.replace(true)
    }
}

impl gpui::EventEmitter<Chosen> for Shell {}

/// An unconsumed producer reply handed to the owner that claimed its menu snapshot.
#[derive(Clone, Debug)]
pub struct ProducerReply {
    pub side: usize,
    pub line: String,
}

impl gpui::EventEmitter<ProducerReply> for Shell {}

#[derive(Default)]
pub struct Joined {
    pub menu: Rc<RefCell<Menu>>,
    pub captured: Option<Captured>,
    pub context: MenuContext,
    pub pending: Option<(String, String)>,
    pub activating: bool,
    pub reading: bool,
    pub sequence: u64,
    pub pointer: Option<(f32, f32)>,
    pub resting: Option<(f32, f32)>,
    pub pointer_armed: bool,
    pub presentation: super::presentation::Presentation,
    pub shebang: super::shebang::Shebang,
}

fn identity(host: &PaneHost, side: usize, background: bool) -> String {
    format!(
        "{side}:{}:{}:{:?}:{}:{background}",
        host.path, host.pane.held.listing, host.selected, host.pane.cursor
    )
}

fn flag(data: &Json, key: &str) -> bool {
    data.get(key).and_then(Json::as_bool) == Some(true)
}

impl Shell {
    pub(crate) fn menu_opened(&self) -> bool {
        self.menu.menu.borrow().opened()
    }

    pub(crate) fn menu_identity(&self) -> String {
        self.menu
            .captured
            .as_ref()
            .and_then(|capture| {
                self.panes[capture.side]
                    .as_ref()
                    .map(|host| identity(host, capture.side, capture.background))
            })
            .unwrap_or_default()
    }

    pub(crate) fn menu_entries(&self) -> Vec<Entry> {
        let mut context = self.menu.context.clone();
        let Some(capture) = &self.menu.captured else {
            return Vec::new();
        };
        let Some(host) = &self.panes[capture.side] else {
            return Vec::new();
        };
        let stored = self
            .launch
            .settings
            .data
            .get("menu")
            .and_then(|m| m.get("hidden"))
            .and_then(Json::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(Json::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            });
        context.hidden = inventory::effective_hidden(
            stored.as_deref(),
            self.launch
                .settings
                .data
                .get("shelf")
                .is_some_and(|s| flag(s, "enabled")),
        );
        context.show_hidden = host.nav.folders.borrow().hidden;
        context.sort_by = host.nav.order.key.into();
        context.has_folder_sort =
            !host.pane.swap.session.open && host.nav.folders.borrow().has(&host.path);
        build::listing_entries(&context)
    }

    pub(crate) fn open_menu(
        &mut self,
        side: usize,
        row: Option<usize>,
        at: (f32, f32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.retire_menu(window, cx);
        self.observe_menu(window, cx);
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        if host.stopped || host.pane.busy() {
            return;
        }
        if let Some(row) = row {
            if host.pane.held.row(row).is_none() {
                return;
            }
            host.pane.cursor = row;
        }
        let background = row.is_none();
        self.focus_side = side;
        let rows = if background {
            Vec::new()
        } else if host.selected.is_empty() {
            vec![host.pane.cursor]
        } else {
            host.selected.clone()
        };
        self.menu.sequence += 1;
        let capture = Captured {
            side,
            listing: host.pane.held.listing,
            rows,
            cursor: host.pane.cursor,
            path: host.path.clone(),
            identity: identity(host, side, background),
            id: self.menu.sequence,
            ready: false,
            kinds: Vec::new(),
            background,
        };
        let facts = row.and_then(|row| host.pane.held.row(row));
        // Print, Convert and Combine Images read the marked rows' output kinds.
        let kinds = capture
            .rows
            .iter()
            .filter_map(|&row| host.pane.held.row(row).map(crate::window::output::kind))
            .collect::<Vec<_>>();
        let image = flea::ui_logic_op4::output::Kind::Image;
        self.menu.context = MenuContext {
            has_row: !background,
            selection_count: capture.rows.len(),
            has_shebang: false,
            cursor_is_target: capture.rows.as_slice() == [capture.cursor],
            row_mode: facts.map_or(0, |r| r.mode),
            row_is_file: facts.is_some_and(|r| !r.dir),
            row_is_symlink: facts.is_some_and(|r| r.link.is_some()),
            row_name: facts.map_or_else(String::new, |r| r.name.clone()),
            row_is_image: facts.is_some_and(|r| crate::window::output::kind(r) == image),
            selection_images: kinds.iter().filter(|&&kind| kind == image).count(),
            printable: kinds.len() == capture.rows.len()
                && flea::ui_logic_op4::output::can_print(&kinds),
            can_convert: self.output.convert[side] != Some(false),
            selection_modes: capture
                .rows
                .iter()
                .filter_map(|&row| host.pane.held.row(row).map(|r| r.mode))
                .collect(),
            lang_marks: self
                .launch
                .settings
                .data
                .get("langMarks")
                .and_then(Json::as_bool)
                != Some(false),
            open_with_loaded: background,
            ..MenuContext::default()
        };
        self.menu.captured = Some(capture.clone());
        self.check_menu_shebang(window, cx);
        self.menu.presentation.at = Some(at);
        self.menu.pointer = Some((
            window.mouse_position().x.into(),
            window.mouse_position().y.into(),
        ));
        self.menu.resting = self.menu.pointer;
        self.menu.pointer_armed = false;
        self.keyboard.context = flea::keys::Context::Menu;
        self.keyboard.disarm();
        window.focus(&self.focus, cx);
        if let Some(host) = &mut self.panes[side] {
            host.nav.gate.menu_open = true;
        }
        if !background {
            let rows = capture
                .rows
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",");
            self.menu_send(side, format!("{{\"c\":\"menuaction\",\"op\":\"snapshot\",\"id\":{},\"listing\":{},\"rows\":[{rows}],\"cursor\":{}}}", capture.id, capture.listing, capture.cursor), cx);
            if self.menu.captured.is_some() {
                self.arm_menu_loading(window, cx);
            }
        } else {
            self.refresh_menu(window, cx);
        }
        cx.notify();
    }

    pub(crate) fn menu_env<'a>(&'a self, typeface: &'a crate::list::Typeface) -> Env<'a> {
        Env {
            catalog: &self.locale.catalog,
            preset: self
                .launch
                .settings
                .data
                .get("keys")
                .and_then(Json::as_str)
                .unwrap_or("default"),
            key_hints: flag(&self.launch.settings.data, "keyHints"),
            typeface,
            inset: (4. * f32::from(typeface.body) / 14.).round(),
        }
    }

    pub(crate) fn menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.menu.presentation.at.is_some() {
            if event.keystroke.key == "escape" {
                self.retire_menu(window, cx);
            }
            cx.stop_propagation();
            return true;
        }
        if self.menu_opened() {
            let preset = self
                .launch
                .settings
                .data
                .get("keys")
                .and_then(Json::as_str)
                .unwrap_or("default");
            let action =
                self.keyboard
                    .resolve(preset, event, window.modifiers(), Default::default());
            if let Dispatch::Action(action) = action {
                let side = self
                    .menu
                    .captured
                    .as_ref()
                    .map_or(self.focus_side, |c| c.side);
                let list = self.panes[side]
                    .as_ref()
                    .expect("the menu's pane")
                    .list
                    .borrow();
                let env = self.menu_env(&list.typeface);
                let live = self.menu_entries();
                let outcome = if action == "copyAs" || action == "pasteAs" {
                    self.menu.menu.borrow_mut().open_submenu_for(
                        action,
                        self.menu.context.clipboard_available,
                        &env,
                        window,
                    )
                } else {
                    self.menu.menu.borrow_mut().act(
                        action,
                        Some(&live),
                        &self.menu_identity(),
                        &env,
                        window,
                    )
                };
                drop(list);
                self.menu_outcome(outcome, window, cx);
            }
            cx.stop_propagation();
            return true;
        }
        false
    }

    pub(crate) fn menu_outcome(
        &mut self,
        outcome: Outcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            Outcome::Closed => self.retire_menu(window, cx),
            Outcome::Refused(key) => {
                self.menu_refuse(key, cx);
                self.retire_menu(window, cx);
            }
            Outcome::Fire { action, leaf } => {
                self.keyboard.context = flea::keys::Context::Listing;
                self.menu.pending = Some((action, leaf));
                self.activate_menu(window, cx);
            }
            Outcome::Nothing | Outcome::Redraw => {}
        }
        cx.notify();
    }
}
