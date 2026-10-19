use super::*;
use flea_gui::tabs::state::{label, pack, restore_selection, Tab};

fn tabs_of(paths: &[&str], index: usize) -> Option<flea_gui::tabs::Tabs> {
    Some(pack(
        paths.iter().map(|path| Tab::at(*path)).collect(),
        index,
        None,
    ))
}

#[test]
fn a_tab_opened_from_search_results_records_where_the_user_was_and_drops_the_search() {
    let mut searching = Side::new(Fake::at("/"));
    searching.pane.search_mode = "results";
    searching.pane.search_from = "/home/gm/Work".into();
    searching.state.open_new(&mut searching.pane, None);
    let items = &searching.state.tabs.as_ref().unwrap().items;
    assert_eq!(
        items[1].path, "/home/gm/Work",
        "a tab opened from search results remembers where the user was, not the scope"
    );
    assert_eq!(
        items[0].path, "/home/gm/Work",
        "and so does the tab it was opened from"
    );
    assert_eq!(
        searching.pane.search_mode, "",
        "and the search is dropped once the paths are taken"
    );
    assert_eq!(
        searching.pane.path, "/home/gm/Work",
        "and the pane lands on the path the tab records"
    );
    assert_eq!(
        items[1].cursor, 0,
        "carrying no cursor from the search's own listing"
    );
    assert!(
        items[1].selected.is_empty(),
        "and no selection from it either"
    );

    let mut plain = Side::new(Fake::at("/tmp/here"));
    plain.state.open_new(&mut plain.pane, None);
    assert_eq!(
        plain.paths()[1],
        "/tmp/here",
        "an ordinary listing opens its tab on its own path"
    );
}

#[test]
fn a_restored_selection_keeps_only_rows_that_exist_and_a_relisting_switch_restores_none() {
    let mut shrunk = Fake::standard();
    shrunk.total = 3;
    restore_selection(&mut shrunk, &[0, 2, 7, 40], false);
    assert_eq!(
        shrunk.selection,
        [0, 2],
        "a selection restored into a shrunken directory keeps only rows that still exist"
    );
    let mut none = Fake::standard();
    none.total = 0;
    none.announced = 5;
    restore_selection(&mut none, &[1, 2], false);
    assert_eq!(
        (none.selection.len(), none.announced),
        (0, 5),
        "and a restore that kept nothing does not announce a selection change"
    );

    let mut moved = Side::new(Fake::at("/tmp/a"));
    let mut b = Tab::at("/tmp/b");
    b.cursor = 1;
    b.selected = vec![0, 1, 2];
    moved.state.tabs = Some(pack(vec![Tab::at("/tmp/a"), b], 0, None));
    moved.state.select_at(&mut moved.pane, 1);
    assert!(
        moved.pane.selection.is_empty(),
        "a switch that re-lists asks for no selection to be restored"
    );
    assert_eq!(
        moved.pane.listed,
        ["/tmp/b"],
        "and it did re-list, which is what clears the selection"
    );
}

#[test]
fn a_refused_tenth_tab_and_a_background_close_cost_the_user_nothing_else() {
    let mut full = Side::new(Fake::at("/tmp/full"));
    let nine: Vec<String> = (0..9).map(|t| format!("/tmp/{t}")).collect();
    full.state.tabs = Some(pack(nine.iter().map(Tab::at).collect(), 0, None));
    full.pane.preview_active = true;
    full.pane.search_mode = "results";
    full.pane.search_running = true;
    full.state.open_new(&mut full.pane, None);
    assert_eq!(
        full.pane.last_said(),
        "Nine tabs is the most.",
        "a refused tenth tab says so"
    );
    assert!(full.pane.preview_active, "and leaves the preview open");
    assert!(
        full.pane.search_running,
        "and does not cancel the running search"
    );
    assert_eq!(
        full.pane.search_mode, "results",
        "and the search itself is still standing"
    );

    let mut many = Side::new(Fake::at("/tmp/one"));
    many.state.tabs = tabs_of(&["/tmp/one", "/tmp/two", "/tmp/three"], 0);
    many.pane.preview_active = true;
    many.state.close_at(&mut many.pane, 2);
    assert!(
        many.pane.preview_active,
        "closing a background tab leaves the current tab's preview alone"
    );
    assert_eq!(many.count(), 2, "and still removes it");
}

#[test]
fn a_tab_opened_while_typing_drops_the_live_session_and_keeps_no_walk_cursor() {
    let mut typing = Side::new(Fake::at("/"));
    typing.pane.search_mode = "typing";
    typing.pane.search_query = "no".into();
    typing.pane.search_from = "/home/gm/Work".into();
    typing.pane.search_gen = 1;
    assert!(
        typing.pane.drop_overlay(),
        "dropOverlay while typing drops the live session"
    );
    assert_eq!(
        (typing.pane.search_mode, typing.pane.search_query.as_str()),
        ("typing", "no"),
        "and the drawn line waits for the landing"
    );
    let snap = typing
        .state
        .snapshot(&typing.pane, Some("/home/gm/Work".into()), 0, false);
    assert_eq!(
        snap.cursor, 0,
        "and a typing snapshot keeps no cursor or selection"
    );

    let mut emptied = Side::new(Fake::at("/tmp/same"));
    emptied.pane.search_mode = "typing";
    emptied.pane.search_gen = 2;
    emptied.pane.cursor = 3;
    let mut other = Tab::at("/tmp/same");
    other.cursor = 1;
    emptied.state.tabs = Some(pack(vec![Tab::at("/tmp/same"), other], 0, None));
    emptied.state.select_at(&mut emptied.pane, 1);
    assert_eq!(
        (emptied.pane.listed.join(","), emptied.pane.search_mode),
        ("/tmp/same".into(), ""),
        "a switch to a tab on the same folder after the query is emptied re-lists that folder"
    );
    assert_eq!(
        emptied.state.tabs.as_ref().unwrap().items[0].cursor,
        0,
        "and the tab it left keeps no cursor read off the walk's rows"
    );

    let mut leaving = Side::new(Fake::at("/"));
    leaving.pane.search_mode = "results";
    leaving.pane.search_from = "/home/gm/Work".into();
    leaving.state.tabs = Some(pack(vec![Tab::at("/tmp/other"), Tab::at("/")], 1, None));
    leaving.state.select_at(&mut leaving.pane, 0);
    assert_eq!(
        leaving.paths()[1],
        "/home/gm/Work",
        "the tab left behind during a search records where the user was"
    );
}

#[test]
fn labels_draw_the_live_path_for_the_current_tab_and_name_root_home_and_leaves() {
    let mut state = TabState::default();
    state.tabs = Some(pack(
        vec![Tab::at("/home/gm"), Tab::at("/tmp/one")],
        1,
        None,
    ));
    assert_eq!(
        state.path_at(1, "/tmp/moved"),
        "/tmp/moved",
        "the current tab draws the pane's live path, not its snapshot"
    );
    assert_eq!(
        state.path_at(0, "/tmp/moved"),
        "/home/gm",
        "a hidden tab draws its own snapshot"
    );
    assert_eq!(
        TabState::default().path_at(3, "/home/gm"),
        "",
        "no tabs at all still answers a string"
    );
    assert_eq!(
        label("/", "", "Home"),
        "/",
        "a root tab is labelled with its separator"
    );
    assert_eq!(
        label("/home/gm", "/home/gm", "Home"),
        "Home",
        "the home directory uses the rail's own Home label"
    );
    assert_eq!(
        label("/home/gm/Work", "/home/gm", "Home"),
        "Work",
        "a home child is labelled with its leaf, not the tilde form"
    );
    assert_eq!(
        TabState::default().count(),
        1,
        "one pane with no tab state still counts as one tab"
    );
}

#[test]
fn t_seeds_the_folder_digits_switch_and_the_pending_cursor_lands_after_rows() {
    let mut born = Side::new(Fake::standard());
    born.act("tabNew");
    assert_eq!(
        born.count(),
        2,
        "t seeds the current folder and opens a second tab on it"
    );
    assert_eq!(born.state.current(), 1, "and lands on the new tab");
    assert!(
        born.pane.listed.is_empty(),
        "and does not re-list, because both tabs name the same directory"
    );
    born.pane.path = "/home/gm/Downloads".into();
    let labels = born.state.labels(&born.pane.path, "/home/gm", "Home");
    assert_eq!(
        labels[1], "Downloads",
        "a navigate updates the current tab's label without a switch"
    );
    assert_eq!(
        labels[0], "Work",
        "and leaves the other tab's snapshot alone"
    );
    born.act("tab1");
    assert_eq!(born.state.current(), 0, "1 switches to the first tab");
    assert_eq!(
        born.pane.listed,
        ["/home/gm/Work"],
        "and lists the snapshot path, because it is a different directory"
    );
    assert_eq!(
        born.state.tabs.as_ref().unwrap().pending.cursor,
        Some(4),
        "and keeps the cursor to restore after rows arrive"
    );
    born.state.apply_pending(&mut born.pane);
    assert_eq!(
        born.pane.cursor, 4,
        "the pending cursor lands once rows arrive"
    );
}

#[test]
fn next_and_previous_wrap_and_restore_each_tab_and_never_refuse_while_loading() {
    let mut cycling = Side::new(Fake::at("/tmp/first"));
    cycling.state.open_new(&mut cycling.pane, None);
    cycling.pane.path = "/tmp/second".into();
    cycling.pane.cursor = 7;
    cycling.pane.view = "grid";
    cycling.pane.hidden = true;
    cycling.pane.order = order("size", true);
    cycling.act("tabNext");
    assert_eq!(
        cycling.state.current(),
        0,
        "next wraps from the last tab to the first"
    );
    assert_eq!(
        cycling.pane.path, "/tmp/first",
        "next opens the first tab's directory"
    );
    let left = &cycling.state.tabs.as_ref().unwrap().items[1];
    assert_eq!(
        (left.cursor, left.view, left.hidden),
        (7, "grid", true),
        "the tab left behind retains its live cursor, view and hidden preference"
    );
    cycling.state.apply_pending(&mut cycling.pane);
    cycling.state.apply_pending(&mut cycling.pane);
    assert_eq!(
        (cycling.pane.cursor, cycling.pane.order),
        (4, order("name", false)),
        "next restores the target cursor and sorting after its listing arrives"
    );
    cycling.act("tabPrevious");
    assert_eq!(
        cycling.state.current(),
        1,
        "previous wraps from the first tab to the last"
    );
    assert_eq!(
        (
            cycling.pane.path.as_str(),
            cycling.pane.view,
            cycling.pane.hidden
        ),
        ("/tmp/second", "grid", true),
        "previous restores the second tab's directory, view and hidden preference"
    );
    cycling.state.apply_pending(&mut cycling.pane);
    cycling.state.apply_pending(&mut cycling.pane);
    assert_eq!(
        (cycling.pane.cursor, cycling.pane.order),
        (7, order("size", true)),
        "previous restores the second tab's cursor and sorting"
    );
    cycling.act("tabPrevious");
    assert_eq!(
        cycling.state.current(),
        0,
        "previous steps backward before wrapping"
    );
    cycling.act("tabNext");
    assert_eq!(
        cycling.state.current(),
        1,
        "next steps forward before wrapping"
    );
    cycling.pane.busy = true;
    cycling.act("tabPrevious");
    assert_eq!(
        (cycling.state.current(), cycling.pane.said.join("|")),
        (0, String::new()),
        "previous while a listing is loading switches tab without refusing"
    );
    cycling.pane.superseded = false;
    cycling.act("tabNext");
    assert_eq!(
        (cycling.state.current(), cycling.pane.said.join("|")),
        (1, String::new()),
        "next while a listing is loading switches tab without refusing"
    );
    assert!(
        cycling.pane.superseded,
        "a tab switch asks for its folder as a navigation, so a listing out gives way"
    );

    let mut single = Side::new(Fake::at("/tmp/only"));
    single.pane.preview_active = true;
    single.pane.filter_query = "keep".into();
    single.pane.toggle(2);
    single.act("tabNext");
    single.act("tabPrevious");
    assert!(
        single.state.tabs.is_none() && single.pane.listed.is_empty(),
        "cycling the only tab does not create tab state or relist"
    );
    assert_eq!(
        (
            single.pane.preview_active,
            single.pane.filter_query.as_str(),
            single.pane.cursor,
            single.pane.selection.clone()
        ),
        (true, "keep", 4, vec![2]),
        "cycling the only tab preserves its preview, filter, cursor and selection"
    );
}

#[test]
fn digits_close_and_the_cap_refuse_in_words() {
    let mut missing = Side::new(Fake::standard());
    missing.act("tab3");
    assert_eq!(
        missing.pane.said.join(""),
        "No tab 3",
        "a digit with no such tab says so in words"
    );

    let mut last = Side::new(Fake::standard());
    last.act("tabClose");
    assert_eq!(
        last.pane.said.join(""),
        "The last tab cannot be closed",
        "w on the only tab refuses rather than closing the window"
    );

    let mut pair = Side::new(Fake::at("/home/gm/a"));
    pair.act("tabNew");
    pair.pane.path = "/home/gm/b".into();
    pair.act("tabClose");
    assert_eq!(pair.count(), 1, "w on a second tab leaves one");
    assert_eq!(
        pair.pane.listed,
        ["/home/gm/a"],
        "and lists the tab that remains"
    );

    let mut capped = Side::new(Fake::standard());
    for _ in 0..12 {
        capped.act("tabNew");
    }
    assert_eq!(
        capped.count(),
        9,
        "the ninth tab is the last one t will open"
    );
    assert_eq!(
        capped.pane.last_said(),
        "Nine tabs is the most.",
        "and the tenth says so"
    );

    let mut loading = Side::new(Fake::standard());
    loading.pane.busy = true;
    loading.act("tabNew");
    assert_eq!(
        (
            loading.count(),
            loading.pane.said.join(""),
            loading.pane.listed.join(","),
            loading.pane.superseded
        ),
        (2, String::new(), "/home/gm/Work".to_string(), true),
        "t while a listing is in flight opens the tab, says nothing and lets that listing go"
    );

    let mut previewing = Side::new(Fake::standard());
    previewing.pane.preview_active = true;
    previewing.act("tabNew");
    assert_eq!(
        previewing.pane.preview_closed, 1,
        "t closes an open preview, so the new tab is not sitting under one"
    );
    assert!(
        !previewing.act("view"),
        "an action that is not a tab's is left to its own owner"
    );
}

#[test]
fn a_pending_order_is_asked_for_before_the_cursor_is_restored() {
    let mut pending = Side::new(Fake::at("/home/gm/a"));
    let mut tabs = pack(Vec::new(), 0, None);
    tabs.pending.cursor = Some(9);
    tabs.pending.order = Some(order("size", true));
    pending.state.tabs = Some(tabs);
    pending.state.apply_pending(&mut pending.pane);
    assert_eq!(
        (pending.pane.sorted.join(","), pending.pane.cursor),
        ("size:true".into(), 4),
        "a pending size order is asked for before the cursor is restored"
    );
    pending.state.apply_pending(&mut pending.pane);
    assert_eq!(
        pending.pane.cursor, 9,
        "the next rows reply restores the cursor"
    );
}

#[test]
fn ctrl_return_opens_the_cursor_folder_in_a_new_tab_and_a_file_opens_none() {
    let mut folder = Side::new(
        Fake::at("/tmp/base")
            .with_rows(|i| if i == 2 { row("sub", true) } else { None })
            .toggled(&[1]),
    );
    folder.pane.cursor = 2;
    folder.act("openTab");
    assert_eq!(
        folder.paths()[1],
        "/tmp/base/sub",
        "a directory opens a tab on that folder"
    );
    assert_eq!(folder.state.current(), 1, "and the new tab is shown");
    let kept = &folder.state.tabs.as_ref().unwrap().items[0];
    assert_eq!(kept.cursor, 2, "and the tab left behind keeps its cursor");
    assert_eq!(kept.selected, [1], "and keeps its selection");

    let mut scoped = Side::new(Fake::at("/tmp/base").with_rows(|i| {
        if i == 2 {
            row("deep/sub", true)
        } else {
            None
        }
    }));
    scoped.pane.search_base = "/tmp/scope".into();
    scoped.pane.cursor = 2;
    scoped.state.open_cursor_tab(&mut scoped.pane);
    assert_eq!(
        scoped.paths()[1],
        "/tmp/scope/deep/sub",
        "a search result folder opens its tab under the search base"
    );

    let mut file = Side::new(
        Fake::at("/tmp/base")
            .with_rows(|_| row("a.txt", false))
            .toggled(&[1]),
    );
    file.pane.cursor = 2;
    file.state.open_cursor_tab(&mut file.pane);
    assert_eq!(file.count(), 1, "a file opens no tab");
    assert_eq!(
        file.pane.last_said(),
        "Only a folder opens in a new tab.",
        "and says only a folder does"
    );
    assert_eq!(file.pane.cursor, 2, "and leaves the cursor where it was");
    assert_eq!(file.pane.selection, [1], "and leaves the selection alone");

    let mut empty = Side::new(Fake::at("/tmp/base").with_rows(|_| None));
    empty.pane.cursor = 2;
    empty.state.open_cursor_tab(&mut empty.pane);
    assert_eq!(empty.count(), 1, "no cursor row opens no tab either");
    assert_eq!(
        empty.pane.last_said(),
        "Only a folder opens in a new tab.",
        "and says the same sentence"
    );

    let mut rooted = Side::new(Fake::at("/").with_rows(|_| row("etc", true)));
    rooted.state.open_cursor_tab(&mut rooted.pane);
    assert_eq!(
        rooted.paths()[1],
        "/etc",
        "a folder under the root joins with one separator"
    );
}

#[test]
fn a_tab_count_verdict_stands_only_until_the_count_changes() {
    let mut lone = Side::new(Fake::at("/tmp/lone"));
    lone.act("tabClose");
    assert_eq!(
        lone.pane.notice, "The last tab cannot be closed",
        "the last-tab verdict stood"
    );
    lone.act("tabNew");
    assert_eq!(
        lone.pane.notice, "",
        "a new tab withdraws the standing last-tab verdict"
    );
    assert_eq!(lone.count(), 2, "and the new tab is still opened");

    let mut cap = Side::new(Fake::at("/tmp/cap"));
    for _ in 0..8 {
        cap.act("tabNew");
    }
    cap.state.open_new(&mut cap.pane, None);
    assert_eq!(
        cap.pane.notice, "Nine tabs is the most.",
        "the cap verdict stood"
    );
    cap.state.close_at(&mut cap.pane, 8);
    assert_eq!(
        cap.pane.notice, "",
        "closing a tab withdraws the standing cap verdict"
    );
    assert_eq!(cap.count(), 8, "and the tab is still closed");

    let mut select = Side::new(Fake::at("/tmp/sel"));
    select.state.open_new(&mut select.pane, None);
    select.state.select_at(&mut select.pane, 7);
    assert_eq!(
        select.pane.notice, "No tab 8",
        "a missing tab still says its number"
    );
    select.state.close_at(&mut select.pane, 1);
    assert_eq!(
        select.pane.notice, "",
        "closing a tab withdraws the standing select verdict"
    );

    let mut close = Side::new(Fake::at("/tmp/close"));
    close.state.open_new(&mut close.pane, None);
    close.state.close_at(&mut close.pane, 7);
    assert_eq!(
        close.pane.notice, "No tab 8",
        "a missing close still says its number"
    );
    close.state.open_new(&mut close.pane, None);
    assert_eq!(
        close.pane.notice, "",
        "a new tab withdraws the standing close verdict"
    );

    let foreign =
        flea_gui::tabs::transfer::payload("222", "tok-9", "/tmp/folder", "grid", "note.txt");
    let mut received = Side::new(Fake::at("/tmp/recv"));
    for _ in 0..8 {
        received.act("tabNew");
    }
    received
        .state
        .receive_tab(&mut received.pane, &foreign, Some(0), "111");
    assert_eq!(
        received.pane.notice, "Nine tabs is the most.",
        "the receive cap verdict stood"
    );
    received.state.close_at(&mut received.pane, 0);
    assert_eq!(
        received.pane.notice, "",
        "closing a tab withdraws the standing receive cap verdict"
    );

    let mut landed = Side::new(Fake::at("/tmp/landed"));
    landed.act("tabClose");
    landed.pane.message("Copy failed.", true);
    assert_eq!(
        landed.pane.notice, "The last tab cannot be closed",
        "the count verdict stands before the landing"
    );
    assert!(
        landed.state.count_notice.is_some(),
        "and it still names its notice"
    );
    landed
        .state
        .receive_tab(&mut landed.pane, &foreign, Some(0), "111");
    assert_eq!(
        landed.pane.notice, "",
        "a received tab withdraws the standing count verdict"
    );
    assert_eq!(
        landed.pane.errors.first().map(String::as_str),
        Some("Copy failed."),
        "and the earlier error stands"
    );
    assert_eq!(landed.count(), 2, "and the tab still landed");

    let mut file = Side::new(Fake::at("/tmp/base").with_rows(|_| row("a.txt", false)));
    file.pane.cursor = 2;
    file.state.open_cursor_tab(&mut file.pane);
    assert!(
        file.state.count_notice.is_none(),
        "a plain refusal names no count notice"
    );
    file.act("tabNew");
    assert_eq!(
        file.pane.notice, "Only a folder opens in a new tab.",
        "an unrelated verdict survives a new tab"
    );
    assert_eq!(file.count(), 2, "and the new tab is still opened");

    let mut load = Side::new(Fake::at("/tmp/loading"));
    load.act("tabClose");
    load.pane.busy = true;
    load.act("tabNew");
    assert_eq!(
        (
            load.pane.notice.as_str(),
            load.state.count_notice,
            load.count()
        ),
        ("", None, 2),
        "a new tab with a listing out withdraws the count verdict and says nothing"
    );

    let mut error = Side::new(Fake::at("/tmp/err"));
    error.act("tabClose");
    error.pane.message("Copy failed.", true);
    error.act("tabNew");
    assert_eq!(
        error.pane.errors.first().map(String::as_str),
        Some("Copy failed."),
        "a new tab withdraws the count verdict but the error stands"
    );
    assert_eq!(error.pane.notice, "", "and the count verdict is gone");
    assert_eq!(error.count(), 2, "and the new tab is still opened");

    let mut other = Side::new(Fake::at("/tmp/other"));
    other.state.open_new(&mut other.pane, None);
    other.state.select_at(&mut other.pane, 7);
    other.pane.message("Unrelated notice.", false);
    other.state.close_at(&mut other.pane, 1);
    assert_eq!(
        other.pane.notice, "Unrelated notice.",
        "a close withdraws nothing once another notice replaced the count verdict"
    );
    assert_eq!(other.count(), 1, "and the tab is still closed");

    let mut expired = Side::new(Fake::at("/tmp/exp"));
    expired.state.open_new(&mut expired.pane, None);
    expired.state.select_at(&mut expired.pane, 7);
    expired.pane.notice.clear();
    expired.pane.message("Later notice.", false);
    expired.act("tabNew");
    assert_eq!(
        expired.pane.notice, "Later notice.",
        "an expired count verdict withdraws nothing, so the later notice stands"
    );
}

#[test]
fn the_board_menu_duplicates_a_tab_beside_itself_and_closes_the_others() {
    let mut menu = Side::new(Fake::at("/tmp/a").toggled(&[3]));
    menu.state.open_new(&mut menu.pane, Some("/tmp/b".into()));
    menu.pane.listed.clear();
    menu.state.duplicate(&mut menu.pane, 0);
    assert_eq!(
        menu.paths(),
        ["/tmp/a", "/tmp/a", "/tmp/b"],
        "a duplicate lands right after the tab it copies"
    );
    assert_eq!(menu.state.current(), 1, "and is shown");
    assert_eq!(
        menu.pane.listed,
        ["/tmp/a"],
        "and lists its folder, as a switch to that tab would"
    );
    let ids = menu.identities();
    assert!(
        ids[0] != ids[1] && ids[1] != ids[2],
        "and carries its own identity, so an ack never closes the original"
    );
    menu.pane.listed.clear();
    menu.state.duplicate(&mut menu.pane, 1);
    assert_eq!(
        (menu.count(), menu.state.current()),
        (4, 2),
        "a duplicate of the current tab is shown in its place"
    );
    assert!(
        menu.pane.listed.is_empty(),
        "and lists nothing, since its folder is the one on screen"
    );

    menu.pane.listed.clear();
    menu.state.close_others(&mut menu.pane, 3);
    assert_eq!(
        (menu.paths(), menu.state.current()),
        (vec!["/tmp/b".to_string()], 0),
        "close others keeps only the chosen tab"
    );
    assert_eq!(
        menu.pane.listed,
        ["/tmp/b"],
        "and shows it, listing its folder"
    );
    assert_eq!(menu.count(), 1, "and the strip is down to one");
    menu.state.close_others(&mut menu.pane, 0);
    assert_eq!(
        menu.pane.last_said(),
        "The last tab cannot be closed",
        "close others on a lone tab refuses like a close"
    );

    let mut current = Side::pair("/tmp/c");
    current
        .state
        .open_new(&mut current.pane, Some("/tmp/d".into()));
    current.pane.listed.clear();
    current.state.close_others(&mut current.pane, 2);
    assert_eq!(
        (current.paths(), current.pane.listed.len()),
        (vec!["/tmp/d".to_string()], 0),
        "close others on the current tab lists nothing"
    );

    let mut full = Side::new(Fake::at("/tmp/full"));
    let nine: Vec<String> = (0..9).map(|t| format!("/tmp/{t}")).collect();
    full.state.tabs = Some(pack(nine.iter().map(Tab::at).collect(), 0, None));
    full.state.duplicate(&mut full.pane, 0);
    assert_eq!(
        (full.count(), full.pane.last_said()),
        (9, "Nine tabs is the most.".into()),
        "a duplicate on a full strip says the cap"
    );
    full.state.duplicate(&mut full.pane, 9);
    full.state.close_others(&mut full.pane, 9);
    assert_eq!(full.count(), 9, "no such tab duplicates or closes nothing");
}
