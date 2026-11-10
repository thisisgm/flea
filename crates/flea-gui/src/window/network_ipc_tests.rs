//! The network card and NETWORK rail readers answer through the live shell's IPC dispatch, read from the painted card.
use super::*;
use crate::g2::network_shares::view::NetworkView;
use crate::ipc::server;
use flea::ipc::wire;
use gpui::{point, px, size, Modifiers};

#[path = "network_cohesion_tests.rs"]
mod cohesion;

fn call(shell: &Entity<Shell>, name: &str, args: &[&str], cx: &mut VisualTestContext) -> String {
    let (answer, receiver) = std::sync::mpsc::sync_channel(1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.ipc(
                server::Event::Call {
                    request: wire::Request::Call {
                        target: "flea".into(),
                        function: name.into(),
                        arguments: args.iter().map(|v| (*v).into()).collect(),
                    },
                    answer,
                },
                window,
                cx,
            )
        })
    });
    let reply = receiver.try_recv().expect("a synchronous IPC answer").reply;
    assert_eq!(reply.code, 0, "{name} is served, not refused: {reply:?}");
    reply.stdout.trim_end_matches('\n').to_string()
}

fn numbers(text: &str) -> Vec<f32> {
    text.split([' ', '|'])
        .map(|value| value.parse::<f32>().expect("a number"))
        .collect()
}

fn card(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Entity<NetworkView> {
    cx.update(|_, cx| {
        let network = shell.read(cx).rail.as_ref().unwrap().network.as_ref();
        network
            .expect("the first frame built the network join")
            .card
            .clone()
    })
}

fn click(centre: &str, cx: &mut VisualTestContext) {
    let at = numbers(centre);
    cx.simulate_click(point(px(at[0]), px(at[1])), Modifiers::none());
    frame(cx);
}

#[gpui::test]
fn smbpath_incomplete_address_opens_a_failed_card_from_the_path_bar(cx: &mut TestAppContext) {
    for uri in ["smb://", "smb:///share", "sftp://", "ftp://"] {
        let recorder = Rc::new(Recorder::default());
        let (shell, cx, mut lines) = open(recorder.clone(), cx);
        cx.simulate_resize(size(px(1100.), px(700.)));
        frame(cx);
        drain(&mut lines[0]);
        drain(&mut lines[1]);
        cx.simulate_keystrokes("ctrl-l ctrl-a");
        cx.simulate_input(uri);
        cx.simulate_keystrokes("enter");
        frame(cx);
        if let Some(at) = recorder.mounts().last() {
            recorder.answer(*at, 1, "");
            cx.run_until_parked();
            let at = *recorder
                .find("info")
                .last()
                .expect("the mount asks for its folder");
            recorder.answer(at, 1, "");
            frame(cx);
        }
        assert!(
            !call(&shell, "networkCardRect", &[], cx).is_empty(),
            "{uri}: the failed address opens its repair card"
        );
        let expected = shell.read_with(cx, |shell, _| shell.locale.catalog.tr("network.refused"));
        assert_eq!(call(&shell, "networkStatus", &[], cx), expected, "{uri}");
        assert_eq!(call(&shell, "networkResult", &[], cx), "failed", "{uri}");
        assert_eq!(call(&shell, "pathBarOpen", &[], cx), "false");
        assert_eq!(
            call(&shell, "statusPrimary", &[], cx),
            expected,
            "{uri}: the footer repeats the failure"
        );
        assert_eq!(call(&shell, "statusError", &[], cx), "true");
        assert_eq!(recorder.mounts().len(), 1, "one connection attempt");
        assert!(drain(&mut lines[0])
            .iter()
            .all(|line| !line.contains("\"c\":\"list\"")));
        shell.read_with(cx, |shell, _| {
            for host in shell.panes.iter().flatten() {
                assert_eq!(host.pane.place(), "/files");
                assert_eq!(
                    host.nav.counters.lists, 0,
                    "a network address never lists locally"
                );
            }
        });
        let view = card(&shell, cx);
        view.read_with(cx, |card, _| {
            assert!(card.model.retry && card.model.failed_connect);
            assert!(card.model.form.host.is_empty());
            assert!(
                !card.model.save_new,
                "repairing a typed address never saves it"
            );
        });
    }
}

#[gpui::test]
fn smbpath_complete_address_opens_only_the_pane_that_typed_it(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, mut lines) = open(recorder.clone(), cx);
    shell.update(cx, |shell, cx| shell.focus_pane(1, cx));
    frame(cx);
    drain(&mut lines[0]);
    drain(&mut lines[1]);
    cx.simulate_keystrokes("ctrl-l ctrl-a");
    cx.simulate_input("sftp://user@host/folder");
    cx.simulate_keystrokes("enter");
    frame(cx);
    assert_eq!(recorder.mounts().len(), 1);
    answer_mount(&recorder, "/fixture/mnt", cx);
    assert!(drain(&mut lines[0])
        .iter()
        .all(|line| !line.contains("\"c\":\"list\"")));
    let asked = drain(&mut lines[1]);
    assert!(
        asked.iter().any(|line| line.contains("/fixture/mnt")),
        "{asked:?}"
    );
}

#[gpui::test]
fn smbpath_unavailable_network_owner_paints_the_footer(cx: &mut TestAppContext) {
    let (shell, cx) = crate::window::u07_tests::open(cx);
    cx.simulate_keystrokes("ctrl-l ctrl-a");
    cx.simulate_input("smb://");
    cx.simulate_keystrokes("enter");
    frame(cx);
    let expected = shell.read_with(cx, |shell, _| {
        shell.locale.catalog.tr("path.network_unavailable")
    });
    assert_eq!(call(&shell, "statusPrimary", &[], cx), expected);
    assert_eq!(call(&shell, "statusError", &[], cx), "true");
}

#[gpui::test]
fn m9_net_ipc_card_readers_read_the_open_card_and_its_painted_frame(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = open(Rc::new(Recorder::default()), cx);
    cx.simulate_resize(size(px(1100.), px(700.)));
    frame(cx);
    for name in [
        "networkUri",
        "networkCardRect",
        "networkFocusState",
        "networkFields",
    ] {
        assert_eq!(call(&shell, name, &[], cx), "", "a shut card reads empty");
    }
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.network_add(0, window, cx)));
    frame(cx);
    let view = card(&shell, cx);
    let word = |cx: &mut VisualTestContext, pick: fn(&NetworkView) -> &String| {
        cx.update(|_, cx| pick(view.read(cx)).clone())
    };
    assert_eq!(call(&shell, "networkProtocol", &[], cx), "SMB");
    assert_eq!(call(&shell, "networkPort", &[], cx), "445");
    assert_eq!(call(&shell, "networkPathLabel", &[], cx), "Share");
    assert_eq!(
        call(&shell, "networkFields", &[], cx),
        "Label|Host|Port|Share|Domain|Username|Password"
    );
    assert_eq!(call(&shell, "networkFocus", &[], cx), "Host");
    let title = word(cx, |card| &card.words.title);
    assert!(!title.is_empty() && title != "network.title");
    assert_eq!(
        call(&shell, "networkTitle", &[], cx),
        title,
        "the drawn title"
    );
    let connect = word(cx, |card| &card.words.connect);
    assert_eq!(
        call(&shell, "networkAction", &[], cx),
        connect,
        "the drawn default"
    );
    assert_eq!(
        call(&shell, "networkUri", &[], cx),
        "",
        "no host builds no URI"
    );
    assert_eq!(call(&shell, "networkStatus", &[], cx), "");
    assert_eq!(call(&shell, "networkNote", &[], cx), "");
    let hint = word(cx, |card| &card.words.password_hint);
    assert_eq!(call(&shell, "networkPasswordHint", &[], cx), hint);
    assert_eq!(
        call(&shell, "networkPasswordState", &[], cx),
        "masked|empty"
    );
    let state = call(&shell, "networkFocusState", &[], cx);
    for part in ["\"inside\":true", "\"busy\":false", "\"field\":\"Host\""] {
        assert!(state.contains(part), "{part} in {state}");
    }
    let card_rect = numbers(&call(&shell, "networkCardRect", &[], cx));
    let body = numbers(&call(&shell, "networkBodyRect", &[], cx));
    assert_eq!(card_rect[2], 499., "the painted card width: {card_rect:?}");
    assert!(
        body[0] > card_rect[0]
            && body[1] > card_rect[1]
            && body[0] + body[2] < card_rect[0] + card_rect[2]
            && body[1] + body[3] < card_rect[1] + card_rect[3],
        "the body clip sits inside the card: {body:?} in {card_rect:?}"
    );
    assert_eq!(call(&shell, "networkDialogMetrics", &[], cx), "16|12");
    assert_eq!(call(&shell, "networkDialogMetricTargets", &[], cx), "16|12");
    let widths = numbers(&call(&shell, "networkHostPortWidths", &[], cx));
    assert!(widths[0] > 0. && widths[0] == widths[1], "{widths:?}");
    let scroll = numbers(&call(&shell, "networkScroll", &[], cx));
    assert!(
        scroll[0] >= 0. && scroll[1] >= scroll[2] && scroll[2] > 0.,
        "{scroll:?}"
    );
    let eye = numbers(&call(&shell, "networkPasswordEyeCentre", &[], cx));
    assert_eq!(eye.len(), 2);
    click(&call(&shell, "networkChipCentre", &["SFTP"], cx), cx);
    assert_eq!(
        call(&shell, "networkProtocol", &[], cx),
        "SFTP",
        "a press at the chip centre picks it"
    );
    assert_eq!(call(&shell, "networkPort", &[], cx), "22");
    assert_eq!(call(&shell, "networkPathLabel", &[], cx), "Path");
    assert_eq!(
        call(&shell, "networkFields", &[], cx),
        "Label|Host|Port|Path|Username|Password"
    );
    assert_eq!(
        call(&shell, "networkFocus", &[], cx),
        "Host",
        "a pointer protocol change preserves the active Host field"
    );
    click(&call(&shell, "networkChipCentre", &["NFS"], cx), cx);
    assert_eq!(
        call(&shell, "networkFields", &[], cx),
        "Label|Host|Port|Export"
    );
    assert_eq!(call(&shell, "networkPasswordHint", &[], cx), "");
    assert_eq!(call(&shell, "networkPasswordEyeCentre", &[], cx), "");
    let note = word(cx, |card| &card.words.nfs_hint);
    assert!(!note.is_empty());
    assert_eq!(call(&shell, "networkNote", &[], cx), note);
    cx.update(|_, cx| {
        view.update(cx, |card, cx| {
            card.model.form.host = "nas.local".into();
            card.changed(cx)
        })
    });
    frame(cx);
    assert!(call(&shell, "networkUri", &[], cx).starts_with("nfs://nas.local"));
    cx.update(|_, cx| view.update(cx, |card, cx| card.close(cx)));
    frame(cx);
    assert_eq!(
        call(&shell, "networkProtocol", &[], cx),
        "",
        "a closed card reads empty"
    );
    assert_eq!(call(&shell, "networkChipCentre", &["SMB"], cx), "");
}

#[gpui::test]
fn m9_net_ipc_rail_readers_read_the_network_rows_and_the_plus(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = open(recorder.clone(), cx);
    assert_eq!(call(&shell, "networkBuilt", &[], cx), "false");
    assert_eq!(call(&shell, "networkEntries", &[], cx), "");
    assert_eq!(
        call(&shell, "networkMarkGeometry", &[], cx),
        "",
        "no NETWORK row, no dot"
    );
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.network_publish(Some(vec![share()]), cx)
        })
    });
    frame(cx);
    assert_eq!(
        call(&shell, "networkEntries", &[], cx),
        "NAS|network|share|false"
    );
    let first = cx.update(|_, cx| {
        let producer = shell.read(cx).rail.as_ref().unwrap().places.read(cx);
        producer
            .entries()
            .iter()
            .position(|entry| entry.group == Group::Network)
            .unwrap()
    });
    assert_eq!(
        call(&shell, "networkStartIndex", &[], cx),
        first.to_string()
    );
    let geometry = call(&shell, "networkMarkGeometry", &[], cx);
    let boxes: Vec<Vec<f32>> = geometry.split('|').map(numbers).collect();
    assert_eq!(boxes.len(), 3, "{geometry}");
    assert_eq!(
        boxes[0][2], boxes[1][2],
        "the press box centres on the ink: {geometry}"
    );
    assert!(
        boxes[1][1] >= 24.,
        "the press box meets the hit minimum: {geometry}"
    );
    let centre = numbers(&call(&shell, "networkMarkCentre", &[], cx));
    assert!(
        (centre[0] - boxes[1][2]).abs() <= 0.5,
        "{centre:?} {geometry}"
    );
    assert_eq!(
        call(&shell, "networkBuilt", &[], cx),
        "false",
        "a direct publish is not a listing"
    );
    cx.update(|_, cx| shell.update(cx, |shell, cx| shell.network_list(cx)));
    cx.run_until_parked();
    let listing = *recorder
        .find("-li")
        .last()
        .expect("a re-list runs gio mount -li");
    recorder.answer(listing, 0, "");
    cx.run_until_parked();
    frame(cx);
    assert_eq!(
        call(&shell, "networkBuilt", &[], cx),
        "true",
        "a listing and a mountinfo read build the group"
    );
}

#[gpui::test]
fn the_add_share_card_takes_typed_text_through_the_window(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = open(Rc::new(Recorder::default()), cx);
    cx.simulate_resize(size(px(1100.), px(700.)));
    frame(cx);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.network_add(0, window, cx)));
    frame(cx);
    assert_eq!(call(&shell, "networkFocus", &[], cx), "Host");
    let hidden = call(&shell, "showHidden", &[], cx);
    // Each key goes down through the window's key handler, then its text reaches the focused field through IME.
    cx.simulate_keystrokes("n a s . l o c a l");
    frame(cx);
    let view = card(&shell, cx);
    let host = cx.update(|_, cx| view.read(cx).model.form.host.clone());
    assert_eq!(
        host, "nas.local",
        "the typed host lands in the focused field"
    );
    assert_eq!(
        call(&shell, "showHidden", &[], cx),
        hidden,
        "a typed . never reaches the listing's toggleHidden while the card holds the keyboard"
    );
}

#[gpui::test]
fn uishfix_p2f_dialog_open_tracks_the_mounted_network_card(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = open(Rc::new(Recorder::default()), cx);
    assert_eq!(call(&shell, "dialogOpen", &[], cx), "false");
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.network_add(0, window, cx)));
    frame(cx);
    assert_eq!(call(&shell, "dialogOpen", &[], cx), "true");
    let view = card(&shell, cx);
    cx.update(|_, cx| view.update(cx, |card, cx| card.close(cx)));
    frame(cx);
    assert_eq!(call(&shell, "dialogOpen", &[], cx), "false");
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.network_add(0, window, cx)));
    frame(cx);
    assert_eq!(call(&shell, "dialogOpen", &[], cx), "true");
    cx.update(|_, cx| shell.update(cx, |shell, _| shell.rail = None));
    assert_eq!(call(&shell, "dialogOpen", &[], cx), "false");
}

#[path = "network_cards_tests.rs"]
mod cards_tests;
