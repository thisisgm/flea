//! The chooser's flat places rail: Recent, favourites, Home and its folders, network, drives and phones.
use super::Picker;
use crate::g2::pl1::source::{Activation, Event, Source};
use crate::g2::pl4::content::row_label;
use flea::i18n::I18n;
use flea::jsondoc::Json;
use flea::ui_logic_pk1 as logic;
use flea::ui_logic_pl4::{
    order::{recent_entry, RailSection},
    places::{home_entries, stored_entries},
    rail::{Group, Kind, RailEntry},
    recent,
};
use gpui::{prelude::*, Context, Entity, FocusHandle, SharedString, Subscription, Task, Window};

// picker-open: Theme.space(172) at body 14 beside its 1 px rule.
pub const WIDTH: f32 = 200.;

/// PickerPlaces.qml's entries, including the network and phone rows after the places.
pub fn entries(
    offer_recent: bool,
    favourites: &[Json],
    home: &str,
    dirs: &str,
    devices: &[RailSection],
) -> Vec<RailEntry> {
    offer_recent
        .then(recent_entry)
        .into_iter()
        .chain(stored_entries(favourites, home))
        .chain(home_entries(
            home,
            dirs,
            crate::g2::pl4::producer::home_glyph,
        ))
        .chain(
            devices
                .iter()
                .flat_map(|section| &section.rows)
                .filter(|row| matches!(row.group, Group::Network | Group::Device))
                .cloned(),
        )
        .collect()
}

/// What the rail draws and walks: its rows, the keyboard cursor and the sources that fill it.
pub struct Rail {
    pub entries: Vec<RailEntry>,
    pub labels: Vec<SharedString>,
    pub cursor: usize,
    pub focus: FocusHandle,
    favourites: Vec<Json>,
    dirs: String,
    // The history file Recent lists, read off the UI thread on each visit.
    pub history: String,
    pub(super) devices: Option<Entity<Source>>,
    pub(super) network: Option<super::network::Network>,
    // The drive a press is waiting on, so only its own mount opens the folder.
    pub(super) awaiting: String,
    pub(super) recent_read: Option<Task<()>>,
    // A place the rail opened hands the keyboard to the files on the next frame, as PickerWindow.qml onChosen does.
    pub(super) hand_off: bool,
    _devices: Option<Subscription>,
    _dirs: Option<Task<()>>,
}

impl Rail {
    pub fn new(home: &str, focus: FocusHandle) -> Self {
        let data_home = std::env::var("XDG_DATA_HOME").unwrap_or_default();
        Self {
            entries: Vec::new(),
            labels: Vec::new(),
            cursor: 0,
            focus,
            favourites: Vec::new(),
            dirs: String::new(),
            history: recent::history_path(&data_home, home),
            devices: None,
            network: None,
            awaiting: String::new(),
            recent_read: None,
            hand_off: false,
            _devices: None,
            _dirs: None,
        }
    }
}

impl Picker {
    pub(super) fn rebuild_rail(&mut self, cx: &mut Context<Self>) {
        let mut sections = self
            .rail
            .network
            .as_ref()
            .map(|n| n.rows.clone())
            .unwrap_or_default();
        sections.extend(
            self.rail
                .devices
                .as_ref()
                .map(|source| source.read(cx).picker_sections())
                .unwrap_or_default(),
        );
        let rail = &mut self.rail;
        rail.entries = entries(
            !logic::saving(&self.req),
            &rail.favourites,
            &self.browse.home,
            &rail.dirs,
            &sections,
        );
        rail.labels = labels(&self.locale.catalog, &rail.entries);
        rail.cursor = rail.cursor.min(rail.entries.len().saturating_sub(1));
        cx.notify();
    }

    // The history is read off the UI thread; a folder opened meanwhile leaves its answer unsent.
    pub(super) fn read_recent(&mut self, cx: &mut Context<Self>) {
        let generation = self.browse.listing.read(cx).generation();
        let history = self.rail.history.clone();
        let read = cx
            .background_executor()
            .spawn(async move { crate::g2::pl4::service::read_history(&history) });
        self.rail.recent_read = Some(cx.spawn(async move |this, cx| {
            let visits = read.await;
            let _ = this.update(cx, |picker, cx| {
                let view = picker.browse.listing.read(cx);
                if view.generation() != generation {
                    return;
                }
                let paths: Vec<String> = visits.into_iter().map(|visit| visit.path).collect();
                let line = view.listing().paths_line(&paths, view.window_size());
                if picker.send_listing(&line) {
                    picker.browse.in_flight += 1;
                }
            });
        }));
    }

    pub(super) fn rail_favourites(&mut self, state: &Json, cx: &mut Context<Self>) {
        self.rail.favourites = flea::ui_logic_pl4::order::saved_places(state).favourites;
        if let Some(source) = &self.rail.devices {
            source.update(cx, |source, cx| source.adopt_state(state, cx));
        }
        self.rebuild_rail(cx);
    }

    /// The live window's sources: the XDG folders read once off the UI thread, and the drives polled for as long as the dialog stands.
    pub fn start_places(&mut self, state: &Json, cx: &mut Context<Self>) {
        let home = self.browse.home.clone();
        let read = cx
            .background_executor()
            .spawn(async move { crate::g2::pl4::producer::dirs_text(&home) });
        self.rail._dirs = Some(cx.spawn(async move |this, cx| {
            let dirs = read.await;
            let _ = this.update(cx, |picker, cx| {
                picker.rail.dirs = dirs;
                picker.rebuild_rail(cx);
            });
        }));
        #[cfg(not(test))]
        let (runner, hostname): (std::rc::Rc<dyn crate::g2::pl1::process::Runner>, _) = (
            std::rc::Rc::new(crate::g2::pl1::process::ThreadRunner),
            None,
        );
        #[cfg(test)]
        let (runner, hostname) = crate::g2::pl1::source::fixture::services(cx);
        self.start_network(runner.clone(), cx);
        let source = cx.new(|_| Source::new(runner, hostname, state));
        self.rail._devices = Some(cx.observe(&source, |picker, _, cx| picker.device_changed(cx)));
        source.update(cx, |source, cx| source.set_polling(true, cx));
        self.rail.devices = Some(source);
        self.read_network(cx);
        self.rebuild_rail(cx);
    }

    fn awaits(&self, source: &Entity<Source>, path: &str, cx: &gpui::App) -> bool {
        let awaited = &self.rail.awaiting;
        !awaited.is_empty()
            && source
                .read(cx)
                .devices
                .entries
                .iter()
                .any(|entry| &entry.device == awaited && entry.path == path)
    }

    fn device_changed(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.rail.devices.clone() else {
            return;
        };
        for request in source.read(cx).drain_events() {
            use crate::g2::pl1::devices::Effect;
            match request.event {
                // Only the drive this dialog pressed answers it, by device and path, as v0.3.12 deviceOpened matches them.
                Event::Device(Effect::Opened(path)) if self.awaits(&source, &path, cx) => {
                    self.rail.awaiting.clear();
                    self.rail_open(&path, cx);
                }
                Event::Device(Effect::Message(note, error)) => {
                    let text = crate::g2::pl1::words::say(&self.locale.catalog, &note);
                    self.say(text, error, cx);
                }
                Event::Device(Effect::Operation {
                    headline: Some(note),
                    ..
                }) => {
                    let text = crate::g2::pl1::words::say(&self.locale.catalog, &note);
                    self.say(text, true, cx);
                }
                _ => {}
            }
        }
        self.read_network(cx);
        self.rebuild_rail(cx);
    }

    pub(super) fn refresh_mounts(&mut self, cx: &mut Context<Self>) {
        if let Some(source) = self.rail.devices.clone() {
            source.update(cx, |source, cx| source.poll_mounts(cx));
        }
    }

    /// PickerPlaces.qml choose(): a place opens in the listing, a drive mounts first; a favourite's diagnostic is said instead.
    pub fn rail_choose(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.accept_state().submitting() || self.backend_unavailable() {
            return;
        }
        let Some(entry) = self.rail.entries.get(index).cloned() else {
            return;
        };
        self.rail.cursor = index;
        self.rail.awaiting.clear();
        self.cancel_network(cx);
        if let Some(error) = entry.error.as_ref() {
            let text = match error {
                flea::ui_logic_pl4::rail::FavouriteError::Record(record) => {
                    self.locale.catalog.tr(record.key())
                }
                flea::ui_logic_pl4::rail::FavouriteError::Status(line) => line.clone(),
            };
            return self.say(text, true, cx);
        }
        if entry.group == Group::Network || entry.kind == Kind::Phone {
            if entry.kind != Kind::Phone && entry.path.starts_with('/') {
                self.rail_open(&entry.path, cx);
            } else {
                self.open_network(&entry, cx);
            }
            return;
        }
        if entry.group == Group::Device {
            if entry.mounted == Some(true) && entry.path.starts_with('/') {
                return self.rail_open(&entry.path, cx);
            }
            if let Some(source) = self.rail.devices.clone() {
                self.rail.awaiting = entry.device.clone();
                let handled = source.update(cx, |source, cx| source.activate(&entry, 0, cx));
                if handled != Activation::Handled {
                    self.rail.awaiting.clear();
                }
                self.device_changed(cx);
            }
            return;
        }
        if entry.kind == Kind::Recent {
            return self.rail_open(logic::RECENT, cx);
        }
        if let Some(path) = local_path(&entry.path) {
            self.rail_open(&path, cx);
        } else if entry.path.contains("://") {
            self.open_network(&entry, cx);
        }
    }

    pub(super) fn rail_open(&mut self, path: &str, cx: &mut Context<Self>) {
        self.rail.hand_off = true;
        self.open_folder(path, cx);
    }

    /// Up, Down, Home and End walk the rail's cursor; Enter, Space and Right open its row, as PickerPlaces.qml's keys do.
    pub(super) fn rail_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.network_key(event, cx) {
            return true;
        }
        if !self.rail.focus.is_focused(window) {
            return false;
        }
        let held = event.keystroke.modifiers;
        if held.control || held.alt || held.platform {
            return false;
        }
        let last = self.rail.entries.len().saturating_sub(1);
        let cursor = self.rail.cursor;
        self.rail.cursor = match crate::window::input::reading(
            event.keystroke.key.as_str(),
            window.layout_mirrored(),
        ) {
            "up" | "k" => cursor.saturating_sub(1),
            "down" | "j" => (cursor + 1).min(last),
            "home" => 0,
            "end" => last,
            "enter" | "space" | "right" | "l" => {
                self.rail_choose(cursor, cx);
                return true;
            }
            _ => return false,
        };
        cx.notify();
        true
    }
}

fn labels(catalog: &I18n, entries: &[RailEntry]) -> Vec<SharedString> {
    entries
        .iter()
        .map(|entry| row_label(catalog, entry))
        .collect()
}

// Local favourites resolve before a remote URI is handed to the network opener.
fn local_path(path: &str) -> Option<String> {
    if path.starts_with('/') {
        return Some(path.to_owned());
    }
    recent::path_of(path).filter(|path| path.starts_with('/'))
}
