//! Acceptance pins for the joined form and mount service; native host composition is checked separately.
use super::{
    dialog::{Effect, Focus},
    mount::{self, Phase},
    routing::{address, AddressRoute},
    service::{Service, ServiceEvent},
    view::{NetworkEvent, NetworkView},
};
use crate::g2::pl1::process::{Event, Handle, Job, KillProbe, Outcome, Runner};
use gpui::{point, px, AppContext, Entity, TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc};

#[path = "m9_g2rows_19_handoff_tests.rs"]
mod handoff;

fn reply(id: u64, state: &str, rows: &str) -> String {
    format!(r#"{{"t":"nearby","id":{id},"state":"{state}","rows":[{rows}],"errors":[]}}"#)
}

const HOSTS: &str = r#"{"name":"backup","source":"mdns","uri":"smb://backup.local/media"},{"name":"east","source":"tailscale","uri":"sftp://east:2222/work"},{"name":"storage","source":"mdns","uri":"nfs://storage.local/export"}"#;

fn frame(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn card(
    cx: &mut TestAppContext,
) -> (
    super::store_tests::Root,
    Entity<NetworkView>,
    &mut VisualTestContext,
) {
    let (root, catalog) = super::input_tests::catalog();
    let (card, cx) =
        cx.add_window_view(|_, cx| NetworkView::new(catalog, super::input_tests::palette(), cx));
    cx.simulate_resize(gpui::size(px(1100.), px(700.)));
    cx.update(|window, cx| {
        card.update(cx, |view, cx| {
            view.model.dropbox_installed = true;
            view.open_for(41, window, cx);
        });
    });
    frame(cx);
    (root, card, cx)
}

#[gpui::test]
fn m9_g2rows_19_nearby_selection_fills_without_connecting(cx: &mut TestAppContext) {
    let (_root, card, cx) = card(cx);
    let effects = Rc::new(RefCell::new(Vec::new()));
    let seen = effects.clone();
    cx.update(|_, cx| {
        cx.subscribe(&card, move |_, event: &NetworkEvent, _| {
            seen.borrow_mut().push(format!("{:?}", event.0));
        })
        .detach();
        card.update(cx, |view, cx| {
            let id = view.model.discovery.id.unwrap();
            assert!(view.line(&reply(id, "found", HOSTS), cx));
        });
    });
    frame(cx);
    let geometry = cx.update(|_, cx| {
        let view = card.read(cx);
        let drawn = view.drawn.borrow();
        let popup = view.menu.borrow();
        let popup = popup.card.as_ref().expect("hosts open a popup");
        let host = super::drawing::field(Focus::Host).unwrap();
        let port = super::drawing::field(Focus::Port).unwrap();
        assert_eq!(popup.frame.x, drawn.origin.0 + host.x);
        assert_eq!(popup.frame.y, drawn.origin.1 + host.y + host.h + 3.);
        assert_eq!(popup.frame.w, port.x + port.w - host.x);
        assert!(view.model.discovery.rows[1].tailnet);
        (drawn.origin, drawn.card)
    });
    cx.simulate_keystrokes("down enter");
    frame(cx);
    cx.update(|_, cx| {
        let view = card.read(cx);
        assert_eq!(
            view.model.form.protocol,
            flea::ui_logic_pl2::form::Protocol::Sftp
        );
        assert_eq!(
            (
                &*view.model.form.host,
                &*view.model.form.port,
                &*view.model.form.path
            ),
            ("east", "2222", "work")
        );
        assert_eq!(view.model.focus, Focus::Connect);
        assert!(!view.model.connecting && !view.model.saving);
        assert!(!view.menu.borrow().opened());
        let drawn = view.drawn.borrow();
        assert_eq!(
            (drawn.origin, drawn.card),
            geometry,
            "choosing a different protocol keeps the card height"
        );
    });
    assert!(
        effects.borrow().is_empty(),
        "selection emits no mount or save"
    );
    cx.update(|_, cx| {
        card.update(cx, |view, cx| {
            view.model.focus = Focus::Host;
            view.model.form.host.clear();
            view.load_input();
            view.changed(cx);
        })
    });
    frame(cx);
    let hit = cx.update(|_, cx| {
        let view = card.read(cx);
        let menu = view.menu.borrow();
        let popup = menu.card.as_ref().unwrap();
        point(px(popup.frame.x + 30.), px(popup.frame.y + 15.))
    });
    cx.simulate_mouse_down(hit, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_up(hit, gpui::MouseButton::Left, gpui::Modifiers::default());
    frame(cx);
    cx.update(|_, cx| {
        let view = card.read(cx);
        assert_eq!(
            view.model.form.uri().as_deref(),
            Some("smb://backup.local/media")
        );
        assert_eq!(view.model.focus, Focus::Connect);
    });
    assert!(effects.borrow().is_empty());
    cx.simulate_keystrokes("enter");
    assert_eq!(effects.borrow().len(), 1, "only explicit Connect submits");
    assert!(effects.borrow()[0].starts_with("Mount {"));
}

#[gpui::test]
fn m9_g2rows_19_deadline_close_and_typing_ignore_stale_hosts(cx: &mut TestAppContext) {
    let (_root, card, cx) = card(cx);
    let first = cx.update(|_, cx| card.read(cx).model.discovery.id.unwrap());
    cx.update(|_, cx| assert!(card.read(cx).model.discovery.looking));
    // The backend owns expiry; the view consumes its terminal reply without an empty popup.
    cx.update(|_, cx| {
        card.update(cx, |view, cx| {
            assert!(view.line(&reply(first, "empty", ""), cx));
            assert!(!view.model.discovery.looking);
        })
    });
    frame(cx);
    cx.update(|_, cx| assert!(!card.read(cx).menu.borrow().opened()));
    cx.update(|window, cx| {
        card.update(cx, |view, cx| {
            view.close(cx);
            view.open_for(42, window, cx);
            assert!(!view.line(&reply(first, "found", HOSTS), cx));
            let current = view.model.discovery.id.unwrap();
            assert_ne!(current, first);
            assert!(view.line(&reply(current, "found", HOSTS), cx));
        })
    });
    frame(cx);
    cx.simulate_input("EAST");
    frame(cx);
    cx.update(|_, cx| {
        let view = card.read(cx);
        assert_eq!(view.model.discovery.filtered(&view.model.form.host), [1]);
    });
    cx.simulate_keystrokes("enter enter");
    let (request, uri) = cx.update(|_, cx| {
        let view = card.read(cx);
        assert!(view.model.connecting);
        (view.model.request, view.model.pending_uri.clone())
    });
    cx.update(|window, cx| {
        card.update(cx, |view, cx| {
            let effects = view.model.close();
            assert!(effects
                .iter()
                .any(|effect| matches!(effect, Effect::Cancel(id) if *id == request)));
            view.open_for(43, window, cx);
            view.mount_finished(request, &uri, Ok(()), false, cx);
            assert_eq!(view.model.focus, Focus::Host);
            assert!(!view.model.connecting && !view.model.saving);
            let current = view.model.discovery.id.unwrap();
            assert!(view.line(&reply(current, "found", HOSTS), cx));
        })
    });
    frame(cx);
    cx.simulate_input("no-matching-host");
    frame(cx);
    cx.update(|_, cx| assert!(!card.read(cx).menu.borrow().opened()));
}

#[derive(Default)]
struct Recorder {
    jobs: RefCell<Vec<Job>>,
    answers: RefCell<Vec<async_channel::Sender<Event>>>,
    probes: RefCell<Vec<KillProbe>>,
}
impl Runner for Recorder {
    fn start(&self, job: Job) -> Handle {
        let (handle, send, probe) = Handle::probed();
        self.jobs.borrow_mut().push(job);
        self.answers.borrow_mut().push(send);
        self.probes.borrow_mut().push(probe);
        handle
    }
}

#[gpui::test]
fn m9_g2rows_19_network_address_uses_existing_mount_flow(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let service =
        cx.new(|_| Service::new(recorder.clone(), "/fixture/runtime".into(), String::new()));
    let results = Rc::new(RefCell::new(Vec::new()));
    let seen = results.clone();
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        cx.subscribe(&service, move |_, event: &ServiceEvent, _| {
            if let mount::Effect::Open { uri, origin, .. } = &event.0 {
                seen.borrow_mut().push((uri.clone(), *origin));
            }
        })
        .detach()
    });
    for (index, uri) in [
        "smb://nas/media",
        "sftp://east/work",
        "nfs://storage/export",
        "ftps://host/files",
        "dav://host/files",
        "davs://host/files",
        "user@east:/work",
    ]
    .iter()
    .enumerate()
    {
        assert!(
            !crate::path::rank::is_query(uri),
            "an address has no path suggestions"
        );
        let AddressRoute::Network(request) = address(uri, index as u64 + 1) else {
            panic!("valid address refused")
        };
        let canonical = request.uri.clone();
        let start = recorder.jobs.borrow().len();
        cx.update(|_, cx| service.update(cx, |service, cx| service.open_share(request, 0, cx)));
        cx.run_until_parked();
        assert_eq!(recorder.jobs.borrow()[start].argv.last(), Some(&canonical));
        recorder.answers.borrow()[start]
            .try_send(Event::Exited(Outcome::code(0)))
            .unwrap();
        cx.run_until_parked();
        assert_eq!(
            recorder.jobs.borrow()[start + 1].argv,
            ["gio", "info", "--", &canonical]
        );
        recorder.answers.borrow()[start + 1]
            .try_send(Event::Exited(Outcome {
                code: 0,
                stdout: "local path: /fixture/served\n".into(),
                stderr: String::new(),
            }))
            .unwrap();
        cx.run_until_parked();
        assert_eq!(
            results.borrow().last(),
            Some(&(canonical, index as u64 + 1))
        );
    }
    let start = recorder.jobs.borrow().len();
    for invalid in [
        "https://host/",
        "smb://user:secret@host/share",
        "nfs://host:0/export",
    ] {
        assert!(matches!(address(invalid, 77), AddressRoute::Invalid(_)));
    }
    assert!(matches!(address("/fixture/local", 77), AddressRoute::Local));
    assert_eq!(recorder.jobs.borrow().len(), start);
    let AddressRoute::Network(request) = address("sftp://late/work", 77) else {
        unreachable!()
    };
    cx.update(|_, cx| service.update(cx, |service, cx| service.open_share(request, 0, cx)));
    cx.run_until_parked();
    let generation = cx.update(|_, cx| service.read(cx).mounts.generation);
    cx.update(|_, cx| service.update(cx, |service, cx| service.cancel(0, 0, cx)));
    assert!(recorder.probes.borrow()[start].killed());
    recorder.answers.borrow()[start]
        .try_send(Event::Exited(Outcome::code(0)))
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        recorder.jobs.borrow().len(),
        start + 1,
        "a cancelled reply starts no info job"
    );
    cx.update(|_, cx| {
        service.update(cx, |service, _| {
            assert!(service
                .mounts
                .exited(generation, Phase::Mount, &Outcome::code(0), 1)
                .is_empty());
        })
    });
    assert_eq!(results.borrow().len(), 7);
    let shutdown = cx.update(|_, cx| service.update(cx, |service, cx| service.shutdown(cx)));
    for answer in recorder.answers.borrow().iter() {
        answer.close();
    }
    cx.run_until_parked();
    assert!(shutdown.is_ready());
}
