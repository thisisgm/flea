//! ui/js/ColumnMenu.js and the neighbour taps: a peek's empty space navigates to its drawn directory and opens the menu once its rows land.
use super::geometry::Slot;
use crate::navigation::history::{join, trim_slash};
use crate::pane::{states::Kind, Pane};

/// The pane states that refuse a background intent, in canArm's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Busy {
    pub in_flight: bool,
    pub trash_opened: bool,
    pub trash_confirming: bool,
    pub search: bool,
    pub collide_opened: bool,
    pub collide_pending: bool,
    pub menu_visible: bool,
    pub menu_action_open: bool,
    pub renaming: bool,
    pub rename_pending: bool,
    pub filter_typing: bool,
    pub band: bool,
}

/// "" when a background intent may arm, else the reason, decided before any intent is stored.
pub fn can_arm(pane: Option<&Busy>) -> &'static str {
    let Some(pane) = pane else { return "no pane" };
    match () {
        _ if pane.in_flight => "loading",
        _ if pane.trash_opened || pane.trash_confirming => "trash",
        _ if pane.search => "search",
        _ if pane.collide_opened || pane.collide_pending => "collide",
        _ if pane.menu_visible || pane.menu_action_open => "menu",
        _ if pane.renaming || pane.rename_pending => "rename",
        _ if pane.filter_typing => "filter",
        _ if pane.band => "band",
        _ => "",
    }
}

pub fn same_path(a: &str, b: &str) -> bool {
    trim_slash(a) == trim_slash(b)
}

/// The target is already shown: the menu opens where it stands, no navigation.
pub fn direct_open(target: &str, pane_path: &str) -> bool {
    !target.is_empty() && same_path(target, pane_path)
}

/// Landed rows open the deferred menu only for the drawn target's own successful listing, empty included.
pub fn should_open(pane_path: &str, pending: &str, state: Kind) -> bool {
    !pending.is_empty()
        && same_path(pane_path, pending)
        && matches!(state, Kind::Ready | Kind::Empty)
}

/// The pane a neighbour background routes through.
pub trait Navigator {
    fn path(&self) -> &str;
    fn busy(&self) -> Busy;
    fn in_flight(&self) -> bool;
    fn listing_state(&self) -> Kind;
    fn open(&mut self, path: &str);
    /// The pane.busy message, translated by the pane's catalog.
    fn say_busy(&mut self);
    fn open_background_menu(&mut self, at: (f32, f32));
}

#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    Opened,
    Navigating,
    Ignored,
    Refused(&'static str),
}

/// The deferred neighbour background: the drawn directory and the point the menu opens at.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Background {
    pub target: String,
    pub at: Option<(f32, f32)>,
}

impl Background {
    /// routeBackground: busy refuses first, the shown folder opens in place, any other target navigates with the menu waiting.
    pub fn route(
        &mut self,
        pane: Option<&mut dyn Navigator>,
        base: &str,
        at: Option<(f32, f32)>,
    ) -> Route {
        let (Some(pane), Some(point)) = (pane, at) else {
            return Route::Ignored;
        };
        if base.is_empty() {
            return Route::Ignored;
        }
        let busy = can_arm(Some(&pane.busy()));
        if !busy.is_empty() {
            if busy == "loading" {
                pane.say_busy();
            }
            return Route::Refused(busy);
        }
        if direct_open(base, pane.path()) {
            pane.open_background_menu(point);
            return Route::Opened;
        }
        *self = Self {
            target: base.to_owned(),
            at: Some(point),
        };
        pane.open(base);
        if !pane.in_flight() {
            *self = Self::default();
            return Route::Refused("not-started");
        }
        Route::Navigating
    }

    /// A failed listing drops the deferred menu, so no later rows open it.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// applyPendingBackground: consumed either way, opened only over the drawn target's own landed rows.
    pub fn apply(&mut self, pane: &mut dyn Navigator) -> bool {
        let Self { target, at } = std::mem::take(self);
        if target.is_empty() || !should_open(pane.path(), &target, pane.listing_state()) {
            return false;
        }
        let busy = pane.busy();
        if busy.search || busy.trash_opened {
            return false;
        }
        let Some(at) = at else { return false };
        pane.open_background_menu(at);
        true
    }
}

/// The menu a navigation defers until its rows land: a neighbour background's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Deferred {
    pub background: Background,
}

impl Deferred {
    /// onFailed: a terminal failure or an ended listing drops it; any other failure leaves the listing, and it, still out.
    pub fn failed(&mut self, operation: &str, ended: bool) {
        if ended || matches!(operation, "backend" | "read") {
            *self = Self::default();
        }
    }

    /// openWithoutHistory: a hop elsewhere drops a waiting background menu, a hop to its own target keeps it.
    pub fn hop(&mut self, path: &str) {
        if !self.background.target.is_empty() && !same_path(path, &self.background.target) {
            self.background.clear();
        }
    }

    /// applyPendingBackground on a landing: the point the shell opens the menu at, the intent consumed either way.
    pub fn landed(&mut self, pane: &Pane) -> Option<(f32, f32)> {
        let mut landed = Landed { pane, at: None };
        self.background.apply(&mut landed);
        landed.at
    }
}

/// The landed pane a deferred background reads; the menu itself opens in the shell.
struct Landed<'a> {
    pane: &'a Pane,
    at: Option<(f32, f32)>,
}

impl Navigator for Landed<'_> {
    fn path(&self) -> &str {
        self.pane.path()
    }
    fn busy(&self) -> Busy {
        Busy {
            in_flight: self.pane.busy(),
            search: self.pane.state.search,
            ..Busy::default()
        }
    }
    fn in_flight(&self) -> bool {
        self.pane.busy()
    }
    fn listing_state(&self) -> Kind {
        self.pane.state.kind
    }
    fn open(&mut self, _path: &str) {}
    fn say_busy(&mut self) {}
    fn open_background_menu(&mut self, at: (f32, f32)) {
        self.at = Some(at);
    }
}

impl super::Area {
    /// menuOnNeighbourBackground's base: the directory a drawn neighbour routes its empty space to, never the active track's.
    pub fn background_base(&self, slot: Slot) -> Option<String> {
        let view = self.tracks().iter().find(|view| view.slot == slot)?;
        let routes = match slot {
            Slot::Active => false,
            Slot::Child => !view.path.is_empty(),
            _ => view.lifted.is_some(),
        };
        routes.then(|| view.path.clone())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    None,
    OpenTab,
    Reveal,
    Open,
}

/// Tap.tappedColumn: one tap on a peeked folder reveals it, a second tap opens a file, middle opens a folder in a tab.
pub fn tapped_column(dir: Option<bool>, button: Button, taps: u32) -> Verb {
    match (dir, button) {
        (None, _) | (_, Button::Right) => Verb::None,
        (Some(true), Button::Middle) if taps == 1 => Verb::OpenTab,
        (_, Button::Middle) => Verb::None,
        (Some(true), Button::Left) if taps == 1 => Verb::Reveal,
        (Some(false), Button::Left) if taps == 2 => Verb::Open,
        _ => Verb::None,
    }
}

/// What a neighbour row asks of the pane: a folder becomes the listing, a file goes to the opener.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Activate {
    Place(String),
    File(String),
}

pub fn activate_neighbour(base: &str, name: &str, is_dir: bool) -> Activate {
    let target = join(base, name);
    if is_dir {
        Activate::Place(target)
    } else {
        Activate::File(target)
    }
}

/// menuOnNeighbour: the peek's directory a neighbour tap opens and the row it selects there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeighbourMenu {
    pub open: String,
    pub select: String,
}

pub fn menu_on_neighbour(base: &str, name: &str) -> NeighbourMenu {
    NeighbourMenu {
        open: base.to_owned(),
        select: join(base, name),
    }
}
