//! The rail's Trash row opens the shared menu without taking a listing selection; Places rows open theirs in g2/context_menus/place_host.rs.
use super::Order;
use crate::{
    g2::context_menus::{
        join::Captured,
        menu::{Face, Outcome},
    },
    state::Shell,
};
use flea::ui_logic_context_menus::{build, Entry};
use flea::ui_logic_rail_places::rail::Kind;
use gpui::{Context, Window};

pub(crate) struct Target {
    side: usize,
    trash_count: i64,
}

impl Target {
    pub(crate) fn rows(&self) -> Vec<Entry> {
        build::trash_entries(self.trash_count, false)
    }
}

pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    let Order::Place(request) = order else {
        return false;
    };
    let crate::g2::rail_places::producer::Event::Menu(entry) = &request.event else {
        return false;
    };
    if entry.kind != Kind::Trash {
        return false;
    }
    shell.open_rail_menu(request.side, window, cx);
    true
}

impl Shell {
    fn open_rail_menu(&mut self, side: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.panes.get(side).and_then(Option::as_ref) else {
            return;
        };
        let Some(join) = &self.rail else { return };
        let producer = join.places.read(cx);
        let at = if producer.model.borrow().pointer_focus {
            let mouse = window.mouse_position();
            (f32::from(mouse.x), f32::from(mouse.y))
        } else {
            let laid = producer.laid.borrow();
            let Some(row) = producer.cursor().and_then(|index| laid.row(index)) else {
                return;
            };
            (row.label_rect.x, row.rect.y + row.rect.height)
        };
        let target = Target {
            side,
            trash_count: producer.model.borrow().trash_count.min(i64::MAX as u64) as i64,
        };
        let rows = target.rows();
        let capture = Captured {
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
        };
        self.retire_menu(window, cx);
        self.observe_menu(window, cx);
        self.menu.sequence += 1;
        let typeface = self.active_typeface();
        let env = self.menu_env(&typeface);
        let size = window.viewport_size();
        self.menu.menu.borrow_mut().open(
            Face::Rail,
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

    pub(crate) fn rail_menu_outcome(
        &mut self,
        outcome: &Outcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.menu.rail.is_none() {
            return false;
        }
        let Outcome::Fire { action, .. } = outcome else {
            return false;
        };
        let target = self.menu.rail.take().expect("rail menu target");
        self.rail_menu_hold(false, cx);
        self.retire_menu(window, cx);
        self.focus_pane(target.side, cx);
        self.trash_action(
            if action == "open" {
                "openTrash"
            } else {
                action
            },
            window,
            cx,
        );
        true
    }
}
