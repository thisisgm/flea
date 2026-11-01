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
    root_with(cx, r#"{"rail":"shown","sidebarWidth":192}"#, BLOCKS)
}

// The window over a places object and an lsblk answer of the test's own.
fn root_with<'a>(
    cx: &'a mut TestAppContext,
    places: &str,
    blocks: &str,
) -> (Entity<Shell>, Rc<FakeDevices>, &'a mut VisualTestContext) {
    root_devices(cx, places, blocks, "")
}

// The window over a places object, an lsblk answer and a gio volume answer of the test's own.
fn root_devices<'a>(
    cx: &'a mut TestAppContext,
    places: &str,
    blocks: &str,
    phone: &str,
) -> (Entity<Shell>, Rc<FakeDevices>, &'a mut VisualTestContext) {
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
    let state = jsondoc::parse(&format!(
        r#"{{"view":"list","keys":"default","places":{places}}}"#
    ))
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
    service.answer("lsblk", 0, blocks);
    service.answer("gio", 0, phone);
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
    walk_to(shell, "128GB", cx)
}

// Tab into the rail and walk down to the row with this label, as the sweep does.
fn walk_to(
    shell: &Entity<Shell>,
    label: &str,
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
            .find(|row| row.label == label)
            .cloned()
            .unwrap_or_else(|| panic!("the rail paints the {label} row"))
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

// The ids of the rows the open menu holds, separators dropped.
fn menu_rows(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<String> {
    shell.read_with(cx, |shell, _| {
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
    })
}

// Every job the fake device runner started, one argv line each.
fn argv_lines(service: &FakeDevices) -> Vec<String> {
    service
        .jobs
        .borrow()
        .iter()
        .map(|(argv, _, _)| argv.join(" "))
        .collect()
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
    cx.update(|cx| cx.set_reduce_motion(true));
    let (shell, _service, cx) = root_at(cx);
    let stick = walk_to_stick(&shell, cx);
    crate::g2::mn1::element::PAINTED.with(|painted| painted.borrow_mut().clear());
    cx.simulate_keystrokes("m");
    native_frames(cx);
    assert_eq!(
        menu_rows(&shell, cx),
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
    let jobs = argv_lines(&service);
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

#[path = "rail_drive_menu_tests.rs"]
mod menu;

#[path = "rail_drive_rows_tests.rs"]
mod rows;

#[path = "rail_drive_focus_tests.rs"]
mod focus;

fn ipc(
    shell: &Entity<Shell>,
    name: &str,
    args: &[&str],
    cx: &mut VisualTestContext,
) -> flea::ipc::wire::Reply {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            // The fixture backend never answers its first List, so the readers are opened as a landed listing opens them.
            shell.ipc_cold = false;
            shell.ipc(
                crate::ipc::server::Event::Call {
                    request: flea::ipc::wire::Request::Call {
                        target: "flea".into(),
                        function: name.into(),
                        arguments: args.iter().map(|arg| (*arg).into()).collect(),
                    },
                    answer,
                },
                window,
                cx,
            )
        })
    });
    receive
        .try_recv()
        .expect("the mounted shell answered")
        .reply
}

// EY-338: railEntries carries v0.3.12's volume facts (Devices.js volumeRow), so a scene can tell a locked container's row.
#[gpui::test]
fn raildrive_rail_entries_carry_locked_fstype_and_size(cx: &mut TestAppContext) {
    const LOCKED: &str = r#"{"blockdevices":[
{"name":"nvme0n1","path":"/dev/nvme0n1","type":"disk","size":512000000000,"children":[{"name":"nvme0n1p1","type":"part","mountpoints":["/"],"fstype":"ext4"}]},
{"name":"sda1","path":"/dev/sda1","type":"part","label":"128GB","mountpoints":["/run/media/gm/128GB"],"fstype":"exfat","rm":true,"size":128000000000},
{"name":"sdb1","path":"/dev/sdb1","type":"part","label":"FLEALUKS","mountpoints":[null],"fstype":"crypto_LUKS","fsver":"2","rm":true,"size":64000000}
]}"#;
    let (shell, _service, cx) = root_with(cx, r#"{"rail":"shown","sidebarWidth":192}"#, LOCKED);
    let reply = ipc(&shell, "railEntries", &[], cx);
    assert_eq!(reply.code, 0, "{reply:?}");
    let entries = jsondoc::parse(reply.stdout.trim()).unwrap();
    let row = |label: &str| {
        entries
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry.get("label").and_then(jsondoc::Json::as_str) == Some(label))
            .unwrap_or_else(|| panic!("{label} in {}", reply.stdout))
            .clone()
    };
    let locked = row("FLEALUKS");
    assert_eq!(
        locked.get("glyph").and_then(jsondoc::Json::as_str),
        Some("lock")
    );
    assert_eq!(locked.get("locked"), Some(&jsondoc::Json::Bool(true)));
    assert_eq!(
        locked.get("fstype").and_then(jsondoc::Json::as_str),
        Some("crypto_LUKS")
    );
    assert_eq!(
        locked.get("size").and_then(jsondoc::Json::as_f64),
        Some(64000000.)
    );
    let stick = row("128GB");
    assert_eq!(stick.get("locked"), Some(&jsondoc::Json::Bool(false)));
    assert_eq!(
        stick.get("fstype").and_then(jsondoc::Json::as_str),
        Some("exfat")
    );
    assert_eq!(
        stick.get("size").and_then(jsondoc::Json::as_f64),
        Some(128000000000.)
    );
    assert!(
        !reply.stdout.trim_end().contains('\n'),
        "one line, as before"
    );
}

// EY-338: railIconSize answers the rail icon slot, the value tokens already names railIconSize.
#[gpui::test]
fn raildrive_rail_icon_size_answers_the_token(cx: &mut TestAppContext) {
    let (shell, _service, cx) = root_at(cx);
    let (theme, parsed) =
        crate::palette::Theme::from_text_observed("background='#101315'\nforeground='#eeeeee'");
    assert!(parsed.0);
    let snapshot = crate::presentation::Snapshot {
        theme,
        parsed,
        fonts: crate::presentation::Fonts::from_sources("", "", "monospace".into()),
    };
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.apply_presentation(snapshot, window, cx)
        });
    });
    frame(cx);
    let reply = ipc(&shell, "railIconSize", &[], cx);
    assert_eq!(reply.code, 0, "{reply:?}");
    let size: f64 = reply.stdout.trim().parse().unwrap();
    assert!(size > 0., "{size}");
    let tokens = ipc(&shell, "tokens", &[], cx);
    assert_eq!(tokens.code, 0, "{tokens:?}");
    // Sample token line: railIconSize=16
    let token_size = tokens
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("railIconSize="))
        .expect("tokens includes railIconSize")
        .parse::<f64>()
        .expect("railIconSize token is numeric");
    assert_eq!(token_size, size);
}
