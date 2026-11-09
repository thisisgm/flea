//! A server's share browser gives its pane back when navigation leaves it.
use super::*;
use crate::window::{dispatch::test_shell::land, navigate::Step};
use flea::ui_logic_rail_places::{
    order::{Heading, RailSection},
    rail::RailEntry as Place,
};
use gpui::{point, px, Modifiers, MouseButton};

const HOME: &str = "/fixture/work";
const SERVER: &str = "smb://nas/";

fn row(name: &str, dir: bool) -> flea::protocol::RowFacts {
    flea::protocol::RowFacts {
        name: name.into(),
        dir,
        ..Default::default()
    }
}

fn click(shell: &Entity<Shell>, cx: &mut VisualTestContext, path: &str) {
    if path != SERVER {
        places(shell, cx);
        frame(cx);
    }
    let at = shell.read_with(cx, |shell, cx| {
        let producer = shell.rail.as_ref().unwrap().places.read(cx);
        let index = producer
            .entries()
            .iter()
            .position(|entry| entry.path == path || entry.uri == path)
            .expect("the sidebar entry exists");
        let laid = producer.laid.borrow();
        let rect = laid.row(index).expect("the sidebar entry was painted").rect;
        point(px(rect.x + 45.), px(rect.y + rect.height / 2.))
    });
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    frame(cx);
}

fn reply(recorder: &Recorder, command: &str, code: i32, text: &str, cx: &mut VisualTestContext) {
    let at = recorder
        .jobs
        .borrow()
        .iter()
        .rposition(|job| {
            job.argv.get(1).map(String::as_str) == Some(command)
                && job.argv.last().map(String::as_str) == Some(SERVER)
        })
        .expect("the service issued its GIO request");
    recorder.answer(at, code, text);
    cx.run_until_parked();
}

fn places(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    shell.update(cx, |shell, cx| {
        let producer = shell.rail.as_ref().unwrap().places.clone();
        producer.update(cx, |producer, cx| {
            producer.replace_sections(
                vec![RailSection {
                    heading: Heading::Key("places.heading.places"),
                    rows: [
                        "Pictures",
                        "Music",
                        "",
                        "Downloads",
                        "Documents",
                        "Videos",
                        "Projects",
                    ]
                    .iter()
                    .map(|name| Place {
                        path: if name.is_empty() {
                            HOME.into()
                        } else {
                            format!("{HOME}/{name}")
                        },
                        label: if name.is_empty() {
                            "Home".into()
                        } else {
                            (*name).into()
                        },
                        glyph: "house",
                        ..Default::default()
                    })
                    .collect(),
                }],
                cx,
            )
        });
    });
}

fn show_server(shell: &Entity<Shell>, recorder: &Recorder, cx: &mut VisualTestContext) {
    shell.update(cx, |shell, cx| {
        shell.network_publish(
            Some(vec![RailEntry {
                uri: SERVER.into(),
                label: "NAS".into(),
                group: flea::ui_logic_device_rail::Group::Network,
                kind: flea::ui_logic_device_rail::Kind::Share,
                glyph: "server".into(),
                ..Default::default()
            }]),
            cx,
        );
    });
    frame(cx);
    click(shell, cx, SERVER);
    reply(recorder, "mount", 0, "", cx);
    reply(recorder, "info", 1, "", cx);
    reply(
        recorder,
        "list",
        0,
        "appdata\nbackups\ndata\nisos\ntimemachine\n",
        cx,
    );
    // Network publication rebuilds the rail; install the fixture's local rows after it settles.
    places(shell, cx);
    frame(cx);
    assert!(browser_open(shell, cx));
    assert!(
        cx.debug_bounds("share-motion").is_some(),
        "the server's actual share browser is drawn"
    );
}

fn browser_open(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    shell.read_with(cx, |shell, cx| {
        shell
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .shares
            .read(cx)
            .model
            .open
    })
}

#[gpui::test]
fn server_browser_sidebar_navigation_shows_every_places_listing(cx: &mut TestAppContext) {
    for reopen in [false, true] {
        let recorder = Rc::new(Recorder::default());
        let (shell, cx, _lines) = super::open_focus_tests::root(recorder.clone(), cx);
        land(&shell, cx, HOME, vec![row("home.txt", false)], 1);
        if !reopen {
            show_server(&shell, &recorder, cx);
        }
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
            if reopen {
                show_server(&shell, &recorder, cx);
            }
            let path = if folder.is_empty() {
                HOME.into()
            } else {
                format!("{HOME}/{folder}")
            };
            click(&shell, cx, &path);
            assert!(
                !browser_open(&shell, cx),
                "the server's rows must not cover {path}"
            );
            let name = format!("local-{index}.txt");
            land(&shell, cx, &path, vec![row(&name, false)], index + 2);
            shell.read_with(cx, |shell, _| {
                let host = shell.panes[0].as_ref().unwrap();
                assert_eq!(host.pane.held.rows[0].name, name);
                assert_eq!(host.list.borrow().drawn_rows.rows[0].name, name);
            });
            assert!(cx.debug_bounds("share-motion").is_none());
        }
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}

#[gpui::test]
fn server_browser_same_folder_navigation_dismisses_the_cover(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = super::open_focus_tests::root(recorder.clone(), cx);
    land(&shell, cx, HOME, vec![row("home.txt", false)], 1);
    show_server(&shell, &recorder, cx);
    click(&shell, cx, HOME);
    assert!(!browser_open(&shell, cx));
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().pane.held.rows[0].name,
            "home.txt"
        )
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn server_browser_navigation_only_dismisses_its_own_panes_cover(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = super::open_focus_tests::root(recorder.clone(), cx);
    land(&shell, cx, HOME, vec![row("home.txt", false)], 1);
    show_server(&shell, &recorder, cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell
                .step(1, Step::OpenPlace("/fixture/other".into()), window, cx)
                .unwrap();
        })
    });
    assert!(
        browser_open(&shell, cx),
        "navigation in the other pane preserves this browser"
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell
                .step(0, Step::Open("/fixture/local".into()), window, cx)
                .unwrap();
        })
    });
    assert!(
        !browser_open(&shell, cx),
        "typed-path navigation in the owning pane closes its browser"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn server_browser_back_and_up_keep_their_navigation_destination(cx: &mut TestAppContext) {
    for step in [Step::Back, Step::MouseBack, Step::Up] {
        let recorder = Rc::new(Recorder::default());
        let (shell, cx, _lines) = super::open_focus_tests::root(recorder.clone(), cx);
        land(&shell, cx, HOME, vec![row("home.txt", false)], 1);
        let pictures = format!("{HOME}/Pictures");
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell
                    .step(0, Step::Open(pictures.clone()), window, cx)
                    .unwrap();
            })
        });
        land(&shell, cx, &pictures, vec![row("photo.png", false)], 2);
        show_server(&shell, &recorder, cx);
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                assert!(shell.step(0, step.clone(), window, cx).unwrap());
            })
        });
        assert!(
            !browser_open(&shell, cx),
            "{step:?} closes the server browser"
        );
        assert_eq!(
            shell.read_with(cx, |shell, _| shell.panes[0]
                .as_ref()
                .unwrap()
                .pane
                .place()
                .to_owned()),
            HOME,
            "closing the browser preserves the requested navigation"
        );
        land(&shell, cx, HOME, vec![row("home.txt", false)], 3);
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}
