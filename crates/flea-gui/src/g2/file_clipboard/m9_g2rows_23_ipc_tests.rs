//! The fileClipboard reader follows the live window selection through the production IPC dispatcher.
use super::{cut_then_paste, edit, key, open, reply, Handle};
use crate::{ipc::server::Event, window::startup::View};
use flea::{ipc::wire::Request, jsondoc};
use gpui::TestAppContext;
use std::sync::mpsc;

fn read(cx: &mut TestAppContext, handle: Handle) -> jsondoc::Json {
    let (answer, receive) = mpsc::sync_channel(1);
    edit(cx, handle, |shell, window, cx| {
        shell.ipc(
            Event::Call {
                request: Request::Call {
                    target: "flea".into(),
                    function: "fileClipboard".into(),
                    arguments: Vec::new(),
                },
                answer,
            },
            window,
            cx,
        );
    });
    let answer = receive
        .try_recv()
        .expect("the mounted reader answers synchronously");
    assert_eq!(answer.reply.code, 0, "{}", answer.reply.stderr);
    assert!(answer.reply.stderr.is_empty());
    assert!(!answer.quit);
    jsondoc::parse(&answer.reply.stdout).expect("fileClipboard is JSON")
}

#[gpui::test]
fn m9_g2rows_23_file_clipboard_reads_live_window_selection(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    lines();
    assert_eq!(
        read(cx, handle),
        jsondoc::parse(r#"{"paths":[],"moving":false,"token":""}"#).unwrap()
    );
    assert!(
        lines().is_empty(),
        "reading never asks the backend or claims ownership"
    );
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![0, 1];
    });
    key(cx, handle, "ctrl-x");
    reply(
        cx,
        handle,
        r#"{"t":"paths","paths":["/files/a\"b\nc.txt","/files/100%.txt"]}"#,
    );
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"set","ok":true,"token":"T1"}"#,
    );
    lines();
    let expected = jsondoc::parse(
        r#"{"paths":["/files/a\"b\nc.txt","/files/100%.txt"],"moving":true,"token":"T1"}"#,
    )
    .unwrap();
    for mode in [View::List, View::Columns, View::Grid, View::Dual] {
        edit(cx, handle, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().drawn_view = Some(mode)
        });
        assert_eq!(
            read(cx, handle),
            expected,
            "the window selection is shared in {mode:?}"
        );
    }
    assert!(lines().is_empty());
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"changed","clip":"copy","paths":["/other/new.txt"],"token":"T2"}"#,
    );
    assert_eq!(
        read(cx, handle),
        jsondoc::parse(r#"{"paths":["/other/new.txt"],"moving":false,"token":"T2"}"#).unwrap()
    );
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"changed","clip":"none","paths":[],"token":""}"#,
    );
    assert_eq!(
        read(cx, handle),
        jsondoc::parse(r#"{"paths":[],"moving":false,"token":""}"#).unwrap()
    );
    assert!(
        lines().is_empty(),
        "ownership loss and reads send no file operation"
    );
}

#[gpui::test]
fn m9_g2rows_23_clipboard_generation_and_cut_ack_are_isolated(cx: &mut TestAppContext) {
    let (handle, mut lines) = cut_then_paste(cx);
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 1
    });
    key(cx, handle, "ctrl-c");
    reply(cx, handle, r#"{"t":"paths","paths":["/files/b.txt"]}"#);
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"set","ok":true,"token":"T2"}"#,
    );
    lines();
    let expected =
        jsondoc::parse(r#"{"paths":["/files/b.txt"],"moving":false,"token":"T2"}"#).unwrap();
    reply(
        cx,
        handle,
        r#"{"t":"collisions","id":1,"total":0,"names":[],"mergeable":false}"#,
    );
    let sent = lines();
    assert_eq!(
        sent.iter()
            .filter(|line| *line == r#"{"c":"clipClear","token":"T1"}"#)
            .count(),
        1
    );
    assert_eq!(read(cx, handle), expected);
    reply(
        cx,
        handle,
        r#"{"t":"collisions","id":1,"total":0,"names":[],"mergeable":false}"#,
    );
    assert!(
        lines().is_empty(),
        "a repeated completion cannot spend the cut again"
    );
    assert_eq!(read(cx, handle), expected);
}

#[test]
fn m9_g2rows_23_uri_and_gnome_clipboard_roundtrip() {
    use flea::ui_logic_file_clipboard::{choose, Offer, FLEA, GNOME, KDE_CUT, URILIST};
    let paths = vec!["/files/a b#%.txt".into(), "/files/café\nreport.txt".into()];
    let token = "ab12cd34ab12cd34ab12cd34ab12cd34";
    for cut in [false, true] {
        let payloads = crate::g2::file_clipboard::seams::payloads(cut, &paths, token);
        let offers: Vec<_> = payloads
            .iter()
            .map(|payload| Offer {
                mime: payload.mime,
                bytes: &payload.bytes,
            })
            .collect();
        let selection = choose(&offers).unwrap();
        assert_eq!(selection.paths, paths);
        assert_eq!(selection.clip, if cut { "cut" } else { "copy" });
        assert_eq!(selection.token, token);
        let without_gnome: Vec<_> = offers
            .iter()
            .filter(|offer| offer.mime != GNOME)
            .map(|offer| Offer {
                mime: offer.mime,
                bytes: offer.bytes,
            })
            .collect();
        assert_eq!(
            choose(&without_gnome),
            Some(selection.clone()),
            "a combined read alone cannot prove GNOME interoperability"
        );
        let gnome = payloads
            .iter()
            .find(|payload| payload.mime == GNOME)
            .expect("GNOME files must be offered");
        assert_eq!(
            gnome.bytes,
            format!(
                "{}\nfile:///files/a%20b%23%25.txt\nfile:///files/caf%C3%A9%0Areport.txt",
                if cut { "cut" } else { "copy" }
            )
            .as_bytes()
        );
        let gnome_only = choose(&[Offer {
            mime: GNOME,
            bytes: &gnome.bytes,
        }])
        .unwrap();
        assert_eq!(gnome_only.paths, paths);
        assert_eq!(gnome_only.clip, selection.clip);
        assert!(gnome_only.token.is_empty());
        let uri = payloads
            .iter()
            .find(|payload| payload.mime == URILIST)
            .unwrap();
        assert_eq!(
            uri.bytes,
            b"file:///files/a%20b%23%25.txt\r\nfile:///files/caf%C3%A9%0Areport.txt\r\n"
        );
        let uri_only: Vec<_> = offers
            .into_iter()
            .filter(|offer| offer.mime != GNOME && offer.mime != FLEA)
            .collect();
        let fallback = choose(&uri_only).unwrap();
        assert_eq!(fallback.paths, paths);
        assert_eq!(fallback.clip, selection.clip);
        assert!(fallback.token.is_empty());
        assert_eq!(payloads.iter().any(|payload| payload.mime == KDE_CUT), cut);
    }
}
