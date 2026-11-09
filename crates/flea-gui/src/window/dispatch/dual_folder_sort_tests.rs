//! A folder open in both Dual panes shows one remembered sort: a sort or a forget in one pane re-sorts the other.
use crate::window::dispatch::menus_tests::{home, settle};
use crate::window::dispatch::test_shell::{asked_on, land_on, open_with, sent};
use crate::{
    navigation::{folder_state::Order, sort::Request, Refusal},
    state::Shell,
    window::navigate::Step,
};
use gpui::{Entity, TestAppContext, VisualTestContext};

const NAME: Order = Order {
    key: "name",
    desc: false,
};
const NAME_DESC: Order = Order {
    key: "name",
    desc: true,
};
const SIZE: Order = Order {
    key: "size",
    desc: false,
};
const SIZE_DESC: Order = Order {
    key: "size",
    desc: true,
};

fn dual(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = open_with(
        cx,
        r#""keys":"default","view":"dual","rememberSort":true"#,
        None,
    );
    for side in [0, 1] {
        let path = asked_on(&shell, cx, side);
        assert_eq!(path, "/files", "both panes open the launch folder");
        land_on(&shell, cx, side, &path, home(), 4, 1);
    }
    settle(cx);
    (shell, cx)
}

fn step(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    side: usize,
    step: Step,
) -> Result<bool, Refusal> {
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.step(side, step, window, cx)))
}

fn order(shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize) -> Order {
    shell.read_with(cx, |shell, _| shell.panes[side].as_ref().unwrap().nav.order)
}

fn writes(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> usize {
    shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .writer
            .borrow()
            .sort_writes()
    })
}

// The listing the pane has out, answered so it lands.
fn relist(shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize) {
    let (path, listing) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[side].as_ref().unwrap();
        let path = host.pane.swap.pending.as_ref().unwrap().path.clone();
        (path, host.pane.greatest_listing + 1)
    });
    land_on(shell, cx, side, &path, home(), 4, listing);
    settle(cx);
}

#[gpui::test]
fn zm14_a_sort_in_one_dual_pane_is_the_folders_in_the_other(cx: &mut TestAppContext) {
    let (shell, cx) = dual(cx);
    assert_eq!((order(&shell, cx, 0), order(&shell, cx, 1)), (NAME, NAME));
    assert_eq!(step(&shell, cx, 0, Step::SortNext), Ok(true));
    settle(cx);
    assert_eq!(writes(&shell, cx), 1, "the choosing pane writes once");
    assert_eq!(
        order(&shell, cx, 1),
        SIZE,
        "the other pane on the folder takes its new sort"
    );
    // The follower re-sorts as a restore, so the right pane's own reverse is still a choice of its own.
    assert_eq!(step(&shell, cx, 1, Step::SortReverse), Ok(true));
    settle(cx);
    assert_eq!(
        writes(&shell, cx),
        2,
        "one write per choice, none per follow"
    );
    assert_eq!(
        order(&shell, cx, 0),
        SIZE_DESC,
        "and the left pane follows back"
    );
    shell.read_with(cx, |shell, _| {
        let folders = shell.panes[0].as_ref().unwrap().nav.folders.borrow();
        assert_eq!(folders.order_for("/files"), SIZE_DESC);
    });
    // Forgetting in one pane returns both to the window's order.
    assert_eq!(step(&shell, cx, 0, Step::ForgetSort), Ok(true));
    settle(cx);
    assert_eq!(
        order(&shell, cx, 1),
        NAME,
        "the forget reaches the other pane"
    );
    relist(&shell, cx, 0);
    assert_eq!(order(&shell, cx, 0), NAME);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn zm14_a_listing_out_takes_the_sort_on_its_folder_only(cx: &mut TestAppContext) {
    let (shell, cx) = dual(cx);
    // A re-read of the same folder was asked in the old order, so the new one waits for it and then applies.
    assert_eq!(step(&shell, cx, 1, Step::Reread), Ok(true));
    assert_eq!(step(&shell, cx, 0, Step::SortNext), Ok(true));
    let pending = shell.read_with(cx, |shell, _| {
        shell.panes[1].as_ref().unwrap().nav.pending_sort
    });
    assert_eq!(pending, Some(Request::Restore(SIZE)));
    sent();
    relist(&shell, cx, 1);
    assert_eq!(order(&shell, cx, 1), SIZE, "the landed re-read re-sorts");
    assert!(
        sent()
            .iter()
            .any(|line| line.contains(r#""c":"sort""#) && line.contains(r#""size""#)),
        "the held order reaches the backend once the listing lands"
    );
    // A pane on its way to another folder keeps that folder's own order.
    assert_eq!(step(&shell, cx, 1, Step::Open("/other".into())), Ok(true));
    assert_eq!(step(&shell, cx, 0, Step::SortReverse), Ok(true));
    let pending = shell.read_with(cx, |shell, _| {
        shell.panes[1].as_ref().unwrap().nav.pending_sort
    });
    assert_eq!(pending, None, "a sort on /files never reaches /other");
    relist(&shell, cx, 1);
    assert_eq!(order(&shell, cx, 1), NAME);
    assert_eq!(order(&shell, cx, 0), SIZE_DESC);
    // A pane that leaves the folder before its listing lands drops the order it was holding for that folder.
    assert_eq!(step(&shell, cx, 1, Step::Open("/files".into())), Ok(true));
    assert_eq!(step(&shell, cx, 0, Step::SortReverse), Ok(true));
    let pending = shell.read_with(cx, |shell, _| {
        shell.panes[1].as_ref().unwrap().nav.pending_sort
    });
    assert_eq!(pending, Some(Request::Restore(SIZE)));
    assert_eq!(step(&shell, cx, 1, Step::Open("/other".into())), Ok(true));
    let pending = shell.read_with(cx, |shell, _| {
        shell.panes[1].as_ref().unwrap().nav.pending_sort
    });
    assert_eq!(
        pending, None,
        "the next folder never takes the old one's order"
    );
    relist(&shell, cx, 1);
    assert_eq!(order(&shell, cx, 1), NAME);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn zm14_with_folder_memory_off_each_pane_keeps_its_own_sort(cx: &mut TestAppContext) {
    let (shell, cx) = dual(cx);
    shell.update(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .folders
            .borrow_mut()
            .remember_sort = false;
    });
    assert_eq!(step(&shell, cx, 0, Step::SortNext), Ok(true));
    settle(cx);
    assert_eq!(order(&shell, cx, 0), SIZE);
    assert_eq!(order(&shell, cx, 1), NAME, "no folder memory, no follow");
    // A forget with folder memory off writes nothing, so the other pane keeps its own sort.
    assert_eq!(step(&shell, cx, 1, Step::SortReverse), Ok(true));
    settle(cx);
    assert_eq!(step(&shell, cx, 0, Step::ForgetSort), Ok(true));
    settle(cx);
    relist(&shell, cx, 0);
    assert_eq!(order(&shell, cx, 0), NAME, "the forget resets its own pane");
    assert_eq!(
        order(&shell, cx, 1),
        NAME_DESC,
        "a forget with memory off never reaches the other pane"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
