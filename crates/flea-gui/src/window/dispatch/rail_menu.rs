//! Rail places and Trash use the shared menu without taking a listing selection.
use super::Order;
use crate::{g2::mn1::{join::Captured, menu::{Face, Outcome}}, state::Shell, window::navigate::Step};
use flea::{jsondoc::Json, ui_logic_mn1::{build, places, Entry}, ui_logic_pl4::rail::{Kind, RailEntry}};
use gpui::{ClipboardItem, Context, Window};

pub(crate) struct Target {
    side: usize,
    entry: RailEntry,
    record: Option<Json>,
    trash_count: i64,
}

impl Target {
    pub(crate) fn rows(&self, shell: &Shell) -> Vec<Entry> {
        if self.entry.kind == Kind::Trash {
            return build::trash_entries(self.trash_count, false);
        }
        let hidden = shell.menu_hidden();
        let rows = places::place_menu(Some(&hidden), self.entry.favourite_index.is_some());
        if !rows.is_empty() {
            return rows;
        }
        if self.entry.favourite_index.is_some() {
            return build::rail_entries(&build::RailEntry { group: "favourite".into(), kind: "favourite".into(), ..Default::default() });
        }
        Vec::new()
    }
}

pub(super) fn run(shell: &mut Shell, order: &Order, window: &mut Window, cx: &mut Context<Shell>) -> bool {
    let Order::Place(request) = order else { return false };
    let crate::g2::pl4::producer::Event::Menu(entry) = &request.event else { return false };
    if !matches!(entry.kind, Kind::Home | Kind::Favourite | Kind::Favorite | Kind::Trash) {
        return false;
    }
    shell.open_rail_menu(request.side, entry.clone(), window, cx);
    true
}

impl Shell {
    fn open_rail_menu(&mut self, side: usize, entry: RailEntry, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.panes.get(side).and_then(Option::as_ref) else { return };
        let Some(join) = &self.rail else { return };
        let producer = join.places.read(cx);
        let at = if producer.model.borrow().pointer_focus {
            let mouse = window.mouse_position();
            (f32::from(mouse.x), f32::from(mouse.y))
        } else {
            let laid = producer.laid.borrow();
            let Some(row) = producer.cursor().and_then(|index| laid.row(index)) else { return };
            (row.label_rect.x, row.rect.y + row.rect.height)
        };
        let record = entry.favourite_index.and_then(|index| producer.places.read(cx).favourites.records.get(index).cloned());
        let target = Target { side, entry, record, trash_count: producer.model.borrow().trash_count.min(i64::MAX as u64) as i64 };
        let rows = target.rows(self);
        if rows.is_empty() { return }
        let capture = Captured { side, listing: host.pane.held.listing, rows: Vec::new(), cursor: host.pane.cursor, path: host.path.clone(), identity: String::new(), id: 0, ready: true, kinds: Vec::new(), background: true };
        self.retire_menu(window, cx);
        self.observe_menu(window, cx);
        self.menu.sequence += 1;
        let face = if target.entry.kind == Kind::Trash { Face::Rail } else { Face::Place };
        let typeface = self.active_typeface();
        let env = self.menu_env(&typeface);
        let size = window.viewport_size();
        self.menu.menu.borrow_mut().open(face, rows, at, flea::ui_logic_mn1::fit::Rect { x: 0., y: 0., w: size.width.into(), h: size.height.into() }, "", &env, window);
        self.menu.captured = Some(capture);
        self.menu.rail = Some(target);
        self.menu.pointer = Some(at);
        self.menu.resting = Some(at);
        self.menu.pointer_armed = false;
        self.keyboard.context = flea::keys::Context::Menu;
        self.keyboard.disarm();
        self.rail_menu_hold(true, cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(crate) fn rail_menu_hold(&mut self, open: bool, cx: &mut Context<Self>) {
        if let Some(join) = &self.rail {
            join.host.update(cx, |host, cx| host.menu(open, cx));
        }
    }

    pub(crate) fn rail_menu_outcome(&mut self, outcome: &Outcome, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.menu.rail.is_none() { return false }
        let Outcome::Fire { action, .. } = outcome else { return false };
        let target = self.menu.rail.take().expect("rail menu target");
        self.rail_menu_hold(false, cx);
        self.retire_menu(window, cx);
        let side = target.side;
        let entry = &target.entry;
        if entry.kind == Kind::Trash {
            self.focus_pane(side, cx);
            self.trash_action(if action == "open" { "openTrash" } else { action }, window, cx);
            return true;
        }
        match action.as_str() {
            "open" if entry.error.is_none() => {
                let _ = self.step(side, Step::OpenPlace(entry.path.clone()), window, cx);
            }
            "openTab" => {
                self.dispatch(Order::Place(crate::g2::pl4::producer::Request { side, id: 0, event: crate::g2::pl4::producer::Event::Tab(entry.clone()) }), window, cx);
            }
            "openTerminal" => self.open_terminal(side, entry.path.clone(), window, cx),
            "copypath" => cx.write_to_clipboard(ClipboardItem::new_string(entry.path.clone())),
            "addFavourite" => self.add_favourite(&entry.path, window, cx),
            "removeFavourite" => {
                let Some(producer) = self.rail.as_ref().map(|join| join.places.clone()) else { return true };
                let index = entry.favourite_index;
                let live = index.and_then(|index| producer.read(cx).places.read(cx).favourites.records.get(index).cloned());
                if target.record.is_some() && live == target.record {
                    producer.update(cx, |producer, cx| producer.write_favourite(&flea::ui_logic_pl4::favourites::Operation::Remove { index: index.expect("favourite index") }, cx));
                } else {
                    self.menu_notice(side, self.locale.catalog.tr(places::FAVOURITES_CHANGED), cx);
                }
            }
            _ => {}
        }
        cx.notify();
        true
    }
}
