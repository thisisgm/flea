use super::*;
use flea_gui::tabs::state::pack;

fn searching_home(side: &mut Side) {
    side.pane.search_mode = "results";
    side.pane.search_query = "gm".into();
    side.pane.search_from = "/home/gm".into();
}

#[test]
fn a_search_dropped_onto_its_own_scope_lists_that_scope_again() {
    let mut scoped = Side::new(Fake::at("/home/gm"));
    searching_home(&mut scoped);
    scoped.state.open_new(&mut scoped.pane, None);
    assert_eq!(
        scoped.pane.listed,
        ["/home/gm"],
        "a tab opened onto the scope its search walked lists it again"
    );

    let mut switching = Side::new(Fake::at("/home/gm"));
    switching.state.open_new(&mut switching.pane, None);
    switching.pane.listed.clear();
    searching_home(&mut switching);
    switching.state.select_at(&mut switching.pane, 0);
    assert_eq!(
        switching.pane.listed,
        ["/home/gm"],
        "switching to a tab standing on that scope lists it again"
    );

    let mut closing = Side::new(Fake::at("/home/gm"));
    closing.state.open_new(&mut closing.pane, None);
    closing.pane.listed.clear();
    searching_home(&mut closing);
    closing.state.close_at(&mut closing.pane, 1);
    assert_eq!(
        closing.pane.listed,
        ["/home/gm"],
        "closing a searching tab onto one on that scope lists it again"
    );
}

#[test]
fn a_selection_returns_only_to_the_rows_it_was_read_off() {
    let mut same = Side::new(Fake::standard().toggled(&[3, 4]));
    let unchanged = same.state.snapshot(&same.pane, None, 0, false);
    same.pane.clear_selection();
    same.state.apply(&mut same.pane, &unchanged, false, None);
    assert_eq!(
        same.pane.selection,
        [3, 4],
        "an unchanged directory restores the selection it recorded"
    );

    let mut reread = Side::new(Fake::standard().toggled(&[3, 4]));
    let stale = reread.state.snapshot(&reread.pane, None, 0, false);
    reread.pane.list_requests += 1;
    reread.state.apply(&mut reread.pane, &stale, false, None);
    assert!(
        reread.pane.selection.is_empty(),
        "a listing re-read while the tab was hidden carries none of it back"
    );

    let mut reordered = Side::new(Fake::standard().toggled(&[3]));
    reordered.pane.thumbs_stale = true;
    reordered.pane.sizes_stale = true;
    reordered.state.tabs = Some(pack(Vec::new(), 0, None));
    let mut other = reordered.state.snapshot(&reordered.pane, None, 0, false);
    other.order = order("size", true);
    reordered
        .state
        .apply(&mut reordered.pane, &other, false, None);
    assert_eq!(
        (
            reordered.pane.selection.len(),
            reordered.pane.thumbs_stale,
            reordered.pane.sizes_stale
        ),
        (0, false, false),
        "a different order in the other tab drops the selection and both row-indexed caches"
    );
    assert_eq!(
        (
            reordered.pane.sorted.join(","),
            reordered.pane.order,
            reordered.pane.windows.join(",")
        ),
        ("size:true".into(), order("size", true), "0:40".into()),
        "and the order that tab recorded is the one asked for, with its first window"
    );
    assert!(
        reordered.pane.listed.is_empty(),
        "and the reorder lists nothing"
    );
    assert_eq!(
        reordered.state.tabs.as_ref().unwrap().pending.cursor,
        Some(4),
        "and the tab's cursor lands on the reorder's first rows"
    );

    let mut lone = Side::new(Fake::standard().toggled(&[5]));
    lone.pane.follows = true;
    let snap = lone.state.snapshot(&lone.pane, None, 0, false);
    lone.pane.clear_selection();
    lone.pane.follows = false;
    lone.state.apply(&mut lone.pane, &snap, false, None);
    assert_eq!(
        (lone.pane.selection.clone(), lone.pane.follows),
        (vec![5], true),
        "a lone row restores as the only one, so it still follows a plain move"
    );
}

#[test]
fn a_half_typed_search_relists_nothing_and_the_dotfile_answer_is_the_tabs_own() {
    let mut typing = Side::new(Fake::at("/home/gm").toggled(&[2]));
    typing.state.open_new(&mut typing.pane, None);
    typing.pane.listed.clear();
    typing.pane.search_mode = "typing";
    typing.state.select_at(&mut typing.pane, 0);
    assert!(
        typing.pane.listed.is_empty(),
        "switching with the search field open but no results re-lists nothing"
    );
    assert_eq!(
        typing.pane.selection,
        [2],
        "and the tab's own selection comes back rather than being dropped"
    );

    let mut dots = Side::new(Fake::at("/home/gm"));
    dots.state.open_new(&mut dots.pane, None);
    dots.pane.hidden = true;
    dots.pane.preference_hidden = true;
    dots.pane.path = "/home/gm/Work".into();
    dots.pane.listed.clear();
    dots.state.select_at(&mut dots.pane, 0);
    assert!(
        !dots.pane.hidden,
        "a tab that had dotfiles off keeps them off when the other tab turned them on"
    );
    assert_eq!(
        dots.pane.listed,
        ["/home/gm"],
        "and the switch is a re-listing, because the rows it wants are not the ones on screen"
    );
    assert!(
        !dots.pane.cleared_at_once,
        "and in the same view it lets the swap hold the rows on screen until it lands"
    );

    let mut viewed = Side::new(Fake::at("/home/gm"));
    viewed.state.open_new(&mut viewed.pane, None);
    viewed.pane.view = "grid";
    viewed.pane.path = "/home/gm/Work".into();
    viewed.pane.listed.clear();
    viewed.state.select_at(&mut viewed.pane, 0);
    assert_eq!(
        (viewed.pane.listed.join(","), viewed.pane.view, viewed.pane.cleared_at_once),
        ("/home/gm".into(), "list", true),
        "a switch to a tab in another view clears at once rather than drawing these rows in that view"
    );
    assert_eq!(
        viewed.pane.adopted.last(),
        Some(&("/home/gm".to_string(), "list")),
        "and the tab's view is offered to its folder's own view memory"
    );
}

#[test]
fn an_order_the_listing_already_has_is_spent_on_that_same_reply() {
    let mut already = Side::new(Fake::at("/home/gm/a"));
    let mut tabs = pack(Vec::new(), 0, None);
    tabs.pending.cursor = Some(9);
    tabs.pending.order = Some(order("name", false));
    already.state.tabs = Some(tabs);
    already.state.apply_pending(&mut already.pane);
    assert!(
        already.pane.sorted.is_empty(),
        "an order the listing already has asks for no sort"
    );
    assert_eq!(
        already.pane.cursor, 9,
        "and the cursor is restored on that same reply"
    );
    assert_eq!(
        already.state.tabs.as_ref().unwrap().pending.order,
        None,
        "and the pending order is spent"
    );
    already.pane.sort(order("size", true));
    already.pane.sorted.clear();
    already.state.apply_pending(&mut already.pane);
    assert_eq!(
        (already.pane.sorted.len(), already.pane.order),
        (0, order("size", true)),
        "so a later user sort survives the reply that answers it"
    );
}

fn waiting_tabs() -> Side {
    let mut side = Side::new(Fake::at("/home/gm"));
    side.state.open_new(&mut side.pane, None);
    side.pane.path = "/home/gm/Work".into();
    side.pane.listing_path.clear();
    side.pane.listed.clear();
    side.pane.busy = true;
    side.pane.listing_state = "waiting";
    side
}

fn wait(side: &Side) -> (bool, &'static str, usize) {
    (
        side.pane.busy,
        side.pane.listing_state,
        side.pane.said.len(),
    )
}

#[test]
fn a_listing_past_its_deadline_is_let_go_by_a_listing_operation_and_kept_by_one_that_lists_nothing()
{
    let mut picked = waiting_tabs();
    picked.state.select_at(&mut picked.pane, 0);
    assert_eq!(
        (picked.pane.listed.join(","), wait(&picked)),
        ("/home/gm".into(), (false, "loading", 0)),
        "switching tabs past the wait lists the tab's folder"
    );

    let mut current = waiting_tabs();
    current.state.select_at(&mut current.pane, 1);
    assert_eq!(
        (current.pane.listed.len(), wait(&current)),
        (0, (true, "waiting", 0)),
        "selecting the current tab lists nothing and keeps the wait"
    );

    let mut alike = waiting_tabs();
    alike.state.tabs.as_mut().unwrap().items[0].path = "/home/gm/Work".into();
    alike.state.select_at(&mut alike.pane, 0);
    assert_eq!(
        (alike.pane.listed.join(","), wait(&alike)),
        ("/home/gm/Work".into(), (false, "loading", 0)),
        "a tab on the folder being shown still lists, the held listing is not its answer"
    );

    let mut opened = waiting_tabs();
    opened.state.open_new(&mut opened.pane, None);
    assert_eq!(
        (opened.pane.listed.join(","), opened.count(), wait(&opened)),
        ("/home/gm/Work".into(), 3, (false, "loading", 0)),
        "a new tab past the wait lists its folder"
    );

    let mut dropped = waiting_tabs();
    dropped.state.close_at(&mut dropped.pane, 0);
    assert_eq!(
        (dropped.pane.listed.len(), dropped.count(), wait(&dropped)),
        (0, 1, (true, "waiting", 0)),
        "closing a hidden tab lists nothing and keeps the wait"
    );

    let mut closed = waiting_tabs();
    closed.state.close_at(&mut closed.pane, 1);
    assert_eq!(
        (closed.pane.listed.join(","), wait(&closed)),
        ("/home/gm".into(), (false, "loading", 0)),
        "closing the current tab lists the one it lands on"
    );

    let mut moved = waiting_tabs();
    moved.state.move_tab(&mut moved.pane, 0, 1);
    assert_eq!(
        (moved.paths(), moved.pane.listed.len(), wait(&moved)),
        (
            vec!["/home/gm/Work".to_string(), "/home/gm".to_string()],
            0,
            (true, "waiting", 0)
        ),
        "reordering lists nothing and keeps the wait"
    );

    let mut received = waiting_tabs();
    assert!(
        received.state.can_receive(),
        "a waiting pane takes a dropped tab"
    );
    let payload = flea_gui::tabs::transfer::payload("222", "tok", "/tmp/r", "list", "");
    let took = received
        .state
        .receive_tab(&mut received.pane, &payload, Some(1), "111");
    assert_eq!(
        (took, received.pane.listed.join(","), wait(&received)),
        (true, "/tmp/r".into(), (false, "loading", 0)),
        "and lists its folder"
    );

    let mut sent = waiting_tabs();
    let closed_out = sent.state.close_after_move(&mut sent.pane, 0);
    assert_eq!(
        (closed_out, sent.count(), wait(&sent)),
        (true, 1, (true, "waiting", 0)),
        "a tab moved out of a waiting pane closes"
    );
}

#[test]
fn a_direct_close_while_loading_lists_the_surviving_tab_over_the_one_out() {
    let mut direct = Side::new(Fake::at("/tmp/one"));
    direct.state.open_new(&mut direct.pane, None);
    direct.pane.path = "/tmp/two".into();
    direct.pane.listing_path = "/tmp/two".into();
    direct.pane.busy = true;
    direct.pane.superseded = false;
    direct.state.close_at(&mut direct.pane, 1);
    assert_eq!(
        (direct.count(), direct.pane.said.join("")),
        (1, String::new()),
        "a direct close while loading closes the tab, refusing nothing"
    );
    assert_eq!(
        (
            direct.state.current(),
            direct.pane.path.as_str(),
            direct.pane.superseded
        ),
        (0, "/tmp/one", true),
        "and the surviving tab is current, listed over the one out"
    );
    assert_eq!(direct.pane.listed.last().map(String::as_str), Some("/tmp/one"), "and the listing asked is the surviving tab's, so the closed tab's late reply has nothing to land on");
}

#[test]
fn a_switch_carries_the_tabs_history_and_keeps_the_dotfile_answer_on_its_listing() {
    let mut side = Side::new(Fake::at("/tmp/a"));
    side.pane.back = vec![Entry::new("/tmp", "a", 3)];
    side.state.open_new(&mut side.pane, Some("/tmp/b".into()));
    side.pane.back = vec![Entry::new("/tmp/a", "b", 1)];
    side.pane.forward = vec![Entry::new("/tmp/c", "", 0)];
    side.state.select_at(&mut side.pane, 0);
    assert_eq!(
        side.pane.back,
        [Entry::new("/tmp", "a", 3)],
        "the tab's own back history returns with it"
    );
    assert!(side.pane.forward.is_empty(), "and its own forward history");
    let left = &side.state.tabs.as_ref().unwrap().items[1];
    assert_eq!(
        (left.back.len(), left.forward.len()),
        (1, 1),
        "and the tab left behind keeps both of its own"
    );
    let open = *side.pane.opened.last().unwrap();
    assert!(
        open.keep_hidden && open.supersede,
        "a switch keeps the tab's own dotfile answer and lets a listing out go"
    );
    assert_eq!(
        open.order,
        Some(Order::default()),
        "and asks its listing in the tab's own order"
    );
}
