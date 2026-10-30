//! tests/js/columns.js runColumnMenu head: ColumnMenu.js's path rule, busy gate and the neighbour background routing.
use super::check::Checks;
use flea_gui::columns::menu::{
    can_arm, direct_open, same_path, Background, Busy, Navigator as MenuPane, Route,
};
use flea_gui::navigation::history::trim_slash;
use flea_gui::pane::Kind;

/// The pane routeBackground reads, recording what it said, asked for and opened.
struct Bg {
    path: String,
    in_flight: bool,
    state: Kind,
    starts: bool,
    said: Vec<String>,
    asked: Vec<String>,
    opened: Vec<(f32, f32)>,
}

impl Bg {
    fn at(path: &str, state: Kind) -> Self {
        Self {
            path: path.to_owned(),
            in_flight: false,
            state,
            starts: false,
            said: Vec::new(),
            asked: Vec::new(),
            opened: Vec::new(),
        }
    }

    /// A pane whose open starts a listing, so the hop is in flight once asked.
    fn starting(path: &str) -> Self {
        Self {
            starts: true,
            ..Self::at(path, Kind::Ready)
        }
    }
}

impl MenuPane for Bg {
    fn path(&self) -> &str {
        &self.path
    }
    fn busy(&self) -> Busy {
        Busy {
            in_flight: self.in_flight,
            ..Busy::default()
        }
    }
    fn in_flight(&self) -> bool {
        self.in_flight
    }
    fn listing_state(&self) -> Kind {
        self.state
    }
    fn open(&mut self, path: &str) {
        self.asked.push(path.to_owned());
        if self.starts {
            self.in_flight = true;
        }
    }
    fn say_busy(&mut self) {
        self.said
            .push(crate::test_catalogs::english().tr("pane.busy"));
    }
    fn open_background_menu(&mut self, at: (f32, f32)) {
        self.opened.push(at);
    }
}

fn route(pane: &mut Bg, background: &mut Background, base: &str, at: (f32, f32)) -> Route {
    background.route(Some(pane as &mut dyn MenuPane), base, Some(at))
}

fn waiting(target: &str, at: (f32, f32)) -> Background {
    Background {
        target: target.to_owned(),
        at: Some(at),
    }
}

#[test]
fn the_busy_gate_refuses_before_any_intent_is_stored() {
    let mut c = Checks::new("tests/js/columns.js");
    c.eq(
        432,
        "a trailing slash trims except at the root",
        trim_slash("/a/"),
        "/a",
    );
    c.eq(433, "the root keeps its own slash", trim_slash("/"), "/");
    c.eq(
        434,
        "same path ignores a trailing slash",
        same_path("/a/", "/a"),
        true,
    );
    c.eq(
        435,
        "different paths stay different",
        same_path("/a", "/b"),
        false,
    );
    let idle = Busy::default();
    let gates = [
        (436, "an idle pane may arm", idle, ""),
        (
            437,
            "a listing in flight refuses before any intent",
            Busy {
                in_flight: true,
                ..idle
            },
            "loading",
        ),
        (
            438,
            "an open trash refuses",
            Busy {
                trash_opened: true,
                ..idle
            },
            "trash",
        ),
        (
            439,
            "a confirming trash refuses",
            Busy {
                trash_confirming: true,
                ..idle
            },
            "trash",
        ),
        (
            440,
            "a search refuses",
            Busy {
                search: true,
                ..idle
            },
            "search",
        ),
        (
            441,
            "a card waiting refuses",
            Busy {
                collide_pending: true,
                ..idle
            },
            "collide",
        ),
        (
            442,
            "an open menu refuses a second",
            Busy {
                menu_visible: true,
                ..idle
            },
            "menu",
        ),
        (
            443,
            "a menu action in flight refuses too",
            Busy {
                menu_action_open: true,
                ..idle
            },
            "menu",
        ),
        (
            444,
            "an open rename refuses",
            Busy {
                renaming: true,
                ..idle
            },
            "rename",
        ),
        (
            445,
            "a pending rename refuses as well",
            Busy {
                rename_pending: true,
                ..idle
            },
            "rename",
        ),
        (
            446,
            "a filter line with the caret refuses",
            Busy {
                filter_typing: true,
                ..idle
            },
            "filter",
        ),
        (
            447,
            "a selection band refuses",
            Busy { band: true, ..idle },
            "band",
        ),
        (
            448,
            "an open collision card refuses",
            Busy {
                collide_opened: true,
                ..idle
            },
            "collide",
        ),
        // A collision slot holding null and a trash with neither flag are the idle flags.
        (449, "a null collision slot arms", idle, ""),
        (450, "an empty trash slot arms", idle, ""),
    ];
    for (line, label, pane, want) in gates {
        c.eq(line, label, can_arm(Some(&pane)), want);
    }
    c.eq(451, "no pane refuses", can_arm(None), "no pane");
    c.eq(
        452,
        "the shown folder opens where it stands",
        direct_open("/a", "/a/"),
        true,
    );
    c.eq(
        453,
        "another folder navigates instead",
        direct_open("/a", "/b"),
        false,
    );
    c.eq(
        454,
        "no target navigates nowhere",
        direct_open("", "/b"),
        false,
    );
    c.finish();
}

#[test]
fn a_neighbour_background_refuses_opens_in_place_or_navigates() {
    let mut c = Checks::new("tests/js/columns.js");
    let mut sample = Bg::at("/a", Kind::Ready);
    let routed = route(&mut sample, &mut Background::default(), "/a", (1., 0.));
    c.eq(
        463,
        "the sample opens the shown folder where it stands",
        routed,
        Route::Opened,
    );
    c.eq(
        464,
        "through the real menu entrance",
        sample.opened.len(),
        1,
    );
    let mut idle = Bg::starting("/a");
    let mut armed = Background::default();
    let routed = route(&mut idle, &mut armed, "/b", (1., 2.));
    c.eq(
        466,
        "an idle hop arms and navigates",
        routed,
        Route::Navigating,
    );
    c.eq(
        467,
        "with the drawn target stored",
        armed,
        waiting("/b", (1., 2.)),
    );
    c.eq(
        468,
        "and the listing asked for",
        idle.asked.join("|").as_str(),
        "/b",
    );
    let mut busy = Bg::at("/a", Kind::Ready);
    busy.in_flight = true;
    let mut refused = Background::default();
    let routed = route(&mut busy, &mut refused, "/b", (1., 2.));
    c.eq(
        471,
        "a busy hop refuses before storing anything",
        routed,
        Route::Refused("loading"),
    );
    c.eq(472, "leaving no intent behind", refused.target.as_str(), "");
    c.eq(473, "and asking for no listing", busy.asked.len(), 0);
    c.eq(
        474,
        "while the refusal says to wait",
        busy.said.join("|").as_str(),
        "A directory is already loading.",
    );
    let mut same = Bg::at("/a", Kind::Ready);
    let routed = route(&mut same, &mut Background::default(), "/a/", (3., 4.));
    c.eq(
        476,
        "the shown folder opens where it stands",
        routed,
        Route::Opened,
    );
    c.eq(477, "leaving the listing alone", same.asked.len(), 0);
    c.eq(478, "at the stored point", same.opened, vec![(3., 4.)]);
    let mut unstated = Bg::at("/a", Kind::Ready);
    let routed = route(&mut unstated, &mut Background::default(), "", (1., 2.));
    c.eq(480, "a targetless tap is ignored", routed, Route::Ignored);
    let mut unstarted = Bg::at("/a", Kind::Ready);
    let mut dropped = Background::default();
    let routed = route(&mut unstarted, &mut dropped, "/b", (1., 2.));
    c.eq(
        482,
        "an open that never starts is refused",
        routed,
        Route::Refused("not-started"),
    );
    c.eq(
        483,
        "clearing the intent it just stored",
        dropped,
        Background::default(),
    );
    c.finish();
}
