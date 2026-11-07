//! Places menus keep their captured path across rail polls and recheck a favorite before removal.
use super::{join::Captured, menu::Face};
use crate::{state::Shell, window::navigate::Step};
use flea::ui_logic_pl1::{menu::RailAction, mounts::rail_key, Kind as DeviceKind};
use flea::ui_logic_pl4::rail::{Group, Kind, RailEntry};
use flea::{
    jsondoc::Json,
    ui_logic_mn1::{
        build,
        fit::Rect,
        places::{self, PlaceChoice, RailMenu},
        refresh, Entry,
    },
};
use gpui::{ClipboardItem, Context, Window};

#[derive(Clone)]
pub(crate) struct Place {
    pub key: String,
    entry: RailEntry,
    // A device row's own entry as the menu opened over it; a choice resolves its row by key again.
    device: Option<flea::ui_logic_pl1::RailEntry>,
}

impl Shell {
    // RailMenu.railMenuFor: a device row opens Mounts.rowMenu over its own entry, keyed as Mounts.railKey.
    fn rail_place_menu(
        &self,
        entry: &RailEntry,
        device: Option<&flea::ui_logic_pl1::RailEntry>,
    ) -> Option<(String, Vec<Entry>)> {
        if let Some(device) = device {
            let kind = match device.kind {
                DeviceKind::Volume => "volume",
                DeviceKind::Phone => "phone",
                _ => "disk",
            };
            let mount = build::RailEntry {
                group: "device".into(),
                kind: kind.into(),
                mounted: device.mounted,
                locked: device.locked,
                removable: device.removable,
                loop_device: device.loop_device,
                read_only: device.read_only,
                device: device.device.clone(),
                fixed: false,
            };
            return match places::rail_menu_for(kind, &entry.path, None, "", 0, None, &mount) {
                RailMenu::Mount(rows) => Some((format!("device:{}", rail_key(device)), rows)),
                _ => None,
            };
        }
        match self.rail_favourite_menu(entry) {
            RailMenu::Place { key, rows } | RailMenu::LegacyFavourite { key, rows } => {
                Some((key, rows))
            }
            _ => None,
        }
    }

    fn rail_favourite_menu(&self, entry: &RailEntry) -> RailMenu {
        let kind = if entry.kind == Kind::Home {
            "home"
        } else {
            "favourite"
        };
        let record = entry
            .original
            .as_ref()
            .map(flea::jsondoc::render)
            .unwrap_or_default();
        places::rail_menu_for(
            kind,
            &entry.path,
            entry.favourite_index,
            &record,
            0,
            Some(&self.menu_hidden()),
            &build::RailEntry::default(),
        )
    }

    pub(crate) fn place_menu_entries(&self) -> Option<Vec<Entry>> {
        let place = self.menu.place.as_ref()?;
        Some(
            self.rail_place_menu(&place.entry, place.device.as_ref())
                .filter(|(key, _)| *key == place.key)
                .map_or_else(Vec::new, |(_, rows)| rows),
        )
    }

    pub(crate) fn open_place_menu(
        &mut self,
        side: usize,
        entry: &RailEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.retire_menu(window, cx);
        let device = if entry.group == Group::Device {
            let Some(device) = self
                .rail
                .as_ref()
                .and_then(|join| join.devices.read(cx).device_row(entry))
            else {
                return;
            };
            Some(device)
        } else {
            None
        };
        let Some((key, entries)) = self.rail_place_menu(entry, device.as_ref()) else {
            // Mounts.raiseMenu: the sheet advertises m on every rail row, so a row without a menu says so; a right click stays quiet.
            let keyboard = self
                .rail
                .as_ref()
                .is_some_and(|rail| !rail.places.read(cx).model.borrow().pointer_focus);
            if keyboard {
                let label = crate::g2::pl4::content::row_label(&self.locale.catalog, entry);
                let note =
                    flea::ui_logic_pl1::Note::new("device.nothing_to_release", &[label.as_ref()]);
                let text = crate::g2::pl1::words::say(&self.locale.catalog, &note);
                let hint = flea::ui_logic_fb1::status::Hint::None;
                self.status_messages(cx, |messages, now| {
                    messages.say(&text, false, "", hint, now)
                });
                cx.notify();
            }
            return;
        };
        let face = if device.is_some() {
            Face::Rail
        } else {
            Face::Place
        };
        let Some(host) = self.panes.get(side).and_then(Option::as_ref) else {
            return;
        };
        self.menu.sequence += 1;
        self.menu.captured = Some(Captured {
            side,
            listing: host.pane.held.listing,
            rows: Vec::new(),
            cursor: host.pane.cursor,
            path: host.path.clone(),
            identity: key.clone(),
            id: self.menu.sequence,
            ready: true,
            kinds: Vec::new(),
            background: true,
            target: None,
        });
        self.menu.place = Some(Place {
            key: key.clone(),
            entry: entry.clone(),
            device,
        });
        self.rail_menu_hold(true, cx);
        let pointer = window.mouse_position();
        let pointer_at = (f32::from(pointer.x), f32::from(pointer.y));
        let at = self
            .rail
            .as_ref()
            .and_then(|rail| {
                let producer = rail.places.read(cx);
                let model = producer.model.borrow();
                if model.pointer_focus {
                    return Some(pointer_at);
                }
                let laid = producer.laid.borrow();
                laid.row(model.cursor?)
                    .map(|row| (row.rect.x + 14., row.rect.y + row.rect.height))
            })
            .unwrap_or(pointer_at);
        let typeface = self.active_typeface();
        let viewport = window.viewport_size();
        super::keycaps::watch(cx);
        self.menu.menu.borrow_mut().open(
            face,
            entries,
            at,
            Rect {
                x: 0.,
                y: 0.,
                w: viewport.width.into(),
                h: viewport.height.into(),
            },
            &key,
            &self.menu_env(&typeface),
            window,
        );
        self.menu.menu.borrow_mut().handle = key;
        self.menu.pointer = Some(pointer_at);
        self.menu.resting = Some(pointer_at);
        self.menu.pointer_armed = false;
        self.keyboard.context = flea::keys::Context::Menu;
        self.keyboard.disarm();
        window.focus(&self.focus, cx);
        if let Some(host) = self.panes[side].as_mut() {
            host.nav.gate.menu_open = true;
        }
        self.observe_menu(window, cx);
        cx.notify();
    }

    pub(crate) fn activate_place_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(place) = self.menu.place.clone() else {
            return false;
        };
        let Some((action, leaf)) = self.menu.pending.take() else {
            return true;
        };
        let side = self
            .menu
            .captured
            .as_ref()
            .map_or(self.focus_side, |capture| capture.side);
        let entries = self.place_menu_entries().unwrap_or_default();
        let valid = refresh::validate_choice(&entries, &action, &leaf, false).is_ok();
        let choice = places::place_choice(&action, &place.key);
        self.retire_menu(window, cx);
        if !valid {
            self.menu_notice(side, self.locale.catalog.tr("menu.refusal.unavailable"), cx);
            return true;
        }
        if let Some(device) = &place.device {
            self.device_choice(side, &action, &place.entry, device, window, cx);
            return true;
        }
        match choice {
            PlaceChoice::Open(path) => {
                let path = places::decode_file_uri(&path).unwrap_or(path);
                if self
                    .step(side, Step::OpenPlace(path), window, cx)
                    .is_ok_and(|opened| opened)
                {
                    self.rail_focus(false, cx);
                    self.focus_pane(side, cx);
                    window.focus(&self.focus, cx);
                }
            }
            PlaceChoice::OpenTab(path) => {
                let path = places::decode_file_uri(&path).unwrap_or(path);
                self.open_tab(side, path, window, cx);
            }
            PlaceChoice::Terminal(path) => self.open_terminal(
                side,
                places::decode_file_uri(&path).unwrap_or(path),
                window,
                cx,
            ),
            PlaceChoice::CopyPath(path) => cx.write_to_clipboard(ClipboardItem::new_string(path)),
            PlaceChoice::AddFavourite { path, .. } => self.add_favourite(&path, window, cx),
            choice @ (PlaceChoice::RemoveFavourite { .. } | PlaceChoice::RemoveLegacy { .. }) => {
                self.remove_place_favourite(side, &choice, window, cx);
            }
            PlaceChoice::Ignore => {
                self.menu_notice(side, self.locale.catalog.tr("menu.refusal.unavailable"), cx)
            }
        }
        true
    }

    /// RailMenu.release: Open, Mount and Unlock are the row's own activation; the releases and Flash image run on its key.
    fn device_choice(
        &mut self,
        side: usize,
        action: &str,
        row: &RailEntry,
        device: &flea::ui_logic_pl1::RailEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = self.rail.as_ref().map(|join| join.devices.clone()) else {
            return;
        };
        let release = match action {
            "unmountVolume" => Some(RailAction::UnmountVolume),
            "unmountPhone" => Some(RailAction::UnmountPhone),
            "eject" => Some(RailAction::Eject),
            _ => None,
        };
        if let Some(release) = release {
            let key = rail_key(device);
            source.update(cx, |source, cx| source.release_by(release, &key, side, cx));
            return;
        }
        match action {
            "open" | "mountVolume" | "unlockVolume" | "mountPhone" => {
                let owned = source.update(cx, |source, cx| source.activate(row, side, cx));
                if let crate::g2::pl1::source::Activation::Phone(phone) = owned {
                    self.open_phone(side, &phone, window, cx);
                }
            }
            "flashImage" => self.flash_drive(side, &device.device, window, cx),
            _ => self.menu_notice(side, self.locale.catalog.tr("menu.refusal.unavailable"), cx),
        }
    }

    // Pane.openFlashDrive: the flash view opens with the volume's disk to match against its drive list.
    fn flash_drive(
        &mut self,
        side: usize,
        device: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let archive = &mut self.operations.archive;
        if archive.flash.running() && archive.flash_side != side {
            let text = flea::ui_logic_op2::flash::already_running(&self.locale.catalog);
            let effects = vec![crate::g2::op2::Effect::Say { text, error: true }];
            self.archive_effects(side, effects, window, cx);
            return;
        }
        archive.flash_side = side;
        let effects = archive.flash.open_with_drive(device);
        self.archive_effects(side, effects, window, cx);
        // FlashView.qml open(): the view takes the keys, so the rail the menu came from no longer holds them.
        if self.operations.archive.flash.shown() {
            self.flash_take_keys(window, cx);
        }
        cx.notify();
    }

    fn remove_place_favourite(
        &mut self,
        side: usize,
        choice: &PlaceChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let stored = self
            .launch
            .settings
            .data
            .get("places")
            .and_then(|p| p.get("favourites"))
            .cloned()
            .unwrap_or(Json::Arr(Vec::new()));
        let records: Vec<places::Favourite> = stored
            .as_array()
            .unwrap_or(&[])
            .iter()
            .map(|record| places::Favourite {
                label: record
                    .get("label")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_owned(),
                path: record
                    .get("path")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_owned(),
            })
            .collect();
        let index = match choice {
            PlaceChoice::RemoveLegacy { index, record } => stored
                .as_array()
                .and_then(|items| items.get(*index))
                .filter(|item| flea::jsondoc::render(item) == *record)
                .map(|_| *index),
            _ => places::removal_holds(&records, choice),
        };
        let Some(index) = index else {
            self.menu_notice(side, self.locale.catalog.tr(places::FAVOURITES_CHANGED), cx);
            return;
        };
        let operation = Json::Obj(vec![
            ("op".into(), Json::Str("remove".into())),
            ("index".into(), Json::Num(index.to_string())),
            ("expected".into(), stored),
        ]);
        self.settings_favourite(operation, window, cx);
    }
}
