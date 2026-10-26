//! Places menus keep their captured path across rail polls and recheck a favorite before removal.
use super::{join::Captured, menu::Face};
use crate::{state::Shell, window::navigate::Step};
use flea::ui_logic_pl4::rail::{Kind, RailEntry};
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
}

impl Shell {
    fn rail_place_menu(&self, entry: &RailEntry) -> RailMenu {
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
        Some(match self.rail_place_menu(&place.entry) {
            RailMenu::Place { key, rows } | RailMenu::LegacyFavourite { key, rows }
                if key == place.key =>
            {
                rows
            }
            _ => Vec::new(),
        })
    }

    pub(crate) fn open_place_menu(
        &mut self,
        side: usize,
        entry: &RailEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.retire_menu(window, cx);
        let (key, entries) = match self.rail_place_menu(entry) {
            RailMenu::Place { key, rows } | RailMenu::LegacyFavourite { key, rows } => (key, rows),
            _ => return,
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
        });
        self.menu.place = Some(Place {
            key: key.clone(),
            entry: entry.clone(),
        });
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
        let face = self.active_typeface();
        let viewport = window.viewport_size();
        self.menu.menu.borrow_mut().open(
            Face::Place,
            entries,
            at,
            Rect {
                x: 0.,
                y: 0.,
                w: viewport.width.into(),
                h: viewport.height.into(),
            },
            &key,
            &self.menu_env(&face),
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
