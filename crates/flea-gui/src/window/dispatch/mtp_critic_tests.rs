//! The busy-folder board owns the requested place and its unavailable count.
use super::{
    land,
    mtp_reload_tests::{notice, wait},
    row, set_cursor,
};
use crate::{
    g2::status_feedback::caption::INKED,
    state::Shell,
    window::{navigate::Step, startup::LISTING_WAIT_MS, u07_tests::read},
};
use gpui::{px, size, Entity, Modifiers, TestAppContext, VisualTestContext};
use std::time::Duration;

#[gpui::test]
fn mtp_critic_waiting_disk_does_not_borrow_the_previous_folders_space(cx: &mut TestAppContext) {
    let (shell, cx) = super::open(cx, "default");
    land(&shell, cx, "/files", vec![row("phone", true)], 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                crate::backend::Event::Filesystem(crate::backend::status::Filesystem {
                    path: "/files".into(),
                    name: "old-filesystem".into(),
                    free: Some(1_000_000),
                    class: String::new(),
                    tagable: None,
                }),
                window,
                cx,
            );
            assert_eq!(
                shell.step(0, Step::Open("/files/phone".into()), window, cx),
                Ok(true)
            );
        })
    });
    cx.executor()
        .advance_clock(Duration::from_millis(LISTING_WAIT_MS + 1));
    cx.run_until_parked();
    draw(cx);
    let footer = flea::jsondoc::parse(&read(&shell, cx, "statusFooterState", &[])).unwrap();
    assert_eq!(
        footer
            .get("filesystem")
            .and_then(flea::jsondoc::Json::as_str),
        Some("unknown"),
        "F2: the previous filesystem cannot supply the requested folder's free space"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn pending(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = super::open(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("phone", true), row("keep.txt", false)],
        1,
    );
    set_cursor(&shell, cx, 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(
                shell.step(0, Step::Open("/files/phone".into()), window, cx),
                Ok(true)
            );
        })
    });
    assert_eq!(
        read(&shell, cx, "path", &[]),
        "/files",
        "the hold keeps its old place"
    );
    cx.executor()
        .advance_clock(Duration::from_millis(LISTING_WAIT_MS + 1));
    cx.run_until_parked();
    draw(cx);
    (shell, cx)
}

#[gpui::test]
fn mtp_critic_waiting_count_is_unavailable(cx: &mut TestAppContext) {
    let (shell, cx) = pending(cx);
    let count = shell.read_with(cx, |shell, cx| {
        shell.surfaces[0]
            .footer
            .as_ref()
            .unwrap()
            .read(cx)
            .counts
            .to_string()
    });
    assert_eq!(
        count, "unavailable",
        "F1: Waiting uses the Error count recipe"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn mtp_critic_waiting_place_reaches_every_path_owner(cx: &mut TestAppContext) {
    let (shell, cx) = pending(cx);
    let paths = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        vec![
            host.pane.path().to_owned(),
            host.path.clone(),
            shell.path_labels[0].to_string(),
            host.pane.state.target.clone(),
        ]
    });
    let mut paths = paths;
    paths.push(read(&shell, cx, "path", &[]));
    paths.push(read(&shell, cx, "crumbText", &[]));
    assert_eq!(
        paths,
        vec!["/files/phone"; 6],
        "F2: all folder readers must name the requested place"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn mtp_critic_waiting_back_restores_the_old_cursor_without_duplicate_history(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = pending(cx);
    let before = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().nav.history.back.clone()
    });
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].name, "keep.txt");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(shell.step(0, Step::Back, window, cx), Ok(true));
        })
    });
    land(
        &shell,
        cx,
        "/files",
        vec![row("phone", true), row("keep.txt", false)],
        2,
    );
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.path, "/files");
        assert_eq!(host.pane.cursor, 1);
        assert!(host.nav.history.back.is_empty());
        assert_eq!(host.nav.history.forward[0].path, "/files/phone");
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn mtp_critic_waiting_answer_keeps_one_history_entry_and_write_keys_send_nothing(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = pending(cx);
    let _ = super::sent();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            for action in ["newFolder", "newFile", "paste"] {
                assert_eq!(
                    shell.run_key_action(action, Default::default(), false, false, window, cx),
                    crate::window::input::KeyRun::Busy,
                    "{action} cannot write in the held folder"
                );
            }
        })
    });
    assert!(super::sent().is_empty());
    land(&shell, cx, "/files/phone", vec![row("photo.jpg", false)], 2);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.path, "/files/phone");
        assert_eq!(host.nav.history.back.len(), 1);
        assert_eq!(host.nav.history.back[0].path, "/files");
        assert_eq!(host.nav.history.back[0].name, "keep.txt");
        assert!(host.nav.history.forward.is_empty());
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn mtp_critic_waiting_place_changes_only_its_dual_pane(cx: &mut TestAppContext) {
    let (shell, cx) = super::open_with(cx, r#""keys":"default","view":"dual""#, None);
    for side in 0..2 {
        super::land_on(
            &shell,
            cx,
            side,
            "/files",
            vec![row("keep.txt", false)],
            1,
            1,
        );
    }
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(
                shell.step(0, Step::Open("/phone".into()), window, cx),
                Ok(true)
            );
        })
    });
    cx.executor()
        .advance_clock(Duration::from_millis(LISTING_WAIT_MS + 1));
    cx.run_until_parked();
    draw(cx);
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().path, "/phone");
        assert_eq!(shell.path_labels[0].as_ref(), "/phone");
        let other = shell.panes[1].as_ref().unwrap();
        assert_eq!(other.path, "/files");
        assert_eq!(other.pane.path(), "/files");
        assert_eq!(shell.path_labels[1].as_ref(), "/files");
        assert_eq!(other.pane.held.total, 1);
        assert!(other.nav.history.back.is_empty());
        assert!(other.nav.history.forward.is_empty());
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// C2: the Reload label shares its frame's centre at every scale and window height, so it sits alike in every view.
fn assert_label_centred(cx: &mut VisualTestContext, panes: usize) {
    let width = cx.update(|window, _| window.viewport_size().width);
    // Scales and window heights that put the centred group on whole, half and odd fractions of a pixel.
    for scale in [1., 2.] {
        cx.simulate_scale_factor_change(scale);
        for height in [600., 600.25, 600.5, 601.3, 601.75] {
            cx.simulate_resize(size(width, px(height)));
            label_centred_at(cx, panes);
        }
    }
}

fn label_centred_at(cx: &mut VisualTestContext, panes: usize) {
    INKED.with_borrow_mut(|inked| *inked = Some(Vec::new()));
    draw(cx);
    let inked = INKED.with_borrow_mut(|inked| inked.take().unwrap());
    let labels: Vec<_> = inked
        .into_iter()
        .filter(|(text, ..)| text.as_ref() == "Reload")
        .map(|(.., label)| label)
        .collect();
    assert_eq!(labels.len(), panes, "one Reload label per waiting pane");
    cx.update(|window, _| {
        let scale = window.scale_factor();
        for label in labels {
            let (x, y) = (
                f32::from(label.center().x) * scale,
                f32::from(label.center().y) * scale,
            );
            let frame = window
                .painted_quads()
                .iter()
                .filter(|quad| quad.border_widths.top.0 > 0.)
                .map(|quad| quad.bounds)
                .filter(|frame| {
                    frame.origin.x.0 <= x
                        && x <= frame.origin.x.0 + frame.size.width.0
                        && frame.origin.y.0 <= y
                        && y <= frame.origin.y.0 + frame.size.height.0
                })
                .min_by(|a, b| a.size.height.0.total_cmp(&b.size.height.0))
                .expect("the label sits in its frame");
            let label = f32::from(label.center().y) * scale;
            let frame = frame.origin.y.0 + frame.size.height.0 / 2.;
            assert!(
                (label - frame).abs() < 0.01,
                "label centre {label} dp, frame centre {frame} dp, at scale {scale} and height {}",
                f32::from(window.viewport_size().height)
            );
        }
    });
}

#[gpui::test]
fn mtp_critic_reload_label_offset_list(cx: &mut TestAppContext) {
    let (shell, cx) = pending(cx);
    assert_label_centred(cx, 1);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn mtp_critic_reload_label_offset_grid(cx: &mut TestAppContext) {
    let (shell, cx) = super::open_with(cx, r#""keys":"default","view":"grid""#, None);
    cx.executor()
        .advance_clock(Duration::from_millis(LISTING_WAIT_MS + 1));
    cx.run_until_parked();
    assert_label_centred(cx, 1);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn mtp_critic_reload_label_offset_dual(cx: &mut TestAppContext) {
    use super::{land_on, open_with};
    let (shell, cx) = open_with(cx, r#""keys":"default","view":"dual""#, None);
    for side in 0..2 {
        land_on(&shell, cx, side, "/files", vec![row("phone", true)], 1, 1);
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                let _ = shell.step(
                    side,
                    crate::window::navigate::Step::Open(format!("/phone-{side}")),
                    window,
                    cx,
                );
            })
        });
    }
    cx.executor()
        .advance_clock(Duration::from_millis(LISTING_WAIT_MS + 1));
    cx.run_until_parked();
    assert_label_centred(cx, 2);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn mtp_reload_of_a_listed_folder_that_waits_keeps_the_change_count(cx: &mut TestAppContext) {
    let (shell, cx) = super::open(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("phone", true), row("local", true)],
        1,
    );
    cx.simulate_keystrokes("f5");
    wait(cx);
    let at = cx.debug_bounds("fb1-reload").unwrap().center();
    cx.simulate_click(at, Modifiers::none());
    draw(cx);
    assert_eq!(super::asked(&shell, cx), "/files");
    land(&shell, cx, "/files", vec![row("phone", true)], 2);
    assert_eq!(notice(&shell, cx), "Reloaded · 1 item changed");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
