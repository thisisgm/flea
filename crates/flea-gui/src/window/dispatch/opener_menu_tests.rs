//! The menu's Open uses its validated paths through the production Shell opener, the cursor's own file keeping Enter's path.
use super::opener_tests::{home, launched, looking, quit, shell};
use super::test_shell::{asked, asked_on, land, land_on, row, set_cursor};
use super::Order;
use crate::g2::context_menus::join::{Captured, Chosen};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::rc::Rc;

const REGULAR_FILE_KIND: u32 = 0o100000;

// The menu's Open chosen on the left pane's cursor row with these marked paths; answers whether the opener claimed it.
fn menu_open(
    shell: &Entity<crate::state::Shell>,
    cx: &mut VisualTestContext,
    cursor: usize,
    paths: &[&str],
) -> bool {
    menu_open_on(shell, cx, 0, cursor, paths, None)
}

fn menu_open_on(
    shell: &Entity<crate::state::Shell>,
    cx: &mut VisualTestContext,
    side: usize,
    cursor: usize,
    paths: &[&str],
    rows: Option<&[usize]>,
) -> bool {
    let claimed = Rc::new(std::cell::Cell::new(false));
    let chosen = Chosen {
        captured: Captured {
            side,
            listing: shell.read_with(cx, |shell, _| {
                shell.panes[side].as_ref().unwrap().pane.held.listing
            }),
            rows: rows
                .map(<[usize]>::to_vec)
                .unwrap_or_else(|| (cursor..cursor + paths.len()).collect()),
            cursor,
            path: "/files".into(),
            identity: String::new(),
            id: 7,
            ready: true,
            kinds: Vec::new(),
            background: false,
            target: None,
        },
        action: "open".into(),
        leaf: String::new(),
        paths: paths.iter().map(|path| path.to_string()).collect(),
        claimed: claimed.clone(),
    };
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.menu.context.row_is_file = true;
            shell.dispatch(Order::Menu(chosen), window, cx);
        })
    });
    cx.run_until_parked();
    claimed.get()
}

#[gpui::test]
fn the_menu_open_on_a_file_runs_the_opener(cx: &mut TestAppContext) {
    let (shell, cx, _, _keep) = shell(cx);
    set_cursor(&shell, cx, 1);
    assert!(
        menu_open(&shell, cx, 1, &["/files/notes.txt"]),
        "the opener claims the choice, so no refusal follows"
    );
    assert_eq!(launched(), ["/files/notes.txt"]);
    quit(&shell, cx);
}

#[gpui::test]
fn search_enter_reveal_menu_open_still_opens_the_audio_result(cx: &mut TestAppContext) {
    let (shell, cx, _, _keep) = shell(cx);
    cx.simulate_keystrokes("f s");
    super::search_open_tests::answer(
        &shell,
        cx,
        1,
        vec![
            row("Music/Album", true),
            row("Music/Album/track.flac", false),
        ],
    );
    cx.simulate_keystrokes("down");
    assert!(menu_open(&shell, cx, 1, &["/files/Music/Album/track.flac"]));
    assert_eq!(launched(), ["/files/Music/Album/track.flac"]);
    quit(&shell, cx);
}

#[gpui::test]
fn the_menu_open_on_an_archive_hands_it_to_quick_look(cx: &mut TestAppContext) {
    let (shell, cx, (actions, _), _keep) = shell(cx);
    set_cursor(&shell, cx, 2);
    assert!(menu_open(&shell, cx, 2, &["/files/backup.tar.zst"]));
    assert_eq!(
        launched(),
        Vec::<String>::new(),
        "an archive never reaches flea --open"
    );
    assert!(actions.borrow().is_empty(), "{:?}", actions.borrow());
    assert_eq!(
        looking(&shell, cx),
        "true",
        "the archive opens in Quick Look"
    );
    quit(&shell, cx);
}

#[gpui::test]
fn the_menu_open_opens_the_whole_captured_selection(cx: &mut TestAppContext) {
    let (shell, cx, _, _keep) = shell(cx);
    set_cursor(&shell, cx, 1);
    // Open takes the validated captured selection, independent of the cursor.
    assert!(menu_open(
        &shell,
        cx,
        1,
        &["/files/todo.txt", "/files/notes.txt"]
    ));
    assert_eq!(launched(), ["/files/todo.txt /files/notes.txt"]);
    quit(&shell, cx);
}

#[gpui::test]
fn gh350_menu_open_uses_every_captured_path_including_offscreen_marks(cx: &mut TestAppContext) {
    let (shell, cx, _, _keep) = shell(cx);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("f5");
    let count = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.first_count()
    });
    land_on(
        &shell,
        cx,
        0,
        "/files",
        (0..count)
            .map(|i| row(&format!("filler-{i}.txt"), false))
            .collect(),
        1000,
        2,
    );
    assert!(shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        !host.pane.busy() && host.pane.held.row(900).is_none() && host.pane.held.row(901).is_none()
    }));
    assert!(menu_open_on(
        &shell,
        cx,
        0,
        1,
        &[
            "/files/one.png",
            "/files/two.png",
            "/files/three.png",
            "/files/notes.txt"
        ],
        Some(&[1, 2, 900, 901])
    ));
    assert_eq!(
        launched(),
        ["/files/one.png /files/two.png /files/three.png /files/notes.txt"]
    );
    quit(&shell, cx);
}

#[gpui::test]
fn the_menu_open_in_a_busy_pane_launches_nothing(cx: &mut TestAppContext) {
    let (shell, cx, _, _keep) = shell(cx);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("f5");
    assert!(shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.busy()));
    assert!(menu_open(&shell, cx, 1, &["/files/notes.txt"]));
    assert_eq!(
        launched(),
        Vec::<String>::new(),
        "a busy pane hands nothing to flea --open"
    );
    land(&shell, cx, "/files", home(), 2);
    quit(&shell, cx);
}

#[gpui::test]
fn the_menu_open_on_the_unfocused_dual_pane_opens_that_panes_row(cx: &mut TestAppContext) {
    let (shell, cx, _, _keep) = shell(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.set_view(crate::window::startup::View::Dual, window, cx)
        })
    });
    let right = asked_on(&shell, cx, 1);
    land_on(
        &shell,
        cx,
        1,
        &right,
        vec![row("beta", true), row("report.pdf", false)],
        2,
        1,
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.focus_side),
        0,
        "the right pane is unfocused when its menu opens"
    );
    // A right click on the right pane's file row; the left cursor stays on folder alpha, which a left-pane Open would enter.
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_menu(1, Some(1), (10., 10.), window, cx)
        })
    });
    let file = format!("{}/report.pdf", right.trim_end_matches('/'));
    assert!(menu_open_on(&shell, cx, 1, 1, &[&file], None));
    assert_eq!(launched(), [file], "Open ran on the menu's pane");
    assert_eq!(asked(&shell, cx), "/files", "the left pane did not move");
    quit(&shell, cx);
}

#[gpui::test]
fn gh350_menu_open_keeps_its_single_validated_target_when_cursor_is_an_archive(
    cx: &mut TestAppContext,
) {
    let (shell, cx, (actions, _), _keep) = shell(cx);
    set_cursor(&shell, cx, 2);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![1]
    });
    assert!(menu_open_on(
        &shell,
        cx,
        0,
        2,
        &["/files/notes.txt"],
        Some(&[1])
    ));
    assert_eq!(launched(), ["/files/notes.txt"]);
    assert!(
        !actions.borrow().contains(&"preview"),
        "the unrelated cursor archive cannot replace the validated target"
    );
    assert_eq!(
        looking(&shell, cx),
        "false",
        "the unrelated cursor archive opens no Quick Look"
    );
    quit(&shell, cx);
}

#[gpui::test]
fn ey379_neighbour_archive_open_waits_for_its_own_row_and_never_opens_a_replacement(
    cx: &mut TestAppContext,
) {
    for missing in [false, true] {
        let (shell, cx, _, _keep) = shell(cx);
        let mut archive = home().remove(2);
        archive.name = "neighbour.zip".into();
        let chosen = Chosen {
            captured: Captured {
                side: 0,
                listing: 1,
                rows: vec![0],
                cursor: 0,
                path: "/parent".into(),
                identity: String::new(),
                id: 7,
                ready: true,
                kinds: vec![REGULAR_FILE_KIND],
                background: false,
                target: Some(archive.clone()),
            },
            action: "open".into(),
            leaf: String::new(),
            paths: vec!["/parent/neighbour.zip".into()],
            claimed: Rc::new(std::cell::Cell::new(false)),
        };
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.menu.context.row_is_file = true;
                shell.dispatch(Order::Menu(chosen), window, cx);
            })
        });
        assert_eq!(asked(&shell, cx), "/parent");
        assert_eq!(launched(), Vec::<String>::new());
        let rows = if missing {
            vec![row("replacement.txt", false)]
        } else {
            vec![archive]
        };
        land(&shell, cx, "/parent", rows, 2);
        assert_eq!(looking(&shell, cx), (!missing).to_string());
        assert_eq!(launched(), Vec::<String>::new());
        quit(&shell, cx);
    }
}

#[gpui::test]
fn ey379_neighbour_image_open_requests_its_own_folder_gallery(cx: &mut TestAppContext) {
    let (shell, cx, _, _keep) = shell(cx);
    let mut image = row("neighbour.png", false);
    image.icon = "image-x-generic".into();
    let chosen = Chosen {
        captured: Captured {
            side: 0,
            listing: 1,
            rows: vec![0],
            cursor: 0,
            path: "/parent".into(),
            identity: String::new(),
            id: 7,
            ready: true,
            kinds: vec![REGULAR_FILE_KIND],
            background: false,
            target: Some(image.clone()),
        },
        action: "open".into(),
        leaf: String::new(),
        paths: vec!["/parent/neighbour.png".into()],
        claimed: Rc::new(std::cell::Cell::new(false)),
    };
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.menu.context.row_is_file = true;
            shell.dispatch(Order::Menu(chosen), window, cx);
        })
    });
    assert_eq!(
        asked(&shell, cx),
        "/parent",
        "the explicit Open adopts its own folder"
    );
    assert!(
        launched().is_empty(),
        "the gallery resolves its directory order before launch"
    );
    super::test_shell::sent();
    land(
        &shell,
        cx,
        "/parent",
        vec![
            flea::protocol::RowFacts {
                name: "second.png".into(),
                ..image.clone()
            },
            image,
        ],
        2,
    );
    let requests = super::test_shell::sent();
    assert!(
        requests.iter().any(|line| line.contains(r#""c":"paths""#)
            && flea::json::field_usize(line, "images") == Some(1)),
        "{requests:?}"
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                crate::backend::Event::Paths(vec![
                    "/parent/second.png".into(),
                    "/parent/neighbour.png".into(),
                ]),
                window,
                cx,
            )
        })
    });
    cx.run_until_parked();
    let paths = launched();
    assert_eq!(paths.len(), 1);
    assert!(paths[0].contains("/parent/second.png") && paths[0].contains("/parent/neighbour.png"));
    quit(&shell, cx);
}
