//! Mounted sidebar navigation must replace both the pane and the painted rows.
use super::test_shell::{open_root_at, row, sent};
use crate::{state::Shell, window::navigate::Step};
use flea::ui_logic_rail_places::rail::{Group, Kind, RailEntry};
use gpui::{point, px, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};

const HOME: &str = "/fixture/home";
const SHARE: &str = "/run/user/1000/gvfs/smb-share:server=host,share=docs";

fn frame(cx: &mut VisualTestContext) {
    cx.simulate_frame_request(gpui::RequestFrameOptions {
        require_presentation: true,
        ..Default::default()
    });
    cx.run_until_parked();
    let id = cx.update(|window, _| window.presented_frames().unwrap().committed);
    if id > 0 {
        cx.simulate_frame_terminal(gpui::FrameTerminal {
            frame_id: id,
            outcome: gpui::FrameOutcome::Presented(gpui::PresentationTime {
                timestamp: None,
                clock_id: None,
            }),
        });
        cx.run_until_parked();
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = open_root_at(cx, HOME);
    cx.enable_frame_feedback();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.presentation.close();
            shell.follow_frame_feedback(window, cx);
            window.activate_window();
            cx.notify();
        })
    });
    frame(cx);
    (shell, cx)
}

fn land(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    path: &str,
    rows: Vec<flea::protocol::RowFacts>,
    listing: usize,
) {
    use flea::protocol::Reply;
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            for reply in [
                Reply::Listed {
                    path: path.into(),
                    total: rows.len(),
                },
                Reply::Rows {
                    start: 0,
                    rows,
                    listing,
                },
            ] {
                shell.receive(0, crate::backend::Event::Reply(reply), window, cx);
            }
        })
    });
    for _ in 0..4 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(400));
        cx.run_until_parked();
        frame(cx);
    }
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().path.clone()),
        path
    );
}

fn click(shell: &Entity<Shell>, cx: &mut VisualTestContext, entry: RailEntry) {
    if entry.group == Group::Network {
        shell.update(cx, |shell, cx| {
            shell.network_publish(
                Some(vec![flea::ui_logic_device_rail::RailEntry {
                    path: entry.path.clone(),
                    uri: entry.uri.clone(),
                    label: entry.label.clone(),
                    group: flea::ui_logic_device_rail::Group::Network,
                    kind: flea::ui_logic_device_rail::Kind::Share,
                    mounted: true,
                    ..Default::default()
                }]),
                cx,
            )
        });
    }
    super::railrecent_tests::rows(shell, cx, vec![entry]);
    let at = shell.read_with(cx, |shell, cx| {
        let rail = shell.rail.as_ref().unwrap().places.read(cx).laid.borrow();
        let rect = rail.row(0).expect("the sidebar row was painted").rect;
        point(px(rect.x + 45.), px(rect.y + rect.height / 2.))
    });
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    frame(cx);
}

fn place(path: &str) -> RailEntry {
    RailEntry {
        label: path.rsplit('/').next().unwrap_or(path).into(),
        path: path.into(),
        glyph: "house",
        kind: Kind::Home,
        ..RailEntry::default()
    }
}

fn nas() -> RailEntry {
    RailEntry {
        label: "NAS".into(),
        path: SHARE.into(),
        uri: "smb://host/docs".into(),
        mounted: Some(true),
        group: Group::Network,
        kind: Kind::Share,
        ..RailEntry::default()
    }
}

fn assert_rows(shell: &Entity<Shell>, cx: &mut VisualTestContext, name: &str) {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.pane.held.rows[0].name, name,
            "the pane follows navigation"
        );
        assert_eq!(
            host.list.borrow().drawn_rows.rows[0].name,
            name,
            "the painted listing follows navigation"
        );
    });
}

#[gpui::test]
fn mounted_sidebar_navigation_replaces_rows_for_every_place(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    land(&shell, cx, HOME, vec![row("home.txt", false)], 1);
    click(&shell, cx, nas());
    assert!(sent().iter().any(|line| line.contains(SHARE)));
    land(&shell, cx, SHARE, vec![row("appdata", true)], 2);
    super::fileops_tests::reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"fsinfo","path":"{SHARE}","fs":"fuse","free":100,"class":"network","tagable":false}}"#
        ),
    );
    assert_rows(&shell, cx, "appdata");
    for (index, folder) in [
        "Pictures",
        "Music",
        "",
        "Downloads",
        "Documents",
        "Videos",
        "Projects",
    ]
    .iter()
    .enumerate()
    {
        let path = if folder.is_empty() {
            HOME.into()
        } else {
            format!("{HOME}/{folder}")
        };
        click(&shell, cx, place(&path));
        let name = format!("local-{index}.txt");
        land(&shell, cx, &path, vec![row(&name, false)], index + 3);
        assert_rows(&shell, cx, &name);
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn mounted_navigation_keeps_folder_history_and_typed_paths_working(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    land(&shell, cx, HOME, vec![row("home.txt", false)], 1);
    click(&shell, cx, nas());
    land(&shell, cx, SHARE, vec![row("appdata", true)], 2);
    super::fileops_tests::reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"fsinfo","path":"{SHARE}","fs":"fuse","free":100,"class":"network","tagable":false}}"#
        ),
    );
    cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    cx.simulate_keystrokes("enter");
    land(
        &shell,
        cx,
        &format!("{SHARE}/appdata"),
        vec![row("inside.txt", false)],
        3,
    );
    cx.simulate_keystrokes("backspace");
    land(&shell, cx, SHARE, vec![row("appdata", true)], 4);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.step(0, Step::Back, window, cx).unwrap();
        })
    });
    land(
        &shell,
        cx,
        &format!("{SHARE}/appdata"),
        vec![row("inside.txt", false)],
        5,
    );
    cx.simulate_keystrokes("ctrl-l ctrl-a");
    cx.simulate_input(HOME);
    cx.simulate_keystrokes("enter");
    land(&shell, cx, HOME, vec![row("home.txt", false)], 6);
    assert_rows(&shell, cx, "home.txt");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
