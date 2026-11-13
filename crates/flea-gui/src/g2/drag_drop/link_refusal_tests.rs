//! The drops a link row still refuses, the footer's sentence while a folder waits for its facts, and the link as a Grid tile.
use super::link_target_tests::{
    cue, dropped, footer, hold, nothing_sent, open, ALPHA, BETA, DANGLING, FILE, INTO_ALPHA,
    TO_ELSE, TO_FILE, TO_SELF, UNNAMED,
};
use super::mounted_tests::{centre, frame, transfer, Root};
use crate::list::bench;
use gpui::{point, px, AppContext, Modifiers, MouseButton, VisualTestContext};

#[test]
fn fxdroptarget_a_link_to_the_items_own_folder_is_refused_as_already_here() {
    let root = Root::new("link-self-cue");
    let mut cx = bench::real_cx();
    let (handle, _lines) = open(&mut cx, &root);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    hold(&mut vcx, &mut cx, handle, FILE, TO_SELF);
    assert_eq!(cue(&mut vcx, handle), (None, None), "the row stays unlit");
    assert_eq!(footer(&mut vcx, handle), [UNNAMED]);
    drop(vcx);
    let (sent, said, _) = dropped(FILE, TO_SELF, Modifiers::default());
    assert!(nothing_sent(&sent), "{sent:?}");
    assert_eq!(said, "Already in this folder.");
}

#[test]
fn fxdroptarget_a_link_into_the_dragged_folder_is_refused_as_the_folder_itself_is() {
    for keys in [Modifiers::default(), Modifiers::control()] {
        let (sent, said, _) = dropped(ALPHA, INTO_ALPHA, keys);
        assert!(nothing_sent(&sent), "{sent:?}");
        assert_eq!(said, "That folder is inside the drag.");
    }
}

#[test]
fn fxdroptarget_a_link_to_a_file_or_to_nothing_takes_no_drop() {
    for link in [TO_FILE, DANGLING] {
        let root = Root::new("link-plain");
        let mut cx = bench::real_cx();
        let (handle, _lines) = open(&mut cx, &root);
        let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
        hold(&mut vcx, &mut cx, handle, FILE, link);
        assert_eq!(cue(&mut vcx, handle), (None, None));
        drop(vcx);
        let (sent, said, _) = dropped(FILE, link, Modifiers::default());
        assert!(nothing_sent(&sent), "{sent:?}");
        assert_eq!(said, "Already in this folder.");
    }
}

#[test]
fn fxdroptarget_the_footer_keeps_its_sentence_while_a_folder_waits_for_its_facts() {
    use gpui::{InputEvent, MouseMoveEvent};
    let root = Root::new("pending-line");
    let mut cx = bench::real_cx();
    let (handle, _lines) = open(&mut cx, &root);
    let from = centre(&mut cx, handle, FILE);
    let folders = [ALPHA, BETA].map(|row| centre(&mut cx, handle, row));
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    vcx.simulate_mouse_move(
        point(from.x + px(12.), from.y),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    vcx.run_until_parked();
    assert_eq!(footer(&mut vcx, handle), [UNNAMED]);
    // From the floor onto a folder, then from that folder onto the next one.
    for (to, name) in folders.into_iter().zip(["alpha", "beta"]) {
        // The move is dispatched without running the executor, so the folder's facts are still unread.
        vcx.update_window(handle.into(), |_, window, cx| {
            let moved = MouseMoveEvent {
                position: to,
                pressed_button: Some(MouseButton::Left),
                modifiers: Modifiers::none(),
            };
            window.dispatch_event(moved.to_platform_input(), cx);
        })
        .unwrap();
        assert_eq!(
            footer(&mut vcx, handle),
            [UNNAMED],
            "the pending frame over {name} keeps the sentence"
        );
        vcx.run_until_parked();
        assert_eq!(
            footer(&mut vcx, handle),
            [format!("Move 1 item to {name} · ctrl copies, shift moves")]
        );
    }
}

#[test]
fn fxdroptarget_a_grid_tile_and_a_resolved_row_take_the_link_as_a_folder() {
    let root = Root::new("link-grid");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, &root);
    // What Columns' own column and a keyboard carry resolve a row to.
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let spot = |row| super::join::row_spot(host, Some(row), &[]);
            assert_eq!(spot(TO_ELSE), format!("{}/to-else", host.path));
            assert_eq!(spot(TO_FILE), host.path);
            assert_eq!(spot(DANGLING), host.path);
        })
        .unwrap();
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.set_view(crate::window::startup::View::Grid, window, cx)
        })
        .unwrap();
    frame(&mut cx, handle);
    frame(&mut cx, handle);
    let [file, link] = [FILE, TO_ELSE].map(|index| {
        handle
            .update(&mut cx, |shell, _, _| {
                let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
                let state = grid.state.borrow();
                let slot = state.drawn_row(index).expect("the grid drew the tile").slot;
                state.bounds.origin
                    + point(px(slot.x + slot.width / 2.), px(slot.y + slot.height / 2.))
            })
            .unwrap()
    });
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    super::rail_drop_tests::hold(&mut vcx, file, link, Modifiers::default());
    assert_eq!(
        footer(&mut vcx, handle),
        ["Move 1 item to to-else · ctrl copies, shift moves"]
    );
    let lit = handle
        .update(&mut vcx, |shell, window, cx| {
            shell.file_grid_cue(0, window, cx).map(|(row, _)| row)
        })
        .unwrap();
    assert_eq!(lit, Some(TO_ELSE));
    vcx.simulate_mouse_up(link, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    let transfer = transfer(&mut vcx, handle, &mut lines);
    assert!(
        transfer.contains(&format!(r#""dest":"{}/to-else""#, root.src())),
        "{transfer}"
    );
}

#[test]
fn fxdroptarget_facts_resolve_a_linked_listing_and_never_follow_a_dragged_link() {
    use flea::ui_logic_drag_drop::Refusal;
    let root = Root::new("link-facts");
    let src = std::path::PathBuf::from(root.src());
    let top = src.parent().unwrap().to_owned();
    std::fs::create_dir_all(top.join("elsewhere")).unwrap();
    std::os::unix::fs::symlink("src", top.join("via")).unwrap();
    std::os::unix::fs::symlink("../elsewhere", src.join("to-else")).unwrap();
    let text = |path: std::path::PathBuf| path.to_string_lossy().into_owned();
    let via = |name: &str| text(top.join("via").join(name));
    // The listing shown through a link: its item already sits in the folder the link names.
    let here = super::facts::read(&[via("b.txt")], &root.src());
    assert_eq!(here.refused, Some(Refusal::AlreadyHere));
    // A dragged link to a folder may move into the folder it points at.
    let into = super::facts::read(&[via("to-else")], &text(top.join("elsewhere")));
    assert_eq!(into.refused, None);
}
