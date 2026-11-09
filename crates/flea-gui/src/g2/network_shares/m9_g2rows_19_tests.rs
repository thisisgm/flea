//! Acceptance pins for the joined form and mount service; native host composition is checked separately.
use super::{
    dialog::Focus,
    mount::{self, Phase},
    routing::{address, AddressRoute},
    service::{Service, ServiceEvent},
    view::{NetworkEvent, NetworkView},
};
use crate::g2::device_rail::process::{Event, Handle, Job, KillProbe, Outcome, Runner};
use gpui::{px, AppContext, Entity, TestAppContext, VisualTestContext};
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
fn m9_g2rows_19_down_and_typing_in_host_open_no_list(cx: &mut TestAppContext) {
    let (_root, card, cx) = card(cx);
    let effects = Rc::new(RefCell::new(Vec::new()));
    let seen = effects.clone();
    cx.update(|_, cx| {
        cx.subscribe(&card, move |_, event: &NetworkEvent, _| {
            seen.borrow_mut().push(format!("{:?}", event.0));
        })
        .detach();
    });
    let geometry = cx.update(|_, cx| {
        let view = card.read(cx);
        let drawn = view.drawn.borrow();
        (drawn.origin, drawn.card)
    });
    cx.simulate_input("EA");
    frame(cx);
    cx.simulate_keystrokes("down");
    frame(cx);
    cx.update(|_, cx| {
        let view = card.read(cx);
        assert_eq!(view.model.focus, Focus::Host);
        assert_eq!(&*view.model.form.host, "EA");
        let drawn = view.drawn.borrow();
        assert_eq!(
            (drawn.origin, drawn.card),
            geometry,
            "typing and Down keep the card at its one height and place"
        );
    });
    assert!(
        effects.borrow().is_empty(),
        "typing and Down emit no discovery, mount or save"
    );
}

#[gpui::test]
fn m9_g2rows_19_tab_through_every_field_keeps_one_card_and_no_list(cx: &mut TestAppContext) {
    let (_root, card, cx) = card(cx);
    let effects = Rc::new(RefCell::new(Vec::new()));
    let seen = effects.clone();
    cx.update(|_, cx| {
        cx.subscribe(&card, move |_, event: &NetworkEvent, _| {
            seen.borrow_mut().push(format!("{:?}", event.0));
        })
        .detach();
    });
    let geometry = cx.update(|_, cx| {
        let view = card.read(cx);
        let drawn = view.drawn.borrow();
        (drawn.origin, drawn.card)
    });
    let steps = cx.update(|_, cx| card.read(cx).model.ring().len());
    for _ in 0..steps {
        cx.simulate_keystrokes("tab");
        frame(cx);
        cx.update(|_, cx| {
            let view = card.read(cx);
            let drawn = view.drawn.borrow();
            assert_eq!(
                (drawn.origin, drawn.card),
                geometry,
                "every field keeps the card at its one height and place"
            );
        });
    }
    assert!(
        effects.borrow().is_empty(),
        "Tab through every field emits no discovery, mount or save"
    );
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
