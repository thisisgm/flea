//! A mounted stick in the native Root window: Return opens its mount and m paints the device menu, by real keys.
use super::*;
use crate::window::{
    locale::Snapshot,
    startup::{Arguments, Launch, Settings},
};
use flea::jsondoc;
use gpui::RequestFrameOptions;

type Receive = Box<dyn FnMut() -> Option<String>>;

thread_local! {
    // The backend's request lines stay open for the window's life.
    static LINES: RefCell<Vec<Receive>> = RefCell::default();
}

const BLOCKS: &str = r#"{"blockdevices":[
{"name":"nvme0n1","path":"/dev/nvme0n1","type":"disk","children":[{"name":"nvme0n1p1","type":"part","mountpoints":["/"],"fstype":"ext4"}]},
{"name":"sda1","path":"/dev/sda1","type":"part","label":"128GB","mountpoints":["/run/media/gm/128GB"],"fstype":"exfat","rm":true,"size":128000000000}
]}"#;

fn root_at(cx: &mut TestAppContext) -> (Entity<Shell>, Rc<FakeDevices>, &mut VisualTestContext) {
    let service = Rc::new(FakeDevices::default());
    let runner = service.clone();
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, _| {
                let (backend, lines) = crate::backend::Backend::test_lines();
                LINES.with(|kept| kept.borrow_mut().push(Box::new(lines)));
                Ok(backend)
            },
            monitor: None,
        });
        cx.set_global(crate::g2::pl1::source::fixture::Services {
            runner,
            hostname: "minipc\n".into(),
        });
    });
    let state = jsondoc::parse(
        r#"{"view":"list","keys":"default","places":{"rail":"shown","sidebarWidth":192}}"#,
    )
    .unwrap();
    let launch = Launch::resolve(
        Settings::from_json(state),
        Arguments {
            path: Some("/fixture/work".into()),
            select: None,
        },
        "/fixture/home",
    );
    let locale = Snapshot {
        revision: 1,
        catalog: Arc::new(crate::test_catalogs::english()),
    };
    let (_sender, events) = async_channel::unbounded();
    let (root, cx) = cx.add_window_view(|window, cx| crate::window::root::Root {
        shell: cx.new(|cx| Shell::new(launch, locale, events, window, cx)),
    });
    let shell = root.read_with(cx, |root, _| root.shell.clone());
    cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    cx.run_until_parked();
    service.answer("lsblk", 0, BLOCKS);
    service.answer("gio", 0, "");
    frame(cx);
    cx.executor().advance_clock(Duration::from_millis(900));
    frame(cx);
    // The fixture backend never answers its first List, so the pane is free for the next open.
    shell.update(cx, |shell, _| {
        if let Some(host) = &mut shell.panes[0] {
            host.pane.swap.pending = None;
        }
    });
    frame(cx);
    (shell, service, cx)
}

// Only the native frame callback draws, and it draws only a window something marked dirty.
fn native_frames(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.simulate_frame_request(RequestFrameOptions::default());
    }
    cx.run_until_parked();
}

// Tab into the rail and walk down to the stick, as the sweep does.
fn walk_to_stick(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
) -> crate::g2::pl4::rows::LaidRow {
    cx.simulate_keystrokes("tab");
    frame(cx);
    let producer = producer(shell, cx);
    let stick = producer.read_with(cx, |producer, _| {
        producer
            .laid
            .borrow()
            .rows
            .iter()
            .find(|row| row.label == "128GB")
            .cloned()
            .expect("the rail paints the 128GB stick")
    });
    for _ in 0..16 {
        if producer.read_with(cx, |p, _| p.cursor()) == Some(stick.index) {
            break;
        }
        cx.simulate_keystrokes("down");
        frame(cx);
    }
    assert_eq!(producer.read_with(cx, |p, _| p.cursor()), Some(stick.index));
    stick
}

#[gpui::test]
fn raildrive_return_on_a_mounted_stick_opens_its_mount(cx: &mut TestAppContext) {
    let (shell, _service, cx) = root_at(cx);
    walk_to_stick(&shell, cx);
    cx.simulate_keystrokes("enter");
    native_frames(cx);
    let asked = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.place().to_owned()
    });
    assert_eq!(asked, "/run/media/gm/128GB", "Return opens the stick");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn raildrive_menu_key_on_a_mounted_stick_paints_the_device_menu(cx: &mut TestAppContext) {
    let (shell, _service, cx) = root_at(cx);
    let stick = walk_to_stick(&shell, cx);
    crate::g2::mn1::element::PAINTED.with(|painted| painted.borrow_mut().clear());
    cx.simulate_keystrokes("m");
    native_frames(cx);
    let actions = shell.read_with(cx, |shell, _| {
        shell
            .menu
            .menu
            .borrow()
            .card
            .as_ref()
            .map(|card| {
                card.entries
                    .iter()
                    .filter(|entry| !entry.separator)
                    .map(|entry| entry.id.to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    assert_eq!(
        actions,
        ["open", "unmountVolume", "eject", "flashImage"],
        "rail-menu-device rows"
    );
    let painted = crate::g2::mn1::element::PAINTED.with(|painted| painted.borrow().clone());
    let frame = painted
        .last()
        .expect("the device menu is painted in a native frame");
    let top = f32::from(frame.origin.y);
    let bottom = stick.rect.y + stick.rect.height;
    assert!(
        (top - bottom).abs() < 12.,
        "the menu opens under the stick: top {top}, row bottom {bottom}"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn raildrive_device_menu_eject_runs_on_the_stick(cx: &mut TestAppContext) {
    let (shell, service, cx) = root_at(cx);
    walk_to_stick(&shell, cx);
    cx.simulate_keystrokes("m");
    native_frames(cx);
    // Open, then Unmount, then Eject: the cursor walks the live rows only.
    cx.simulate_keystrokes("down down enter");
    for _ in 0..3 {
        frame(cx);
    }
    let jobs: Vec<String> = service
        .jobs
        .borrow()
        .iter()
        .map(|(argv, _, _)| argv.join(" "))
        .collect();
    assert!(
        jobs.iter()
            .any(|job| job == "gio mount -e /run/media/gm/128GB"),
        "Eject releases the stick: {jobs:?}"
    );
    assert!(
        !shell.read_with(cx, |shell, _| shell.menu_opened()),
        "the choice closes the menu"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
