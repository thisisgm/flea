#[path = "transfer_launch.rs"]
mod transfer_launch;

use super::*;
use flea::jsondoc;
use flea_gui::tabs::{
    state::{pack, Tab},
    transfer::{
        ack_closes, catcher_outcome, drop_decision, drop_index_at, enter_accepts, fresh_launch,
        is_own, new_token, parse, payload, peek_line, peeked, source_rect, take_token,
        tear_off_command, tear_refusal, Decision, Geometry, Launch, Outcome, Rect, ACK_WAIT_MS,
        PEEK_FIRST, TAB_MIME, TEAR_OFF_ENV,
    },
};
use std::collections::BTreeMap;

fn tabbed() -> Side {
    let mut side =
        Side::new(
            Fake::at("/tmp/a").with_rows(|i| if i == 4 { row("note.txt", false) } else { None }),
        );
    let mut b = Tab::at("/tmp/b");
    b.cursor = 2;
    b.view = "grid";
    side.state.tabs = Some(pack(vec![Tab::at("/tmp/a"), b], 0, None));
    side
}

fn tab_payload(side: &mut Side, index: usize, pid: &str, token: &str) -> String {
    side.state.tab_payload(&side.pane, index, pid, token)
}

fn foreign(token: &str, path: &str, view: &str, cursor: &str) -> String {
    payload("222", token, path, view, cursor)
}

#[test]
fn the_private_payload_names_the_source_lift_and_refuses_every_malformed_shape() {
    let mut tabbed = tabbed();
    let info = parse(&tab_payload(&mut tabbed, 0, "111", "tok-1")).unwrap();
    assert_eq!(
        (
            info.pid.as_str(),
            info.token.as_str(),
            info.path.as_str(),
            info.view.as_str(),
            info.cursor.as_str()
        ),
        ("111", "tok-1", "/tmp/a", "list", "note.txt"),
        "a tab encodes its pid, token, folder, view and cursor file"
    );
    let hidden = parse(&tab_payload(&mut tabbed, 1, "111", "tok-2")).unwrap();
    assert_eq!(
        (hidden.path.as_str(), hidden.view.as_str()),
        ("/tmp/b", "grid"),
        "a hidden tab encodes its own snapshot"
    );
    assert_eq!(
        TAB_MIME, "application/x-flea-tab",
        "the lift offers only the private tab type, never a uri-list or text"
    );
    assert_eq!(
        tab_payload(&mut tabbed, 7, "111", "tok-1"),
        "",
        "no such tab offers no payload"
    );
    assert!(
        is_own(&info, "111"),
        "this window's own drag is never a receive"
    );
    let other = parse(&foreign("tok-9", "/tmp/a", "list", "")).unwrap();
    assert!(!is_own(&other, "111"), "another window's is");
    assert!(is_own(&other, "222"), "an explicit pid decides too");
    assert!(!is_own(&other, ""), "an unset pid never owns a drag");
    assert_eq!(parse("not json"), None, "garbage refuses");
    assert_eq!(parse(""), None, "an empty payload refuses");
    assert_eq!(
        parse(r#"["i","/tmp/a","list",""]"#),
        None,
        "the old four-field shape refuses"
    );
    assert_eq!(
        parse(&payload("111", "tok-1", "tmp/a", "list", "")),
        None,
        "a relative path refuses"
    );
    assert_eq!(
        parse(&payload("111", "tok-1", "/tmp/a\0b", "list", "")),
        None,
        "a NUL path refuses"
    );
    assert_eq!(
        parse(&payload("other-instance", "tok-1", "/tmp/a", "list", "")),
        None,
        "a non-numeric pid refuses"
    );
    assert_eq!(
        parse(&payload("111", "", "/tmp/a", "list", "")),
        None,
        "an empty token refuses"
    );
    assert_eq!(
        parse(&payload("111", "tok\u{7}-1", "/tmp/a", "list", "")),
        None,
        "a control character in the token refuses"
    );
    assert_eq!(
        parse(r#"["111","tok-1","/tmp/a"]"#),
        None,
        "a wrong arity refuses"
    );
    assert_eq!(
        parse(r#""12/34""#),
        None,
        "a JSON string is not a tab array"
    );
    assert!(take_token("tok-1", "tok-1"), "a taken ack matches its lift");
    assert!(!take_token("tok-1", "tok-2"), "and no other lift");
    assert!(
        !take_token("", "tok-1"),
        "and nothing matches an empty outstanding token"
    );
    assert_ne!(
        new_token(NOW, 1),
        new_token(NOW, 2),
        "two lifts in one instant still carry two tokens"
    );
}

#[test]
fn a_path_with_cr_or_lf_travels_verbatim_and_opens_through_the_receiver() {
    let mut cr = Side::new(Fake::at("/tmp/a\rb"));
    let text = cr.state.tab_payload(&cr.pane, 0, "222", "cr");
    let value = jsondoc::parse(&text).unwrap();
    assert_eq!(
        value.as_array().and_then(|fields| fields[2].as_str()),
        Some("/tmp/a\rb"),
        "CR path round trip is verbatim"
    );
    assert_eq!(
        parse(&text).map(|info| info.path),
        Some("/tmp/a\rb".into()),
        "CR absolute path accepted"
    );
    let mut receiver = Side::new(Fake::at("/tmp/receiver"));
    assert!(
        receiver
            .state
            .receive_tab(&mut receiver.pane, &text, Some(1), "111"),
        "CR folder opens through receiver"
    );
    assert_eq!(
        receiver.pane.path, "/tmp/a\rb",
        "receiver keeps exact CR folder path"
    );
    assert_eq!(
        parse(&foreign("lf", "/tmp/a\nb", "list", "")).map(|info| info.path),
        Some("/tmp/a\nb".into()),
        "LF absolute path accepted"
    );
}

#[test]
fn a_tab_from_another_window_opens_where_it_was_dropped_and_lands_on_its_file() {
    assert_eq!(
        drop_index_at(9999., 100., 3),
        3,
        "a drop past the last tab lands at the end"
    );
    assert_eq!(
        drop_index_at(-50., 100., 3),
        0,
        "a drop before the first lands at zero"
    );
    assert_eq!(
        drop_index_at(140., 100., 3),
        1,
        "a drop over the second tab names its near edge"
    );

    let tab = foreign("tok-9", "/tmp/folder", "grid", "note.txt");
    let mut receiver = Side::new(Fake::at("/tmp/r"));
    assert!(
        receiver
            .state
            .receive_tab(&mut receiver.pane, &tab, Some(1), "111"),
        "a foreign tab opens"
    );
    assert_eq!(
        (receiver.state.current(), receiver.count()),
        (1, 2),
        "at the drop position and shown"
    );
    assert_eq!(receiver.pane.path, "/tmp/folder", "on the folder it names");
    assert_eq!(receiver.pane.view, "grid", "in the view it names");
    assert_eq!(
        receiver.state.tabs.as_ref().unwrap().items[1].cursor_name,
        "note.txt",
        "carrying the cursor name"
    );
    receiver.pane.rows = Some(Box::new(|i| {
        row(
            &if i == 7 {
                "note.txt".to_string()
            } else {
                format!("other-{i}")
            },
            false,
        )
    }));
    receiver.state.apply_pending(&mut receiver.pane);
    assert_eq!(
        receiver.pane.cursor, 7,
        "cross-window receive restores the carried cursor file after rows"
    );

    let mut hidden_source =
        Side::new(Fake::at("/tmp/hidden-cursor").with_rows(|i| row(&format!("file-{i}"), false)));
    hidden_source
        .state
        .open_new(&mut hidden_source.pane, Some("/tmp/next-tab".into()));
    let lifted = parse(&hidden_source.state.tab_payload(
        &hidden_source.pane,
        0,
        "222",
        "hidden-cursor",
    ))
    .unwrap();
    assert_eq!(
        lifted.cursor, "file-4",
        "cross-window lift of a hidden tab carries its saved cursor file"
    );

    let mut absent = Side::new(Fake::at("/tmp/r").with_rows(|i| row(&format!("other-{i}"), false)));
    absent.state.receive_tab(
        &mut absent.pane,
        &foreign("missing", "/tmp/r", "list", "gone.txt"),
        None,
        "111",
    );
    absent.state.apply_pending(&mut absent.pane);
    assert_eq!(
        absent.pane.cursor, 0,
        "a same-folder receive never trusts the receiver cursor index"
    );

    let mut distant = Side::new(Fake::at("/tmp/r").with_rows(|i| {
        if i == 0 {
            row("first.txt", false)
        } else {
            None
        }
    }));
    distant.pane.held_count = Some(1);
    distant.pane.can_locate = true;
    distant.state.receive_tab(
        &mut distant.pane,
        &foreign("distant", "/tmp/distant", "list", "last.txt"),
        None,
        "111",
    );
    distant.state.apply_pending(&mut distant.pane);
    assert_eq!(
        distant.pane.located,
        ["/tmp/distant/last.txt"],
        "a moved cursor beyond the held rows asks locate by filename"
    );
    distant.state.located(
        &mut distant.pane,
        "/tmp/distant/last.txt",
        "/tmp/distant",
        Some(19),
    );
    assert_eq!(
        distant.pane.cursor, 19,
        "a located moved cursor uses the receiver's fresh index"
    );
    assert_eq!(
        distant.pane.windows,
        ["19:40"],
        "a located moved cursor asks for its own window"
    );
    distant.pane.cursor = 3;
    distant.state.located(
        &mut distant.pane,
        "/tmp/distant/last.txt",
        "/tmp/distant",
        Some(19),
    );
    assert_eq!(
        distant.pane.cursor, 3,
        "a spent locate reply cannot move the cursor twice"
    );

    absent.pane.can_locate = true;
    absent.state.prepare_cursor(&absent.pane, "gone.txt");
    absent.state.apply_pending(&mut absent.pane);
    absent.pane.cursor = 4;
    absent
        .state
        .located(&mut absent.pane, "/tmp/r/gone.txt", "/tmp/r", None);
    assert_eq!(
        absent.pane.cursor, 0,
        "a missing moved filename falls back to the first row"
    );
    assert_eq!(
        absent.pane.located,
        ["/tmp/r/gone.txt"],
        "a missing filename was located in the receiving folder"
    );
    absent.pane.busy = true;
    absent.state.prepare_cursor(&absent.pane, "gone.txt");
    absent.state.apply_pending(&mut absent.pane);
    absent.pane.cursor = 4;
    absent
        .state
        .located(&mut absent.pane, "/tmp/r/gone.txt", "/tmp/r", Some(2));
    assert_eq!(
        absent.pane.cursor, 4,
        "a locate answered while a listing is out spends itself and moves nothing"
    );
    absent.pane.busy = false;
    absent.state.prepare_cursor(&absent.pane, "gone.txt");
    absent.state.apply_pending(&mut absent.pane);
    absent
        .state
        .located(&mut absent.pane, "/tmp/r/gone.txt", "/elsewhere", Some(2));
    assert_eq!(
        absent.pane.cursor, 4,
        "and a locate answered for another folder is not this landing's"
    );

    let mut plain = Side::new(Fake::at("/tmp/r"));
    plain.state.receive_tab(
        &mut plain.pane,
        &foreign("tok-9", "/tmp/g", "britelite", ""),
        None,
        "111",
    );
    assert_eq!(
        plain.pane.view, "list",
        "an unknown view keeps the standing one"
    );
    assert_eq!(
        plain.state.current(),
        1,
        "and off the strip lands at the end"
    );
    let mut tabbed = tabbed();
    let own_payload = tab_payload(&mut tabbed, 0, "111", "tok-1");
    let mut own = Side::new(Fake::at("/tmp/r"));
    own.state
        .receive_tab(&mut own.pane, &own_payload, Some(0), "111");
    assert_eq!(own.count(), 1, "this window's own drag never receives");
    let mut busy = Side::new(Fake::at("/tmp/r"));
    busy.pane.busy = true;
    let took = busy.state.receive_tab(&mut busy.pane, &tab, Some(0), "111");
    assert_eq!(
        (took, busy.pane.said.join("")),
        (true, String::new()),
        "a loading window takes the drop and says nothing"
    );
    assert_eq!(
        (busy.pane.listed.last().cloned(), busy.pane.superseded),
        (Some("/tmp/folder".into()), true),
        "and the tab's listing takes the place of the one out"
    );
    let mut full = Side::new(Fake::at("/tmp/r"));
    let nine: Vec<String> = (0..9).map(|f| format!("/tmp/{f}")).collect();
    full.state.tabs = Some(pack(nine.iter().map(Tab::at).collect(), 0, None));
    assert!(
        !full.state.receive_tab(&mut full.pane, &tab, Some(0), "111"),
        "a full strip refuses the drop"
    );
    assert_eq!(
        full.pane.last_said(),
        "Nine tabs is the most.",
        "with the cap's own sentence"
    );
    assert!(!full.state.can_receive(), "a full strip does not receive");
}

#[test]
fn the_taken_ack_closes_the_lifted_tab_and_a_lone_tab_never_lifts() {
    let mut moved = Side::new(Fake::at("/tmp/one"));
    moved.state.tabs = Some(pack(
        vec![Tab::at("/tmp/one"), Tab::at("/tmp/two")],
        0,
        None,
    ));
    assert!(
        moved.state.close_after_move(&mut moved.pane, 1),
        "a moved tab closes"
    );
    assert_eq!(moved.count(), 1, "and is gone");
    let mut only = Side::new(Fake::at("/tmp/only"));
    assert!(
        !only.state.close_after_move(&mut only.pane, 0),
        "a lone tab never lifts, so it is kept rather than closing the window"
    );
    assert_eq!(only.count(), 1, "and stands");
    assert!(!only.state.can_lift(), "a lone tab is not lifted");
    let mut pair = Side::new(Fake::at("/tmp/one"));
    pair.state.tabs = Some(pack(
        vec![Tab::at("/tmp/one"), Tab::at("/tmp/two")],
        0,
        None,
    ));
    assert!(pair.state.can_lift(), "while a pair lifts");
    let mut kept = Side::pair("/tmp/one");
    kept.pane.busy = true;
    assert_eq!(
        (kept.state.close_after_move(&mut kept.pane, 0), kept.count()),
        (true, 1),
        "a loading source closes the tab that moved"
    );
    assert!(
        !pair.state.close_after_move(&mut pair.pane, 5),
        "no such tab keeps everything"
    );
    assert_eq!(pair.count(), 2, "no such tab keeps both tabs");
}

// A lift out of pane 0 whose platform drag ended without a local drop, as Qt's IgnoreAction finish leaves it.
fn lifted(world: &mut World, bar: &mut Bar, index: usize) -> String {
    bar.tab_lift_began(world, index, NOW);
    let token = bar.out_token.clone();
    bar.out_finished(NOW);
    token
}

#[test]
fn an_ack_closes_the_lifted_identity_wherever_the_tab_moved_and_never_a_stand_in() {
    let mut shifted = World::one(Side::pair("/tmp/duplicate"));
    {
        let side = shifted.side(0);
        side.state.open_new(&mut side.pane, None);
    }
    let ids = shifted.side(0).identities();
    let mut bar = bar();
    let token = lifted(&mut shifted, &mut bar, 1);
    {
        let side = shifted.side(0);
        side.state.close_at(&mut side.pane, 0);
    }
    bar.take(&mut shifted, &token, NOW);
    assert_eq!(
        shifted.side(0).identities(),
        [ids[2]],
        "index shift ack closes lifted identity"
    );

    let mut nav = World::one(Side::pair("/tmp/duplicate"));
    let ids = nav.side(0).identities();
    let mut bar = self::bar();
    let token = lifted(&mut nav, &mut bar, 1);
    nav.side(0).pane.path = "/tmp/navigated".into();
    bar.take(&mut nav, &token, NOW);
    assert_eq!(
        nav.side(0).identities(),
        [ids[0]],
        "navigation ack closes lifted identity"
    );
    assert_eq!(
        nav.side(0).count(),
        1,
        "navigation ack closes exactly one tab"
    );

    let mut gone = World::one(Side::pair("/tmp/duplicate"));
    {
        let side = gone.side(0);
        side.state.open_new(&mut side.pane, None);
    }
    let mut bar = self::bar();
    let token = lifted(&mut gone, &mut bar, 1);
    {
        let side = gone.side(0);
        side.state.close_at(&mut side.pane, 1);
    }
    assert!(
        !bar.take(&mut gone, &token, NOW),
        "an ack for a closed tab is refused"
    );
    assert_eq!(
        gone.side(0).count(),
        2,
        "closed lifted tab never substitutes duplicate"
    );

    let mut multi = World::one(Side::pair("/tmp/duplicate"));
    {
        let side = multi.side(0);
        side.state.open_new(&mut side.pane, None);
    }
    let mut bar = self::bar();
    let first = lifted(&mut multi, &mut bar, 0);
    let second = lifted(&mut multi, &mut bar, 1);
    assert!(
        bar.take(&mut multi, &first, NOW),
        "first ack survives second lift"
    );
    assert!(
        bar.take(&mut multi, &second, NOW),
        "second ack survives first closure"
    );
    assert_eq!(
        multi.side(0).count(),
        1,
        "both outstanding lifts close once"
    );

    let mut panes = World {
        sides: vec![
            Some(Side::pair("/tmp/duplicate")),
            Some(Side::pair("/tmp/duplicate")),
        ],
    };
    let mut bar = self::bar();
    let token = lifted(&mut panes, &mut bar, 1);
    bar.set_pane(&mut panes, Some(1));
    bar.take(&mut panes, &token, NOW);
    assert_eq!(
        panes.side(0).count(),
        1,
        "focus switch closes captured primary"
    );
    assert_eq!(panes.side(1).count(), 2, "focus switch keeps secondary");
}

#[test]
fn an_acknowledged_lift_waits_for_its_pane_to_settle_and_survives_the_deadline_while_it_waits() {
    let mut waiting = World::one(Side::pair("/tmp/duplicate"));
    let mut bar = bar();
    let token = lifted(&mut waiting, &mut bar, 1);
    waiting.side(0).pane.path = "/tmp/loading-next".into();
    waiting.side(0).pane.busy = true;
    assert!(
        bar.take(&mut waiting, &token, NOW),
        "navigation in flight accepts captured ack"
    );
    assert_eq!(
        waiting.side(0).count(),
        2,
        "navigation in flight holds source until settled"
    );
    waiting.side(0).pane.busy = false;
    bar.drain_lifts(&mut waiting, NOW + 100);
    assert_eq!(
        waiting.side(0).count(),
        1,
        "navigation completion closes acknowledged identity"
    );

    let mut delayed = World::one(Side::pair("/tmp/duplicate"));
    let mut bar = self::bar();
    let token = lifted(&mut delayed, &mut bar, 1);
    bar.lifts[0].lifted_at = NOW - 1000;
    delayed.side(0).pane.busy = true;
    assert!(
        bar.take(&mut delayed, &token, NOW),
        "backdated lift accepts ack at 1000 ms"
    );
    bar.lifts[0].lifted_at = NOW - ACK_WAIT_MS - 1;
    bar.drain_lifts(&mut delayed, NOW);
    assert_eq!(
        bar.lifts.len(),
        1,
        "taken lift survives past 15001 ms while loading"
    );
    delayed.side(0).pane.busy = false;
    bar.drain_lifts(&mut delayed, NOW);
    assert_eq!(
        delayed.side(0).count(),
        1,
        "settlement past 15001 ms closes acknowledged source"
    );
    assert!(
        bar.lifts.is_empty(),
        "settlement consumes acknowledged lift"
    );

    let mut expired = World::one(Side::pair("/tmp/duplicate"));
    let mut bar = self::bar();
    let token = lifted(&mut expired, &mut bar, 1);
    assert!(
        !bar.take(&mut expired, &token, NOW + ACK_WAIT_MS + 1),
        "an ack after the wait closes nothing"
    );
    assert_eq!(
        expired.side(0).count(),
        2,
        "so a late receiver can never take the source away"
    );
    bar.drain_lifts(&mut expired, NOW + ACK_WAIT_MS + 1);
    assert!(
        bar.lifts.is_empty() && bar.out_token.is_empty(),
        "and the expired lift is dropped, its tab still standing"
    );
    assert_eq!(expired.side(0).count(), 2, "with both tabs kept");

    let mut refused = World::one(Side::pair("/tmp/duplicate"));
    let mut bar = self::bar();
    lifted(&mut refused, &mut bar, 1);
    assert!(
        !bar.take(&mut refused, "someone-else", NOW),
        "an ack naming no lift of this window is refused"
    );
    assert_eq!(refused.side(0).count(), 2, "and closes nothing");
}

#[test]
fn a_local_reorder_and_an_ack_never_close_the_wrong_tab() {
    let mut held = World::one(Side::pair("/tmp/duplicate"));
    {
        let side = held.side(0);
        side.state.open_new(&mut side.pane, None);
    }
    let ids = held.side(0).identities();
    let mut bar = bar();
    let token = lifted(&mut held, &mut bar, 0);
    bar.drag_started(Some(1));
    bar.drop_at = Some(0);
    assert!(
        bar.take(&mut held, &token, NOW),
        "ack during held B reorder is accepted"
    );
    assert_eq!(
        held.side(0).count(),
        3,
        "ack leaves A B C indices intact during reorder"
    );
    assert_eq!(
        held.side(0).identities()[1],
        ids[1],
        "held B remains at its captured index"
    );
    bar.drag_finished(&mut held, NOW);
    assert_eq!(
        held.side(0).identities(),
        [ids[1], ids[2]],
        "release lands B at front then closes A"
    );
    assert!(bar.lifts.is_empty(), "release drains deferred ack");

    let mut local = World::one(Side::pair("/tmp/duplicate"));
    {
        let side = local.side(0);
        side.state.open_new(&mut side.pane, None);
    }
    let mut bar = self::bar();
    let token = lifted(&mut local, &mut bar, 0);
    bar.tab_lift_began(&mut local, 2, NOW);
    bar.drop_at = Some(0);
    bar.tab_lift_ended(&mut local, NOW);
    assert!(
        bar.take(&mut local, &token, NOW),
        "later local reorder preserves earlier ack"
    );
    assert_eq!(
        local.side(0).count(),
        2,
        "reordered lifted identity closes exactly once"
    );
    assert!(
        !bar.take(&mut local, &token, NOW),
        "repeat ack cannot close another tab"
    );

    let mut long = World::one(Side::pair("/tmp/duplicate"));
    let mut bar = self::bar();
    bar.tab_lift_began(&mut long, 1, NOW);
    let active = bar.lifts[0].clone();
    assert!(
        ack_closes(
            &active.token,
            &active.token,
            active.lifted_at,
            NOW + ACK_WAIT_MS + 1
        ),
        "active drag does not spend post-finish ack window"
    );
    bar.tab_lift_ended(&mut long, NOW);
    assert!(
        bar.lifts.is_empty(),
        "local release consumes only its own lift"
    );

    let mut finished = World::one(Side::pair("/tmp/duplicate"));
    let mut bar = self::bar();
    let token = lifted(&mut finished, &mut bar, 1);
    bar.tab_lift_ended(&mut finished, NOW);
    assert!(
        ack_closes(&bar.out_token, &token, bar.ack_lifted_at, NOW),
        "Ignore finish preserves stored ack token"
    );
    bar.take(&mut finished, &token, NOW);
    assert_eq!(
        finished.side(0).count(),
        1,
        "Ignore finish ack closes moved tab"
    );

    let mut escape = World::one(Side::pair("/tmp/duplicate"));
    let mut bar = self::bar();
    bar.tab_lift_began(&mut escape, 1, NOW);
    bar.out_active = true;
    let token = bar.out_token.clone();
    bar.cancel_out(NOW);
    assert!(!bar.out_active, "Escape completion stops active drag");
    assert_eq!(
        escape.side(0).count(),
        2,
        "Escape completion keeps both tabs"
    );
    assert_eq!(
        bar.out_token, token,
        "Escape holds stored token for delayed receiver"
    );
    let mut consumed_world = World::one(Side::pair("/tmp/duplicate"));
    let mut consumed = self::bar();
    consumed.tab_lift_began(&mut consumed_world, 1, NOW);
    consumed.own_accepted = true;
    consumed.out_finished(NOW);
    assert_eq!(
        consumed.out_token, "",
        "consumed own drop clears stored token"
    );
    assert!(
        !ack_closes(
            &bar.out_token,
            &token,
            bar.ack_lifted_at,
            bar.ack_lifted_at + ACK_WAIT_MS + 1
        ),
        "expired stored token cannot close"
    );

    let mut lone = World::one(Side::new(Fake::at("/tmp/lone")));
    let mut bar = self::bar();
    bar.tab_lift_began(&mut lone, 0, NOW);
    assert!(
        bar.out_token.is_empty() && bar.drag_from.is_none() && bar.lifts.is_empty(),
        "a lone tab never lifts, so its window can never lose it"
    );
}

#[test]
fn a_receiver_validates_one_drop_at_a_time_and_acknowledges_only_what_it_opened() {
    let mut world = World::one(Side::new(Fake::at("/tmp/receiver")));
    let mut bar = bar();
    let a = foreign("A", "/tmp/A", "list", "");
    let b = payload("333", "B", "/tmp/B", "list", "");
    bar.accept_tab_drop(&mut world, &a, &parse(&a).unwrap(), Some(1), NOW);
    bar.accept_tab_drop(&mut world, &b, &parse(&b).unwrap(), Some(1), NOW);
    assert_eq!(
        bar.pending.as_ref().map(|pending| pending.token.as_str()),
        Some("A"),
        "second pending drop cannot overwrite first"
    );
    bar.on_peeked(&mut world, 0, "/tmp/A", false, false, false, PEEK_FIRST);
    assert_eq!(
        world.side(0).pane.path,
        "/tmp/A",
        "first validated drop opens its folder"
    );
    assert_eq!(acks(&mut bar), ["A"], "only opened drop is acknowledged");
    let peeks: Vec<&str> = world
        .side(0)
        .pane
        .peeks
        .iter()
        .map(|peek| peek.0.as_str())
        .collect();
    assert_eq!(
        peeks,
        ["/tmp/A"],
        "overlapping drop refused before validation"
    );
}

#[test]
fn a_strip_drop_takes_a_foreign_tab_with_move_and_returns_an_own_lift() {
    let mut world = World::one(Side::new(Fake::at("/tmp/receiver")));
    let mut bar = bar();
    let tab = foreign("tok-9", "/tmp/folder", "grid", "");
    assert!(
        bar.strip_drop(&mut world, &tab, 9999., NOW),
        "a foreign tab dropped on the strip is taken at once"
    );
    assert_eq!(
        world.side(0).pane.peeks,
        [("/tmp/folder".to_string(), PEEK_FIRST, false, false)],
        "and only its folder check goes out"
    );
    assert_eq!(
        bar.deadline,
        Some(NOW + ACK_WAIT_MS),
        "under the acknowledgment's own bound"
    );
    assert_eq!(
        world.side(0).count(),
        1,
        "and no tab opens before the check answers"
    );
    assert!(
        !bar.strip_drop(&mut world, "garbage", 0., NOW),
        "a payload that does not parse is not taken"
    );
    let mut own = World::one(Side::pair("/tmp/own"));
    let mut source = self::bar();
    source.tab_lift_began(&mut own, 1, NOW);
    let own_payload = source.out_payload.clone();
    assert!(
        !source.strip_drop(&mut own, &own_payload, 0., NOW),
        "an own tab without a platform drag out is the strip's reorder, not a drop"
    );
    source.out_active = true;
    assert!(
        source.strip_drop(&mut own, &own_payload, 0., NOW),
        "an own lift out and back is taken"
    );
    assert!(
        source.own_accepted,
        "and answers as consumed, so its finish closes nothing"
    );
    assert_eq!(
        own.side(0).state.current(),
        0,
        "and the lifted tab lands where it was dropped"
    );
    source.out_finished(NOW);
    assert!(
        source.out_token.is_empty() && source.lifts.is_empty(),
        "and its lift is spent"
    );
    assert_eq!(own.side(0).count(), 2, "with both tabs kept");
}

#[test]
fn a_launch_acknowledges_its_lift_once_when_its_folder_opens() {
    let env: BTreeMap<&str, &str> = [
        ("FLEA_TAB_SOURCE_PID", "111"),
        ("FLEA_TAB_TOKEN", "tok"),
        ("FLEA_TAB_CURSOR", "a.txt"),
    ]
    .into();
    let mut launch = Launch::read(
        |name| env.get(name).map(|value| value.to_string()),
        "/tmp/tear",
    );
    assert_eq!(
        launch.cursor, "a.txt",
        "the launch carries its cursor filename"
    );
    assert_eq!(
        launch.opened("/tmp/other"),
        None,
        "another folder opening is not the launch's"
    );
    assert_eq!(
        launch.opened("/tmp/tear"),
        Some(("111".into(), "tok".into())),
        "the launch folder opening acknowledges the lift"
    );
    assert_eq!(launch.opened("/tmp/tear"), None, "and only once");
    let mut plain = Launch::read(|_| None, "/tmp/tear");
    assert_eq!(
        plain.opened("/tmp/tear"),
        None,
        "a plain launch acknowledges nothing"
    );
    let forged_env: BTreeMap<&str, &str> =
        [("FLEA_TAB_SOURCE_PID", "pid"), ("FLEA_TAB_TOKEN", "tok")].into();
    let mut forged = Launch::read(
        |name| forged_env.get(name).map(|value| value.to_string()),
        "/tmp/tear",
    );
    assert_eq!(
        forged.opened("/tmp/tear"),
        None,
        "a pid that is not a number is never called"
    );
}

#[test]
fn a_lift_may_not_leave_while_a_rename_or_a_listing_holds_its_tab() {
    assert_eq!(
        tear_refusal(&Fake::standard()),
        None,
        "a clean pane tears out"
    );
    let mut renaming = Fake::standard();
    renaming.rename_pending = true;
    assert_eq!(
        tear_refusal(&renaming).map(|say| say.text(&english())),
        Some("Finish the rename before dragging a tab out.".into()),
        "a pending rename refuses the drag"
    );
    let mut loading = Fake::standard();
    loading.busy = true;
    assert_eq!(
        tear_refusal(&loading).map(|say| say.text(&english())),
        Some("A directory is already loading.".into()),
        "a loading listing refuses"
    );
    assert!(
        TabState::default().can_receive(),
        "a strip with room receives"
    );

    let mut world = World::one(Side::pair("/tmp/duplicate"));
    world.side(0).pane.rename_pending = true;
    let mut bar = bar();
    bar.tab_lift_began(&mut world, 1, NOW);
    assert!(
        !bar.tab_lift_moved(&mut world, 600., true),
        "a refused lift starts no platform drag"
    );
    assert!(
        !bar.tab_lift_moved(&mut world, 650., true),
        "nor on the next move"
    );
    assert_eq!(
        world.side(0).pane.said,
        ["Finish the rename before dragging a tab out."],
        "and says why once, not per move"
    );
    world.side(0).pane.rename_pending = false;
    let mut clean = World::one(Side::pair("/tmp/duplicate"));
    let mut out = self::bar();
    out.tab_lift_began(&mut clean, 1, NOW);
    assert!(
        !out.tab_lift_moved(&mut clean, 100., false),
        "a lift inside the window stays the strip's own reorder"
    );
    assert!(
        out.tab_lift_moved(&mut clean, 600., true),
        "and leaves the window as a platform drag"
    );
    assert!(out.out_active, "which owns its release from then on");
}

#[test]
fn a_foreign_enter_and_drop_decide_from_formats_and_the_lift_state() {
    let foreign_payload = foreign("tok-9", "/tmp/folder", "grid", "");
    let mut tabbed = tabbed();
    let own = tab_payload(&mut tabbed, 0, "111", "tok-1");
    let tab = [TAB_MIME.to_string()];
    let uri = ["text/uri-list".to_string()];
    assert!(
        enter_accepts(&tab, "", "222", true, false),
        "a foreign enter with formats and no payload accepts when receivable"
    );
    assert!(
        !enter_accepts(&uri, "", "222", true, false),
        "it refuses without the tab format"
    );
    assert!(
        !enter_accepts(&tab, "", "222", false, false),
        "it refuses when the strip cannot receive"
    );
    assert!(
        enter_accepts(&tab, &foreign_payload, "111", true, false),
        "a payload enter accepts a foreign tab"
    );
    assert!(
        !enter_accepts(&tab, &own, "111", true, false),
        "it refuses its own tab without an active lift"
    );
    assert!(
        enter_accepts(&tab, &own, "111", true, true),
        "it keeps an own lift out and back"
    );
    assert!(
        !enter_accepts(&tab, "not json", "111", true, false),
        "garbage payload refuses even with the format"
    );

    let take = parse(&foreign_payload).unwrap();
    assert_eq!(
        drop_decision(Some(&take), "111", false, true),
        Decision::Take,
        "a receivable foreign drop takes with Move"
    );
    assert_eq!(
        drop_decision(Some(&take), "111", false, false),
        Decision::Ignore,
        "a foreign drop on a full strip is ignored"
    );
    let lift = parse(&own).unwrap();
    assert_eq!(
        drop_decision(Some(&lift), "111", false, true),
        Decision::Ignore,
        "an own drag without a lift is ignored"
    );
    assert_eq!(
        drop_decision(Some(&lift), "111", true, true),
        Decision::Take,
        "an own lift out and back takes with Move"
    );
    assert_eq!(
        drop_decision(None, "111", true, true),
        Decision::Ignore,
        "no payload takes nothing"
    );
}

#[test]
fn the_catcher_decides_tear_off_return_or_cancel_from_the_source_window() {
    let rect = Some(Rect {
        x: 100.,
        y: 200.,
        width: 600.,
        height: 400.,
    });
    let band = Some(Rect {
        x: 16.,
        y: 40.,
        width: 584.,
        height: 32.,
    });
    assert_eq!(
        catcher_outcome(rect, band, 140., 250., 100., 3),
        Outcome::Return(0),
        "catcher own-strip drop returns at shared insertion index"
    );
    assert_eq!(
        catcher_outcome(rect, band, 266., 250., 100., 3),
        Outcome::Return(2),
        "catcher own-strip midpoint uses the strip insertion math"
    );
    assert_eq!(
        catcher_outcome(rect, band, 140., 350., 100., 3),
        Outcome::Cancel,
        "catcher own-window drop off strip cancels"
    );
    assert_eq!(
        catcher_outcome(rect, band, 701., 250., 100., 3),
        Outcome::TearOff,
        "catcher outside own window tears off"
    );
    assert_eq!(
        catcher_outcome(None, band, 900., 900., 100., 3),
        Outcome::Cancel,
        "unknown source rectangle cancels even on desktop"
    );
    assert_eq!(
        catcher_outcome(rect, band, 105., 250., 100., 3),
        Outcome::Cancel,
        "left strip margin inside window cancels"
    );
    assert_eq!(
        catcher_outcome(rect, band, 140., 272., 100., 3),
        Outcome::Cancel,
        "strip lower edge belongs to window cancel band"
    );
    assert_eq!(
        catcher_outcome(rect, band, 700., 250., 100., 3),
        Outcome::TearOff,
        "window right edge is outside"
    );

    let mut world = World::one(Side::pair("/tmp/duplicate"));
    let mut bar = bar();
    bar.tab_lift_began(&mut world, 1, NOW);
    assert_eq!(
        bar.catcher_drop(&mut world, rect, band, 900., 900., NOW),
        None,
        "a catcher drop before the platform drag is nothing"
    );
    bar.out_active = true;
    let tear = bar.catcher_drop(&mut world, rect, band, 900., 300., NOW);
    assert_eq!(
        tear.map(|tear| tear.path),
        Some("/tmp/duplicate".into()),
        "a desktop drop starts the tear-off of the lifted folder"
    );
    assert_eq!(
        world.side(0).count(),
        2,
        "and keeps the source tab until the new window acknowledges"
    );
}

#[test]
fn the_compositor_answer_names_the_one_mapped_window_of_this_pid_and_the_latest_lift_wins() {
    let clients = r#"[{"pid":111,"at":[10,20],"size":[900,500],"mapped":true,"hidden":false},{"pid":222,"at":[0,0],"size":[5,5],"mapped":true,"hidden":false},{"pid":111,"at":[0,0],"size":[9,9],"mapped":false,"hidden":false},{"pid":111,"at":[0,0],"size":[9,9],"mapped":true,"hidden":true}]"#;
    assert_eq!(
        source_rect(clients, 111),
        Some(Rect {
            x: 10.,
            y: 20.,
            width: 900.,
            height: 500.
        }),
        "the one mapped visible window of this pid is the source"
    );
    let two = r#"[{"pid":111,"at":[0,0],"size":[9,9]},{"pid":111,"at":[5,5],"size":[9,9]}]"#;
    assert_eq!(
        source_rect(two, 111),
        None,
        "two windows of one pid name no source, so a drop cancels"
    );
    assert_eq!(
        source_rect("not json", 111),
        None,
        "an unreadable answer names none"
    );
    assert_eq!(
        source_rect(r#"[{"pid":111,"at":[0,0],"size":[0,9]}]"#, 111),
        None,
        "nor does an empty window"
    );

    let strip = Rect {
        x: 14.,
        y: 27.,
        width: 486.,
        height: 27.,
    };
    let mut geometry = Geometry::default();
    assert!(
        geometry.begin("first", strip),
        "the first lift starts its query"
    );
    assert!(
        !geometry.begin("latest", strip),
        "a later lift waits for the query out"
    );
    assert!(
        geometry.finished(clients, 111),
        "the old answer restarts the query for the latest lift"
    );
    assert_eq!(
        geometry.rect, None,
        "and is never taken as the latest lift's window"
    );
    assert!(
        !geometry.finished(clients, 111),
        "the latest answer ends the queries"
    );
    assert_eq!(
        (geometry.token.as_str(), geometry.rect.is_some()),
        ("latest", true),
        "and the latest lift holds its own window"
    );
}

#[path = "transfer_peek.rs"]
mod peek;
