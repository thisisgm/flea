//! Non-default states travel through the normal window's mounted readers.
use super::*;
use flea::{
    jsondoc::{parse, Json},
    ui_logic_fb1::status::Hint,
    ui_logic_op4::{output::Kind, selection::Source},
};

pub(super) fn network(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    update(cx, handle, |shell, window, cx| {
        shell.rail = Some(crate::window::rail::mount(
            shell.locale.catalog.clone(),
            &shell.launch.settings.data,
            shell.focus_side,
            cx,
        ));
        shell.ensure_network(window, cx);
    });
}

fn convert(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    update(cx, handle, |shell, window, cx| {
        assert!(shell.open_output(
            0,
            crate::g2::op4::model::Action::Convert,
            0,
            vec![Source {
                path: "/files/photo.png".into(),
                kind: Kind::Image
            }],
            window,
            cx
        ));
    });
}

#[gpui::test]
fn ipc_archive_formats_preserve_backend_order_and_focused_side(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    update(cx, handle, |shell, window, cx| {
        assert!(shell.archive_line(
            0,
            r#"{"t":"formats","archive":["zip","7z","tar"]}"#,
            window,
            cx
        ));
    });
    value(cx, handle, "archiveFormats", "zip|7z|tar");
    update(cx, handle, |shell, _, _| {
        shell.operations.archive.formats[0].archive.reverse();
    });
    value(cx, handle, "archiveFormats", "tar|7z|zip");
    let (right, _right_backend) = Backend::test_channel();
    update(cx, handle, |shell, window, cx| {
        shell.panes[1] = shell_with(right, cx).panes[0].take();
        shell.focus_side = 1;
        assert!(shell.archive_line(1, r#"{"t":"formats","archive":["rar","tar"]}"#, window, cx));
    });
    value(cx, handle, "archiveFormats", "rar|tar");
}

#[gpui::test]
fn ipc_convert_format_reads_the_active_draft_and_refuses_discarded_state(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    convert(cx, handle);
    value(cx, handle, "convertFormat", "jpg");
    update(cx, handle, |shell, _, cx| {
        shell
            .output
            .view
            .as_ref()
            .unwrap()
            .clone()
            .update(cx, |view, _| view.model.choose_format(2, &view.catalog));
    });
    value(cx, handle, "convertFormat", "webp");
    update(cx, handle, |shell, _, cx| {
        shell
            .output
            .view
            .as_ref()
            .unwrap()
            .clone()
            .update(cx, |view, _| {
                assert!(view.model.close());
            });
    });
    blocked(cx, handle, "convertFormat");
}

#[gpui::test]
fn ipc_convert_open_follows_open_and_close(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    convert(cx, handle);
    value(cx, handle, "convertOpen", "true");
    update(cx, handle, |shell, _, cx| {
        shell
            .output
            .view
            .as_ref()
            .unwrap()
            .clone()
            .update(cx, |view, _| {
                assert!(view.model.close());
            });
    });
    value(cx, handle, "convertOpen", "false");
}

#[gpui::test]
fn ipc_convert_strip_reads_the_same_toggle_as_the_card(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    convert(cx, handle);
    value(cx, handle, "convertStrip", "false");
    update(cx, handle, |shell, _, cx| {
        shell
            .output
            .view
            .as_ref()
            .unwrap()
            .clone()
            .update(cx, |view, _| view.model.toggle_check(&view.catalog));
    });
    value(cx, handle, "convertStrip", "true");
    update(cx, handle, |shell, _, cx| {
        shell
            .output
            .view
            .as_ref()
            .unwrap()
            .clone()
            .update(cx, |view, _| {
                assert!(view.model.close());
            });
    });
    blocked(cx, handle, "convertStrip");
}

#[gpui::test]
fn ipc_convert_capability_reads_the_backend_reply(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    update(cx, handle, |shell, _, _| {
        shell.record_formats(0, &parse(r#"{"convert":true}"#).unwrap())
    });
    value(cx, handle, "canConvert", "true");
    update(cx, handle, |shell, _, _| {
        shell.record_formats(0, &parse(r#"{"convert":false}"#).unwrap())
    });
    value(cx, handle, "canConvert", "false");
}

#[gpui::test]
fn ipc_convert_state_refuses_unmeasured_painted_fields(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    convert(cx, handle);
    blocked(cx, handle, "convertState");
}

#[gpui::test]
fn ipc_dialog_open_reads_the_network_card(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    network(cx, handle);
    update(cx, handle, |shell, window, cx| {
        shell.network_add(0, window, cx)
    });
    value(cx, handle, "dialogOpen", "true");
}

#[gpui::test]
fn ipc_share_open_reads_the_network_browser_the_window_mounts(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    network(cx, handle);
    update(cx, handle, |shell, window, cx| {
        let shares = shell
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .shares
            .clone();
        shares.update(cx, |shares, cx| {
            shares.show(
                "smb://nas/".into(),
                "NAS".into(),
                vec!["media".into(), "docs".into()],
                1,
                window,
                cx,
            )
        });
    });
    let focused = |cx: &mut TestAppContext| {
        // Sample shareBrowserState: {"active":true,"focused":true,"owner":-1,"rect":"","baseUri":"","cursor":-1,"paneRects":[]}.
        parse(&read(cx, handle, "shareBrowserState").stdout)
            .unwrap()
            .get("focused")
            .and_then(Json::as_bool)
    };
    value(cx, handle, "shareBrowserOpen", "true");
    value(cx, handle, "shareBrowserEntries", "media\ndocs");
    assert_eq!(focused(cx), Some(true));
    update(cx, handle, |shell, window, cx| {
        window.focus(&shell.focus, cx)
    });
    assert_eq!(focused(cx), Some(false));
    value(cx, handle, "shareBrowserOpen", "true");
    update(cx, handle, |shell, _, cx| {
        let shares = shell
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .shares
            .clone();
        shares.update(cx, |shares, cx| shares.close(cx));
    });
    value(cx, handle, "shareBrowserOpen", "false");
    value(cx, handle, "shareBrowserEntries", "media\ndocs");
}

#[gpui::test]
fn ipc_status_secondary_retains_hint_separators_and_queued_activities(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    update(cx, handle, |shell, _, cx| {
        shell.status_messages(cx, |messages, now| {
            messages.set_activity(0, "First", 1, false, false, now);
            messages.set_activity(1, "Second", 2, true, false, now);
            messages.say("Failed", true, "detail", Hint::None, now)
        });
    });
    value(
        cx,
        handle,
        "statusSecondary",
        " · esc dismisses · First · Second",
    );
    update(cx, handle, |shell, _, cx| {
        shell.status_messages(cx, |messages, now| {
            messages.set_activity(0, "First", 1, true, false, now);
        });
    });
    value(cx, handle, "statusSecondary", " · esc dismisses · Second");
}

#[gpui::test]
fn ipc_status_secondary_includes_the_split_undo_link_before_extras(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    update(cx, handle, |shell, _, cx| {
        shell.status_messages(cx, |messages, now| {
            messages.say("Done", false, "", Hint::Undo, now);
            messages.set_retry("Try again");
        });
    });
    value(cx, handle, "statusSecondary", " · z undoes · Try again");
    update(cx, handle, |shell, _, cx| {
        shell.status_messages(cx, |messages, now| {
            messages.say(
                "Done",
                false,
                "",
                Hint::Note(flea::ui_logic_fb1::status::Note::Background, true),
                now,
            );
        });
    });
    value(
        cx,
        handle,
        "statusSecondary",
        " · rclone uploads them in the background · z undoes · Try again",
    );
    update(cx, handle, |shell, _, cx| {
        shell.status_messages(cx, |messages, now| {
            messages.say("Failed", true, "", Hint::Undo, now)
        });
    });
    value(cx, handle, "statusSecondary", " · esc dismisses · z undoes");
}

#[gpui::test]
fn ipc_flash_state_reports_open_and_running_models_without_starting_a_process(
    cx: &mut TestAppContext,
) {
    let (handle, _backend) = open(cx);
    update(cx, handle, |shell, _, _| {
        shell
            .operations
            .archive
            .flash
            .open_with_image("/files/quoted\".iso");
    });
    value(
        cx,
        handle,
        "flashState",
        r#"{"opened":true,"cursor":0,"image":"/files/quoted\".iso","drives":0,"selected":-1,"live":false,"running":false,"phase":"","ending":""}"#,
    );
    update(cx, handle, |shell, _, _| {
        shell
            .operations
            .archive
            .flash
            .start_transfer(&shell.locale.catalog, 7, 1024, true, 0);
    });
    value(
        cx,
        handle,
        "flashState",
        r#"{"opened":true,"cursor":1,"image":"/files/quoted\".iso","drives":0,"selected":-1,"live":false,"running":true,"phase":"prepare","ending":""}"#,
    );
}

#[gpui::test]
fn ipc_archive_cards_answer_the_open_extract_card(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    update(cx, handle, |shell, _, _| {
        shell.operations.archive.extract.open(
            "/files/private.7z",
            "/files/out",
            "",
            &shell.locale.catalog,
        );
    });
    let reply = read(cx, handle, "archiveCardsState");
    assert_eq!(reply.code, 0, "{reply:?}");
    assert!(
        reply.stdout.contains("\"extractOpened\":true"),
        "{}",
        reply.stdout
    );
}

#[gpui::test]
fn uish_misc_convert_title_centre_reads_the_painted_title_inside_the_card(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    convert(cx, handle);
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    cx.run_until_parked();
    let numbers = |name: &str, cx: &mut TestAppContext| -> Vec<i64> {
        let reply = read(cx, handle, name);
        assert_eq!(reply.code, 0, "{name}: {}", reply.stderr);
        reply
            .stdout
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect()
    };
    let painted = gpui::VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds("op4-title")
        .expect("the title has an independently observed painted box");
    let card = numbers("convertCardRect", cx);
    let title = numbers("convertTitleCentre", cx);
    assert_eq!(
        title,
        vec![
            f32::from(painted.center().x).round() as i64,
            f32::from(painted.center().y).round() as i64,
        ],
        "the reader must name the painted title, not the containing card"
    );
    assert_eq!((card.len(), title.len()), (4, 2), "{card:?} {title:?}");
    assert!(
        title[0] > card[0]
            && title[0] < card[0] + card[2]
            && title[1] > card[1]
            && title[1] < card[1] + card[3],
        "title {title:?} outside card {card:?}"
    );
}
