//! The NETWORK plus with List nearby hosts: off it opens the 0.4.0 card unchanged; on it lists hosts and a pick only fills the card.
use super::*;
use crate::backend::Event as BackendEvent;
use crate::g2::mn1::menu::Outcome;
use crate::g2::pl2::{dialog::Focus, nearby};
use flea::ui_logic_pl2::form::{Form, Protocol, PROTOCOLS};

const FOUND: &str = r#"{"t":"nearby","id":1,"state":"found","message":"","rows":[{"name":"backup","source":"mdns","uri":"smb://backup.local/","scheme":"smb","protocol":"smb","host":"backup.local","port":445,"user":"","domain":"","path":"/","tls":false},{"name":"east","source":"tailscale","uri":"sftp://east.tail.ts.net/","scheme":"sftp","protocol":"sftp","host":"east.tail.ts.net","port":22,"user":"","domain":"","path":"/","tls":false}],"errors":[]}"#;

fn press_plus(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let producer = cx.update(|_, cx| shell.read(cx).rail.as_ref().unwrap().places.clone());
    cx.update(|_, cx| producer.update(cx, |p, _| p.model.borrow_mut().add_presses += 1));
    frame(cx);
}

fn tick(shell: &Entity<Shell>, cx: &mut VisualTestContext, on: bool) {
    shell.update(cx, |shell, _| {
        shell.launch.settings.data =
            flea::jsondoc::parse(&format!(r#"{{"places":{{"nearbyHosts":{on}}}}}"#)).unwrap()
    });
}

fn card(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
) -> Entity<crate::g2::pl2::view::NetworkView> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .card
            .clone()
    })
}

fn menu_open(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .menu
            .borrow()
            .opened()
    })
}

fn nearby_lines(lines: &mut Lines) -> Vec<String> {
    drain(lines)
        .into_iter()
        .filter(|line| line.contains(r#""c":"nearby""#))
        .collect()
}

#[gpui::test]
fn n343_option_off_plus_opens_the_card_at_rest_and_asks_the_network_nothing(
    cx: &mut TestAppContext,
) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, mut lines) = open(recorder.clone(), cx);
    // The default file and an explicit off both keep 0.4.0's plus.
    for on in [None, Some(false)] {
        if let Some(on) = on {
            tick(&shell, cx, on);
        }
        assert!(!crate::window::network::nearby::enabled(
            &shell.read_with(cx, |s, _| s.launch.settings.data.clone())
        ));
        press_plus(&shell, cx);
        let card = card(&shell, cx);
        assert!(
            cx.update(|_, cx| card.read(cx).model.opened),
            "the plus opens the card"
        );
        assert!(!menu_open(&shell, cx), "no hosts menu with the option off");
        cx.update(|_, cx| {
            let model = &card.read(cx).model;
            assert_eq!(model.form, Form::default(), "the card opens at rest");
            assert_eq!(model.focus, Focus::Host);
        });
        assert!(
            nearby_lines(&mut lines[0]).is_empty(),
            "no discovery runs with the option off"
        );
        cx.update(|_, cx| card.update(cx, |card, cx| card.close(cx)));
        frame(cx);
    }
    assert!(recorder.mounts().is_empty());
}

#[gpui::test]
fn n343_option_on_lists_hosts_and_a_pick_fills_the_card_without_connecting(
    cx: &mut TestAppContext,
) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, mut lines) = open(recorder.clone(), cx);
    tick(&shell, cx, true);
    press_plus(&shell, cx);
    assert!(menu_open(&shell, cx), "the plus opens the hosts menu");
    let card = card(&shell, cx);
    assert!(!cx.update(|_, cx| card.read(cx).model.opened));
    assert_eq!(nearby_lines(&mut lines[0]), [nearby::request("start", 1)]);
    let entries = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let network = shell
                .read(cx)
                .rail
                .as_ref()
                .unwrap()
                .network
                .as_ref()
                .unwrap();
            let menu = network.menu.borrow();
            let card = menu.card.as_ref().unwrap();
            (card.entries.clone(), card.cursor, card.frame.w)
        })
    };
    let (looking, cursor, width) = entries(&shell, cx);
    assert!(looking[0].busy);
    assert_eq!(looking.last().unwrap().action, nearby::ADD_SHARE);
    assert_eq!(
        cursor,
        Some(looking.len() - 1),
        "Add a Network Share… holds the cursor"
    );
    assert!(
        width >= nearby::MENU_MIN,
        "the menu keeps its floor, {width}"
    );
    // An answer to another request lands nowhere; the live one replaces the busy row.
    let stale = FOUND.replace(r#""id":1"#, r#""id":9"#);
    cx.update(|window, cx| {
        shell.update(cx, |s, cx| {
            s.receive(0, BackendEvent::Producer(stale), window, cx)
        })
    });
    assert!(entries(&shell, cx).0[0].busy);
    cx.update(|window, cx| {
        shell.update(cx, |s, cx| {
            s.receive(0, BackendEvent::Producer(FOUND.into()), window, cx)
        })
    });
    let (found, cursor, _) = entries(&shell, cx);
    assert_eq!(found.len(), 4);
    assert_eq!(
        cursor,
        Some(0),
        "the first host takes the cursor from Add a Network Share…"
    );
    cx.update(|window, cx| {
        shell.update(cx, |s, cx| {
            s.nearby_outcome(
                Outcome::Fire {
                    action: "nearby:0".into(),
                    leaf: String::new(),
                },
                window,
                cx,
            )
        })
    });
    frame(cx);
    assert!(!menu_open(&shell, cx));
    cx.update(|_, cx| {
        let model = &card.read(cx).model;
        assert!(model.opened);
        assert_eq!(model.form.protocol, Protocol::Smb);
        assert_eq!(
            (model.form.host.as_str(), model.form.port.as_str()),
            ("backup.local", "445")
        );
        assert_eq!(model.focus, Focus::Connect);
        assert!(!model.connecting);
    });
    assert!(
        recorder.mounts().is_empty(),
        "a pick never connects by itself"
    );
    assert!(
        nearby_lines(&mut lines[0]).is_empty(),
        "a finished scan needs no stop"
    );
}

#[gpui::test]
fn n343_add_a_network_share_row_opens_the_card_at_rest_and_ends_the_scan(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, mut lines) = open(recorder, cx);
    tick(&shell, cx, true);
    press_plus(&shell, cx);
    assert_eq!(nearby_lines(&mut lines[0]).len(), 1);
    cx.update(|window, cx| {
        shell.update(cx, |s, cx| {
            s.nearby_outcome(
                Outcome::Fire {
                    action: nearby::ADD_SHARE.into(),
                    leaf: String::new(),
                },
                window,
                cx,
            )
        })
    });
    frame(cx);
    let card = card(&shell, cx);
    cx.update(|_, cx| {
        let model = &card.read(cx).model;
        assert!(model.opened);
        assert_eq!(model.form, Form::default());
    });
    assert_eq!(nearby_lines(&mut lines[0]), [nearby::request("stop", 1)]);
}

#[test]
fn n343_the_card_keeps_its_five_protocols_and_their_fields() {
    assert_eq!(
        PROTOCOLS,
        [
            Protocol::Smb,
            Protocol::Sftp,
            Protocol::Ftps,
            Protocol::WebDav,
            Protocol::Nfs
        ]
    );
    let words: Vec<_> = PROTOCOLS.iter().map(|p| p.key()).collect();
    assert_eq!(
        words,
        [
            "network.smb",
            "network.sftp",
            "network.ftps",
            "network.webdav",
            "network.nfs"
        ]
    );
    let mut d = crate::g2::pl2::dialog::Dialog::default();
    d.open();
    assert_eq!(
        d.ring(),
        [
            Focus::Protocol(0),
            Focus::Protocol(1),
            Focus::Protocol(2),
            Focus::Protocol(3),
            Focus::Protocol(4),
            Focus::Label,
            Focus::Host,
            Focus::Port,
            Focus::Path,
            Focus::Domain,
            Focus::User,
            Focus::Password,
            Focus::Cancel,
            Focus::Connect,
            Focus::Dropbox,
        ]
    );
}

#[gpui::test]
fn n343_a_key_pressed_while_looking_keeps_the_cursor_on_add_a_network_share(
    cx: &mut TestAppContext,
) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = open(recorder.clone(), cx);
    tick(&shell, cx, true);
    press_plus(&shell, cx);
    let cursor = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let network = shell
                .read(cx)
                .rail
                .as_ref()
                .unwrap()
                .network
                .as_ref()
                .unwrap();
            let menu = network.menu.borrow();
            menu.card
                .as_ref()
                .map(|card| (card.cursor, card.entries.len()))
        })
    };
    // Any key since the menu opened counts, even one that cannot move the cursor off the only live row.
    cx.update(|window, cx| {
        shell.update(cx, |s, cx| assert!(s.network_run("cursorDown", window, cx)))
    });
    cx.update(|window, cx| {
        shell.update(cx, |s, cx| {
            s.receive(0, BackendEvent::Producer(FOUND.into()), window, cx)
        })
    });
    let (at, len) = cursor(&shell, cx).unwrap();
    assert_eq!(
        (at, len),
        (Some(3), 4),
        "the answer leaves the cursor where the person put it"
    );
    // Enter now opens Add a Network Share… at rest, never the first host.
    cx.update(|window, cx| shell.update(cx, |s, cx| assert!(s.network_run("open", window, cx))));
    frame(cx);
    let card = card(&shell, cx);
    cx.update(|_, cx| {
        let model = &card.read(cx).model;
        assert!(model.opened);
        assert_eq!(model.form, Form::default());
    });
    assert!(recorder.mounts().is_empty());
}

#[gpui::test]
fn n343_enter_while_looking_opens_add_a_network_share(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = open(recorder, cx);
    tick(&shell, cx, true);
    press_plus(&shell, cx);
    cx.update(|window, cx| shell.update(cx, |s, cx| assert!(s.network_run("open", window, cx))));
    frame(cx);
    // The answer that lands after Enter finds no menu and moves nothing.
    cx.update(|window, cx| {
        shell.update(cx, |s, cx| {
            s.receive(0, BackendEvent::Producer(FOUND.into()), window, cx)
        })
    });
    assert!(!menu_open(&shell, cx));
    let card = card(&shell, cx);
    cx.update(|_, cx| {
        let model = &card.read(cx).model;
        assert!(model.opened);
        assert_eq!(model.form, Form::default());
    });
}

#[gpui::test]
fn n343_a_backend_that_never_answers_drops_the_busy_row_after_the_quiet_time(
    cx: &mut TestAppContext,
) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, mut lines) = open(recorder, cx);
    tick(&shell, cx, true);
    press_plus(&shell, cx);
    assert_eq!(nearby_lines(&mut lines[0]).len(), 1);
    let rows = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let network = shell
                .read(cx)
                .rail
                .as_ref()
                .unwrap()
                .network
                .as_ref()
                .unwrap();
            let menu = network.menu.borrow();
            let card = menu.card.as_ref().unwrap();
            let actions: Vec<String> = card.entries.iter().map(|e| e.action.to_string()).collect();
            (actions, card.cursor)
        })
    };
    cx.executor().advance_clock(Duration::from_secs(4));
    cx.run_until_parked();
    assert!(
        rows(&shell, cx).0.len() == 3,
        "still looking inside the quiet time"
    );
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(
        rows(&shell, cx),
        (vec![nearby::ADD_SHARE.to_string()], Some(0)),
        "the busy row gives way to Add a Network Share… alone"
    );
    // The scan already counts as over, so closing the menu sends no stop.
    cx.update(|window, cx| shell.update(cx, |s, cx| s.nearby_outcome(Outcome::Closed, window, cx)));
    assert!(nearby_lines(&mut lines[0]).is_empty());
}
