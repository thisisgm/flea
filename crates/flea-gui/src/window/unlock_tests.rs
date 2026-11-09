//! The CLI request through the production constructor, card, input and mount handoff.
use super::*;
use crate::g2::device_rail::process::{Event, Handle, Job, Outcome, Runner};
use crate::window::startup::{Arguments, Launch, Settings};
use gpui::{TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc, sync::Arc};

type Started = (Job, async_channel::Sender<Event>);

#[derive(Default)]
struct Devices(RefCell<Vec<Started>>);

impl Runner for Devices {
    fn start(&self, job: Job) -> Handle {
        let (handle, events) = Handle::detached();
        events.try_send(Event::Started).unwrap();
        self.0.borrow_mut().push((job, events));
        handle
    }
}

impl Devices {
    fn answer(&self, program: &str, body: &str) {
        self.0
            .borrow()
            .iter()
            .rev()
            .find(|(job, _)| job.argv[0] == program)
            .expect("the request started this child")
            .1
            .try_send(Event::Exited(Outcome {
                code: 0,
                stdout: body.into(),
                stderr: String::new(),
            }))
            .unwrap();
    }
}

fn open<'a>(
    settings: &str,
    device: Option<&str>,
    cx: &'a mut TestAppContext,
) -> (Entity<Shell>, Rc<Devices>, &'a mut VisualTestContext) {
    let devices = Rc::new(Devices::default());
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, cx| {
                let (backend, requests) = crate::backend::Backend::test_channel();
                cx.on_release(move |_, _| drop(requests)).detach();
                Ok(backend)
            },
            monitor: Some(|| Err(std::io::Error::other("no external monitor in this fixture"))),
        });
        cx.set_global(crate::g2::device_rail::source::fixture::Services {
            runner: devices.clone(),
            hostname: "fixture".into(),
        });
    });
    let mut launch = Launch::resolve(
        Settings::from_json(flea::jsondoc::parse(settings).unwrap()),
        Arguments::default(),
        "/fixture/home",
    );
    launch.unlock = device.map(str::to_owned);
    let locale = crate::window::locale::Snapshot {
        revision: 1,
        catalog: Arc::new(crate::test_catalogs::english()),
    };
    let (_send, events) = async_channel::unbounded();
    let (shell, cx) =
        cx.add_window_view(|window, cx| Shell::new(launch, locale, events, window, cx));
    frame(cx);
    (shell, devices, cx)
}

fn frame(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn view(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Entity<UnlockView> {
    shell.read_with(cx, |shell, _| {
        shell
            .unlock_view()
            .expect("the startup request opened the card")
    })
}

#[gpui::test]
fn unlock_launch_opens_once_over_the_focused_dual_pane_and_escape_restores_focus(
    cx: &mut TestAppContext,
) {
    let (shell, devices, cx) = open(
        r#"{"view":"dual","dual":{"focus":1,"paths":["/fixture/left","/fixture/right"]}}"#,
        Some("/dev/sdb1"),
        cx,
    );
    let card = view(&shell, cx);
    shell.read_with(cx, |shell, cx| {
        assert!(
            shell.launch.unlock.is_none(),
            "the request is consumed once"
        );
        assert_eq!(
            shell.rail.as_ref().unwrap().unlock.as_ref().unwrap().side,
            1
        );
        assert!(shell.unlock_layer(cx).is_some());
        assert!(shell.card_holds_keyboard(cx));
        let view = card.read(cx);
        let state = view.host.card.as_ref().unwrap();
        assert_eq!(
            (state.device.as_str(), state.label.as_str()),
            ("/dev/sdb1", "sdb1")
        );
        assert!(state.field.is_empty());
        assert!(!state.unlock_available());
    });
    cx.update(|window, cx| assert!(card.read(cx).focus_handle().is_focused(window)));
    cx.simulate_keystrokes("escape");
    frame(cx);
    shell.read_with(cx, |shell, cx| assert!(!shell.card_holds_keyboard(cx)));
    cx.update(|window, cx| assert!(shell.read(cx).focus.is_focused(window)));
    assert!(devices
        .0
        .borrow()
        .iter()
        .all(|(job, _)| job.argv[0] != "udisksctl"));
    frame(cx);
    shell.read_with(cx, |shell, cx| assert!(shell.unlock_layer(cx).is_none()));
}

#[gpui::test]
fn unlock_launch_hidden_rail_names_the_device_and_mounts_with_stdin_only(cx: &mut TestAppContext) {
    let (shell, devices, cx) = open(
        r#"{"view":"list","places":{"rail":"hidden"}}"#,
        Some("/dev/sdb1"),
        cx,
    );
    assert_eq!(
        devices.0.borrow().len(),
        1,
        "only the CLI card's one-shot lsblk runs"
    );
    devices.answer("lsblk", r#"{"blockdevices":[{"name":"sdb1","path":"/dev/sdb1","type":"part","label":"Backup","fstype":"crypto_LUKS","fsver":"2","size":2000000}]}"#);
    frame(cx);
    let card = view(&shell, cx);
    card.read_with(cx, |view, _| {
        let state = view.host.card.as_ref().unwrap();
        assert_eq!(
            (state.label.as_str(), state.crypto.as_str()),
            ("Backup", "LUKS2")
        );
    });
    cx.simulate_keystrokes("s e c r e t enter");
    frame(cx);
    {
        let jobs = devices.0.borrow();
        let job = &jobs.last().unwrap().0;
        assert_eq!(
            job.argv,
            [
                "udisksctl",
                "unlock",
                "--block-device",
                "/dev/sdb1",
                "--key-file",
                "/dev/stdin"
            ]
        );
        assert_eq!(job.stdin.as_ref().unwrap().expose(), b"secret");
        assert_eq!(job.env, [("LC_ALL".into(), "C".into())]);
    }
    devices.answer("udisksctl", "Unlocked /dev/sdb1 as /dev/dm-0.\n");
    frame(cx);
    assert_eq!(
        devices.0.borrow().last().unwrap().0.argv,
        ["udisksctl", "mount", "--block-device", "/dev/dm-0"]
    );
    devices.answer("udisksctl", "Mounted /dev/dm-0 at /run/media/gm/Backup.\n");
    frame(cx);
    shell.read_with(cx, |shell, cx| {
        assert!(!shell.card_holds_keyboard(cx));
        assert_eq!(
            shell.panes[0].as_ref().unwrap().pane.place(),
            "/run/media/gm/Backup"
        );
    });
}

#[gpui::test]
fn unlock_launch_without_a_request_keeps_the_card_lazy(cx: &mut TestAppContext) {
    let (shell, devices, cx) = open(r#"{"places":{"rail":"hidden"}}"#, None, cx);
    shell.read_with(cx, |shell, cx| {
        assert!(shell.unlock_view().is_none());
        assert!(shell.unlock_layer(cx).is_none());
    });
    assert!(devices.0.borrow().is_empty());
}
