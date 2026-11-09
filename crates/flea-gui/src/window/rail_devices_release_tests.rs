//! The eject mark and Ctrl+E through Shell::new: a real pointer click, the keyboard frame and a refusal in the status bar.
use super::*;
use crate::window::{
    locale::Snapshot,
    startup::{Arguments, Launch, Settings},
};
use gpui::{Modifiers, MouseButton};

const BLOCKS: &str = r#"{"blockdevices":[
{"name":"nvme0n1","path":"/dev/nvme0n1","type":"disk","children":[{"name":"nvme0n1p1","type":"part","mountpoints":["/"],"fstype":"ext4"}]},
{"name":"sda1","path":"/dev/sda1","type":"part","label":"128GB","mountpoints":["/media/128GB"],"fstype":"exfat","rm":true,"size":128000000000},
{"name":"sdb1","path":"/dev/sdb1","type":"part","label":"Vault","mountpoints":["/media/vault"],"fstype":"ext4","size":2000000000}
]}"#;

type Receive = Box<dyn FnMut() -> Option<String>>;

thread_local! {
    static LINES: RefCell<Option<Receive>> = RefCell::new(None);
    static LOG: RefCell<Vec<String>> = RefCell::default();
}

/// Backend control lines and device jobs in the order the window sent them.
fn drain_lines() {
    LINES.with(|lines| {
        if let Some(receive) = lines.borrow_mut().as_mut() {
            for _ in 0..64 {
                if let Some(line) = receive() {
                    LOG.with(|log| log.borrow_mut().push(format!("backend {line}")));
                }
            }
        }
    });
}

#[derive(Default)]
struct Ordered(FakeDevices);

impl Runner for Ordered {
    fn start(&self, job: Job) -> Handle {
        drain_lines();
        LOG.with(|log| log.borrow_mut().push(format!("job {}", job.argv.join(" "))));
        self.0.start(job)
    }
}

fn shell_at<'a>(
    path: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<Shell>, Rc<Ordered>, &'a mut VisualTestContext) {
    shell_with_listing(path, BLOCKS, cx)
}

fn shell_with_listing<'a>(
    path: &str,
    blocks: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<Shell>, Rc<Ordered>, &'a mut VisualTestContext) {
    LOG.with(|log| log.borrow_mut().clear());
    let service = Rc::new(Ordered::default());
    let runner = service.clone();
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, cx| {
                let (backend, receive) = crate::backend::Backend::test_operation_channel();
                LINES.with(|lines| *lines.borrow_mut() = Some(Box::new(receive)));
                let _ = cx;
                Ok(backend)
            },
            monitor: None,
        });
        cx.set_global(crate::g2::device_rail::source::fixture::Services {
            runner,
            hostname: "minipc\n".into(),
        });
    });
    let state =
        flea::jsondoc::parse(r#"{"view":"list","places":{"rail":"shown","sidebarWidth":192}}"#)
            .unwrap();
    let launch = Launch::resolve(
        Settings::from_json(state),
        Arguments {
            path: Some(path.into()),
            select: None,
        },
        "/fixture/home",
    );
    let locale = Snapshot {
        revision: 1,
        catalog: Arc::new(crate::test_catalogs::english()),
    };
    let (_sender, events) = async_channel::unbounded();
    let (shell, cx) =
        cx.add_window_view(|window, cx| Shell::new(launch, locale, events, window, cx));
    cx.run_until_parked();
    service.0.answer("lsblk", 0, blocks);
    service.0.answer("gio", 0, "");
    frame(cx);
    cx.executor().advance_clock(Duration::from_millis(900));
    frame(cx);
    (shell, service, cx)
}

/// The listing keyboard with no listing out, since the fixture backend never answers its List.
fn listing_focus(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    shell.update_in(cx, |shell, window, cx| {
        if let Some(host) = &mut shell.panes[0] {
            host.pane.swap.pending = None;
        }
        shell.rail_focus(false, cx);
        shell.keyboard.context = flea::keys::Context::Listing;
        window.focus(&shell.focus, cx);
    });
    frame(cx);
}

fn row(
    shell: &Entity<Shell>,
    label: &str,
    cx: &mut VisualTestContext,
) -> crate::g2::rail_places::rows::LaidRow {
    producer(shell, cx).read_with(cx, |producer, _| {
        producer
            .laid
            .borrow()
            .rows
            .iter()
            .find(|row| row.label == label)
            .cloned()
            .unwrap_or_else(|| panic!("the rail paints {label}"))
    })
}

fn centre(rect: crate::rail::geometry::Rect) -> gpui::Point<gpui::Pixels> {
    gpui::point(
        gpui::px(rect.x + rect.width / 2.),
        gpui::px(rect.y + rect.height / 2.),
    )
}

fn jobs(service: &Ordered) -> Vec<String> {
    service
        .0
        .jobs
        .borrow()
        .iter()
        .map(|(argv, _, _)| argv.join(" "))
        .collect()
}

fn status(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    frame(cx);
    shell.read_with(cx, |shell, cx| {
        let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
        let line = footer.line.read(cx);
        line.painted
            .get("primary")
            .map(|drawn| drawn.text.to_string())
            .unwrap_or_default()
    })
}

#[gpui::test]
fn release_mark_replaces_the_square_and_lifts_under_the_pointer(cx: &mut TestAppContext) {
    let (shell, _, cx) = shell_at("/fixture/work", cx);
    let stick = row(&shell, "128GB", cx);
    assert!(
        stick.indicator_visible && stick.eject,
        "a mounted removable row draws the eject mark in its square's slot"
    );
    let hit = stick.eject_hit().unwrap();
    assert!(
        hit.width >= 24. && hit.height >= 24.,
        "the hit box keeps the WCAG floor"
    );
    let ink = stick.indicator_rect;
    assert!(
        (hit.x + hit.width / 2. - (ink.x + ink.width / 2.)).abs() < 0.01
            && (hit.y + hit.height / 2. - (ink.y + ink.height / 2.)).abs() < 0.01,
        "the hit box is centred on the caption-sized ink"
    );
    let producer = producer(&shell, cx);
    assert_eq!(
        producer.read_with(cx, |p, _| p.model.borrow().mark_hovered),
        None,
        "at rest the mark is muted"
    );
    cx.simulate_mouse_move(centre(hit), None, Modifiers::none());
    frame(cx);
    producer.read_with(cx, |p, _| {
        let model = p.model.borrow();
        assert_eq!(
            model.mark_hovered,
            Some(stick.index),
            "the pointer on the mark lifts it to the foreground"
        );
        assert_eq!(
            model.hovered,
            Some(stick.index),
            "the row under the mark takes the 8 percent hover wash"
        );
    });
}

#[gpui::test]
fn release_click_on_the_mark_ejects_after_the_pane_quiesces(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/media/128GB/photos", cx);
    let hit = row(&shell, "128GB", cx).eject_hit().unwrap();
    drain_lines();
    LOG.with(|log| log.borrow_mut().clear());
    cx.simulate_mouse_down(centre(hit), MouseButton::Left, Modifiers::none());
    frame(cx);
    assert!(
        !jobs(&service)
            .iter()
            .any(|job| job.starts_with("gio mount -e")),
        "a press alone releases nothing, as TapHandler ReleaseWithinBounds"
    );
    cx.simulate_mouse_up(centre(hit), MouseButton::Left, Modifiers::none());
    frame(cx);
    assert!(
        jobs(&service).contains(&"gio mount -e /media/128GB".to_string()),
        "the click ejects that mount: {:?}",
        jobs(&service)
    );
    let log = LOG.with(|log| log.borrow().clone());
    let quiesce = log.iter().position(|line| line.contains("thumbcancel"));
    let eject = log
        .iter()
        .position(|line| line == "job gio mount -e /media/128GB");
    assert!(
        quiesce.is_some() && quiesce < eject,
        "the releasing pane quiesces before the eject child starts: {log:?}"
    );
    assert_eq!(status(&shell, cx), "Ejecting 128GB. Do not unplug it yet.");
}

#[gpui::test]
fn release_keyboard_reaches_the_mark_and_return_ejects(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/fixture/work", cx);
    let stick = row(&shell, "128GB", cx);
    shell.update(cx, |shell, cx| shell.rail_focus(true, cx));
    frame(cx);
    let producer = producer(&shell, cx);
    for _ in 0..16 {
        if producer.read_with(cx, |p, _| p.cursor()) == Some(stick.index) {
            break;
        }
        cx.simulate_keystrokes("down");
        frame(cx);
    }
    assert_eq!(producer.read_with(cx, |p, _| p.cursor()), Some(stick.index));
    cx.simulate_keystrokes("right");
    frame(cx);
    producer.read_with(cx, |p, _| {
        let model = p.model.borrow();
        assert!(
            model.mark_focused && model.focused && !model.pointer_focus,
            "Right reaches the mark and its frame stands on the hit box"
        );
    });
    assert!(
        !jobs(&service)
            .iter()
            .any(|job| job.starts_with("gio mount -e")),
        "reaching the mark releases nothing"
    );
    cx.simulate_keystrokes("left");
    frame(cx);
    assert!(
        !producer.read_with(cx, |p, _| p.model.borrow().mark_focused),
        "Left leaves the mark"
    );
    cx.simulate_keystrokes("right enter");
    frame(cx);
    assert!(
        jobs(&service).contains(&"gio mount -e /media/128GB".to_string()),
        "Return on the mark ejects that mount: {:?}",
        jobs(&service)
    );
    assert!(
        !producer.read_with(cx, |p, _| p.model.borrow().mark_focused),
        "the release leaves the mark"
    );
}

#[gpui::test]
fn release_fixed_disk_keeps_its_square_and_ctrl_e_unmounts_it(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/media/vault/docs", cx);
    let vault = row(&shell, "Vault", cx);
    assert!(
        vault.indicator_visible && !vault.eject && vault.eject_hit().is_none(),
        "a fixed mounted volume keeps its square"
    );
    cx.simulate_mouse_down(
        centre(vault.indicator_rect),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        centre(vault.indicator_rect),
        MouseButton::Left,
        Modifiers::none(),
    );
    frame(cx);
    assert!(
        !jobs(&service)
            .iter()
            .any(|job| job.starts_with("gio mount -e") || job.starts_with("gio mount -u")),
        "its square is not a release control: {:?}",
        jobs(&service)
    );
    listing_focus(&shell, cx);
    cx.simulate_keystrokes("ctrl-e");
    frame(cx);
    let started = jobs(&service);
    assert!(
        started.contains(&"gio mount -u /media/vault".to_string())
            && !started.iter().any(|job| job.starts_with("gio mount -e")),
        "Ctrl+E unmounts the fixed volume and never ejects it: {started:?}"
    );
}

#[gpui::test]
fn release_refusal_draws_the_eject_refusal_sentence(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/fixture/work", cx);
    listing_focus(&shell, cx);
    cx.simulate_keystrokes("ctrl-e");
    assert_eq!(
        status(&shell, cx),
        "This folder is not on a drive Flea can eject."
    );
    assert!(
        !jobs(&service)
            .iter()
            .any(|job| job.starts_with("gio mount -e") || job.starts_with("gio mount -u")),
        "a refusal releases nothing"
    );
}

#[gpui::test]
fn release_quiesce_failure_is_said_and_the_eject_still_runs(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/media/128GB/photos", cx);
    let hit = row(&shell, "128GB", cx).eject_hit().unwrap();
    // The backend stops taking requests, so the pane's quiesce cannot be sent.
    LINES.with(|lines| *lines.borrow_mut() = None);
    cx.simulate_mouse_down(centre(hit), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(centre(hit), MouseButton::Left, Modifiers::none());
    frame(cx);
    assert!(
        jobs(&service).contains(&"gio mount -e /media/128GB".to_string()),
        "the release still runs after a failed quiesce: {:?}",
        jobs(&service)
    );
    assert_eq!(status(&shell, cx), "The backend stopped accepting requests");
}

#[gpui::test]
fn release_open_place_on_a_busy_pane_supersedes_and_never_refuses(cx: &mut TestAppContext) {
    // The phone and unlock opens ignore the step's result; this pins why that is safe.
    let (shell, _service, cx) = shell_at("/fixture/work", cx);
    assert!(shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.busy()));
    let result = shell.update_in(cx, |shell, window, cx| {
        let step = crate::window::navigate::Step::OpenPlace("/fixture/phone".into());
        shell.step(0, step, window, cx)
    });
    assert_eq!(result, Ok(true), "a busy pane takes the new place");
    let place = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.place().to_owned()
    });
    assert_eq!(place, "/fixture/phone");
}

#[gpui::test]
fn pb_rail_2_eject_holds_the_rail_while_its_child_runs(cx: &mut TestAppContext) {
    let (shell, _service, cx) = shell_at("/fixture/work", cx);
    let hit = row(&shell, "128GB", cx).eject_hit().unwrap();
    cx.simulate_mouse_down(centre(hit), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(centre(hit), MouseButton::Left, Modifiers::none());
    frame(cx);
    shell.read_with(cx, |shell, cx| {
        let join = shell.rail.as_ref().unwrap();
        assert!(
            join.devices.read(cx).devices.releasing(),
            "the eject child is in flight"
        );
        assert!(
            join.host.read(cx).rail.wants().releasing,
            "the live release owns the rail hold"
        );
    });
    // The hold's end waits on the post-exit listing that judges the eject; this test stops at the hold's start.
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[path = "rail_devices_pb_rest_tests.rs"]
mod pb_rest;

#[path = "rail_devices_drives_tests.rs"]
mod drives;
