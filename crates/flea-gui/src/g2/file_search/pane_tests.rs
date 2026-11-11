// The exit against G1.02's real swap: the walk is held until the folder lands, and late session lines drop.
use crate::pane::{
    states::{Kind, State},
    swap::{Options, Session as SwapSession, Swap},
    Pane,
};
use flea::{
    protocol::{Reply, RowFacts},
    ui_logic_file_search::{Line, Mode, Session},
};

fn walked() -> State {
    State {
        kind: Kind::Ready,
        search: true,
        ..State::default()
    }
}

#[test]
fn the_exit_holds_the_walk_and_any_other_listing_forgets_it() {
    // searchnoblank.js:56
    let mut exit = Swap::default();
    assert!(exit.begin(
        "/d".into(),
        40,
        &walked(),
        Options {
            hold_walk: true,
            ..Options::default()
        },
        0
    ));
    assert!(exit.holding);
    // searchnoblank.js:58
    let mut other = Swap::default();
    other.begin("/d".into(), 40, &walked(), Options::default(), 0);
    assert!(!other.holding);
    // searchnoblank.js:60
    let mut standing = Swap::default();
    standing.begin(
        "/d".into(),
        40,
        &State {
            kind: Kind::Ready,
            ..State::default()
        },
        Options {
            hold_walk: true,
            ..Options::default()
        },
        0,
    );
    assert!(standing.holding);
}

#[test]
fn the_swap_drops_exactly_the_lines_the_core_session_refuses() {
    let leaving = super::Search {
        session: Session {
            mode: Mode::Typing,
            gen: 9,
            query: "nor".into(),
            leaving: true,
            ..Session::default()
        },
        ..super::Search::default()
    };
    let mut core = leaving.session.clone();
    // searchnoblank.js:62: a late session line never lands once the exit is out.
    assert!(leaving.pane_session().drops(9));
    let late = Line::Searched {
        n: 4,
        scanned: 9,
        ms: 1.,
        cancelled: false,
        gen: 9,
        walking: false,
        partial: None,
    };
    assert!(core.searched(&late, 5).is_empty() && core.total == 0);
    // searchnoblank.js:64: the exit's own directory lines, gen 0, still land.
    assert!(!leaving.pane_session().drops(0));
    for session in [
        Session {
            mode: Mode::Typing,
            gen: 9,
            opened_gen: 2,
            ..Session::default()
        },
        Session {
            mode: Mode::Results,
            gen: 3,
            opened_gen: 3,
            ..Session::default()
        },
        Session::default(),
    ] {
        let search = super::Search {
            session,
            ..super::Search::default()
        };
        for gen in [1, 2, 3, 8, 9, 10] {
            assert_eq!(
                search.pane_session().drops(gen),
                !search.session.accepts(gen as u64),
                "gen {gen}"
            );
        }
    }
}

fn rows(names: &[&str]) -> Vec<RowFacts> {
    names
        .iter()
        .map(|name| RowFacts {
            name: (*name).into(),
            ..RowFacts::default()
        })
        .collect()
}

#[test]
fn a_gen_stamped_pair_replaces_the_folder_rows_in_one_commit() {
    let mut pane = Pane::new("/d".into(), 620.);
    assert!(!pane
        .receive(Reply::Listed {
            path: "/d".into(),
            total: 3
        })
        .unwrap());
    assert!(pane
        .receive(Reply::Rows {
            start: 0,
            rows: rows(&["a", "b", "c"]),
            listing: 1
        })
        .unwrap());
    let search = super::Search {
        session: Session {
            mode: Mode::Typing,
            gen: 2,
            query: "b".into(),
            ..Session::default()
        },
        ..super::Search::default()
    };
    pane.swap.session = search.pane_session();
    // The listed line alone changes nothing drawn: the folder rows stand until the result rows land.
    assert!(!pane
        .receive_session(
            Reply::Listed {
                path: "/d".into(),
                total: 1
            },
            2,
            10
        )
        .unwrap());
    assert_eq!(pane.held.total, 3);
    assert!(pane
        .receive_session(
            Reply::Rows {
                start: 0,
                rows: rows(&["sub/b"]),
                listing: 2
            },
            2,
            11
        )
        .unwrap());
    assert_eq!(
        (pane.held.total, pane.held.rows[0].name.as_str()),
        (1, "sub/b")
    );
    assert!(pane.state.search && pane.state.query == "b");
    assert_eq!(pane.swap.counters.blank_frames, 0);
    // A line from an older gen is refused before it reaches the rows.
    assert!(!pane
        .receive_session(
            Reply::Rows {
                start: 0,
                rows: rows(&["x"]),
                listing: 3
            },
            1,
            12
        )
        .unwrap());
    assert_eq!(pane.held.rows[0].name, "sub/b");
}

// The window's host over a real pane: the swap's session copy is whatever Search published, never a hand-set value.
struct Real {
    pane: Pane,
    sent: Vec<String>,
}

impl super::Host for Real {
    fn pane(&self) -> super::HostPane<'_> {
        super::HostPane {
            path: self.pane.path(),
            home: "/h",
            hidden: false,
            count: self.pane.held.total,
            listing: self.pane.held.listing,
            sizes: 0,
            cursor_name: None,
            recent: None,
            now: 1000,
        }
    }
    fn send(&mut self, line: String) {
        self.sent.push(line);
    }
    fn leave_recent(&mut self, _: &str) {}
    fn reset_landed(&mut self) {}
    fn relist(&mut self, path: &str, select: &str) {
        // No cursor file to restore is no restore name, so the relist never parks waiting for one.
        let restore_name = (!select.is_empty()).then(|| select.into());
        let options = Options {
            hold_walk: true,
            restore_name,
            ..Options::default()
        };
        assert!(self.pane.begin(path.into(), options, 20));
    }
    fn reveal(&mut self, _: &str, _: &str) {}
    fn restart_settle(&mut self) {}
    fn clipboard_text(&self) -> Option<String> {
        None
    }
    fn publish(&mut self, session: SwapSession) {
        self.pane.swap.session = session;
    }
}

#[test]
fn the_pane_filters_session_lines_by_what_search_published() {
    let mut host = Real {
        pane: Pane::new("/d".into(), 620.),
        sent: Vec::new(),
    };
    assert!(!host
        .pane
        .receive(Reply::Listed {
            path: "/d".into(),
            total: 3
        })
        .unwrap());
    assert!(host
        .pane
        .receive(Reply::Rows {
            start: 0,
            rows: rows(&["a", "b", "c"]),
            listing: 1
        })
        .unwrap());
    let mut search = super::Search::default();
    assert!(search.action("search", &mut host));
    search.key(super::StripKey::Text("a".into()), &mut host);
    search.key(super::StripKey::Text("b".into()), &mut host);
    assert!(host.sent.iter().any(|line| line.contains(r#""gen":2"#)));
    // Gen 1 was superseded by gen 2 before it answered, so neither of its lines reaches the rows.
    assert!(!host
        .pane
        .receive_session(
            Reply::Listed {
                path: "/d".into(),
                total: 2
            },
            1,
            10
        )
        .unwrap());
    assert!(!host
        .pane
        .receive_session(
            Reply::Rows {
                start: 0,
                rows: rows(&["a", "b"]),
                listing: 2
            },
            1,
            11
        )
        .unwrap());
    assert_eq!(host.pane.held.total, 3);
    // The newest gen lands in one commit.
    assert!(!host
        .pane
        .receive_session(
            Reply::Listed {
                path: "/d".into(),
                total: 1
            },
            2,
            12
        )
        .unwrap());
    assert!(host
        .pane
        .receive_session(
            Reply::Rows {
                start: 0,
                rows: rows(&["sub/ab"]),
                listing: 3
            },
            2,
            13
        )
        .unwrap());
    assert_eq!(
        (host.pane.held.total, host.pane.held.rows[0].name.as_str()),
        (1, "sub/ab")
    );
    let done =
        r#"{"t":"searched","n":1,"scanned":4,"ms":1.0,"cancelled":false,"gen":2,"walking":false}"#;
    assert!(search.line(done, &mut host).unwrap());
    // Esc leaves: the late gen 2 lines drop while the gen 0 relist of the folder lands.
    search.key(super::StripKey::Escape, &mut host);
    assert!(search.relisting());
    assert!(!host
        .pane
        .receive_session(
            Reply::Rows {
                start: 0,
                rows: rows(&["late"]),
                listing: 3
            },
            2,
            14
        )
        .unwrap());
    assert_eq!(host.pane.held.rows[0].name, "sub/ab");
    assert!(!host
        .pane
        .receive_session(
            Reply::Listed {
                path: "/d".into(),
                total: 3
            },
            0,
            150
        )
        .unwrap());
    assert!(host
        .pane
        .receive_session(
            Reply::Rows {
                start: 0,
                rows: rows(&["a", "b", "c"]),
                listing: 4
            },
            0,
            200
        )
        .unwrap());
    assert_eq!(host.pane.held.total, 3);
    search.landed(&mut host);
    assert!(!search.active());
    assert!(host.pane.swap.session.drops(2));
}
