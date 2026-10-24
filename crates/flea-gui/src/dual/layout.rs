//! WindowBody.qml's dualShareOf/shareAt as pure functions: split is the left share in 1/10000, 5000 equal, never rewritten by the clamp.

/// The split scale WindowBody.qml calls dualSplitScale.
pub const SCALE: f64 = 10_000.;
/// The equal split a double click restores.
pub const EQUAL_SPLIT: i64 = 5_000;
/// A press that never travels this far stays a click and writes nothing.
pub const CLICK_TRAVEL_PX: f32 = 1.;
/// Theme.qml:213 at body 14: two 14px pads, a 13 * 1.45 mark, a 9px gap and the 180px name floor.
pub const ROOM_MIN: f32 = 235.85;
/// The grab zone: ResizeHandle.qml's nine pixels from the divider pixel, over the second listing's padding.
pub const ZONE: f32 = 9.;

/// Offsets from the zone's first pixel: WindowBody.qml 462-466 and ResizeHandle.qml 42-50, muted at 0, accent at dx while lit.
pub fn line_offsets(lit: bool, dx: f32) -> (f32, Option<f32>) {
    (0., lit.then_some(dx))
}

/// One divider drag: press point, left width then, and preview offset now; only the handle moves until the release re-lays both.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drag {
    start_x: f32,
    left: f32,
    dx: f32,
    moved: bool,
}

impl Drag {
    pub fn begin(start_x: f32, left: f32) -> Self {
        Self {
            start_x,
            left,
            dx: 0.,
            moved: false,
        }
    }

    /// The accent line follows the pointer, held to the clamp.
    pub fn travel_to(&mut self, x: f32, room: f32, min: f32) -> f32 {
        let travel = x - self.start_x;
        if !self.moved && travel.abs() < CLICK_TRAVEL_PX {
            return 0.;
        }
        self.moved = true;
        self.dx = room * share_at(self.left, travel, room, min) - self.left;
        self.dx
    }

    pub fn dx(self) -> f32 {
        self.dx
    }

    pub fn live(self) -> bool {
        self.moved
    }

    /// One leaf on release; a click or a narrow-room drag commits nothing.
    pub fn finish(self, room: f32, min: f32) -> Option<i64> {
        (self.moved && fits(room, min)).then(|| split_of(self.left + self.dx, room))
    }
}

/// The room fits two narrowest listings; a narrower room lays them equal.
pub fn fits(room: f32, min: f32) -> bool {
    min / room <= 0.5
}

/// Held between the narrowest listing and its mirror, equal when the room fits no two; never rewrites the stored split.
pub fn share_of(split: f64, room: f32, min: f32) -> f32 {
    if room <= 0. || min <= 0. {
        return 0.5;
    }
    let low = min / room;
    if low > 0.5 {
        return 0.5;
    }
    (split / SCALE).clamp(f64::from(low), f64::from(1. - low)) as f32
}

/// One leaf on release, so both listings re-lay from it in the same frame.
pub fn split_of(left: f32, room: f32) -> i64 {
    if room <= 0. {
        return EQUAL_SPLIT;
    }
    (f64::from(left / room) * SCALE).round() as i64
}

/// The divider's shareAt: the share while the pointer travels from the left listing's width, held to the clamp.
pub fn share_at(left: f32, travel: f32, room: f32, min: f32) -> f32 {
    let split = f64::from((left + travel) / room) * SCALE;
    share_of(split, room, min)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: f32 = 240.;
    const ROOM: f32 = 908.;

    #[test]
    fn default_split_lays_equal_listings() {
        let share = share_of(EQUAL_SPLIT as f64, ROOM, MIN);
        assert!((share - 0.5).abs() < 1e-6, "equal split, got {share}");
        assert_eq!(split_of(ROOM * share, ROOM), EQUAL_SPLIT);
    }

    #[test]
    fn stored_split_is_never_rewritten_by_the_clamp() {
        assert!((share_of(100., ROOM, MIN) - MIN / ROOM).abs() < 1e-6);
        assert!((share_of(9_900., ROOM, MIN) - (1. - MIN / ROOM)).abs() < 1e-6);
    }

    #[test]
    fn stored_6500_lays_65_percent() {
        let share = share_of(6_500., ROOM, MIN);
        assert!((share - 0.65).abs() < 1e-6, "65 percent, got {share}");
        assert_eq!(split_of(ROOM * 0.65, ROOM), 6_500);
    }

    #[test]
    fn narrow_room_lays_equal_and_moves_nothing() {
        let room = 2. * MIN - 20.;
        assert!(!fits(room, MIN));
        assert_eq!(share_of(6_500., room, MIN), 0.5);
        assert_eq!(share_of(100., room, MIN), 0.5);
    }

    #[test]
    fn drag_travel_roundtrips_through_share() {
        let left = ROOM / 2.;
        let share = share_at(left, 100., ROOM, MIN);
        assert_eq!(split_of(ROOM * share, ROOM), split_of(left + 100., ROOM));
        assert_eq!(split_of(left + 100., ROOM), 6_101);
    }

    #[test]
    fn drag_commits_once_clicks_commit_nothing() {
        let mut drag = Drag::begin(500., 454.);
        assert_eq!(drag.travel_to(500.4, ROOM, MIN), 0.);
        assert!(!drag.live());
        assert_eq!(drag.finish(ROOM, MIN), None);
        let mut drag = Drag::begin(500., 454.);
        assert!((drag.travel_to(600., ROOM, MIN) - 100.).abs() < 1e-3);
        assert!(drag.live());
        assert_eq!(drag.finish(ROOM, MIN), Some(6_101));
    }

    #[test]
    fn narrow_drag_keeps_the_stored_split() {
        let room = 2. * MIN - 20.;
        let mut drag = Drag::begin(200., room / 2.);
        drag.travel_to(300., room, MIN);
        assert!(drag.live());
        assert_eq!(drag.finish(room, MIN), None);
    }
}
