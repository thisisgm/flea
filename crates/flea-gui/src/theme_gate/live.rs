//! Real startup, backend, source watcher and IPC on GPUI's headless test platform.
use super::frame::{Frame, Frames};
use crate::{
    ipc::server::Server,
    palette::Theme,
    presentation::Service,
    state::Shell,
    window::{
        self,
        root::Root,
        startup::{Arguments, Launch, Settings},
    },
};
use flea::ipc::wire::Reply;
use gpui::{AppContext, Context, TestAppContext, Window, WindowHandle};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

type Handle = WindowHandle<Root>;

fn edit<R>(
    cx: &mut TestAppContext,
    handle: Handle,
    change: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| change(shell, window, cx))
        })
        .unwrap()
}

fn draw(cx: &mut TestAppContext, handle: Handle) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    edit(cx, handle, |shell, window, cx| {
        super::quads::assert(shell, window, cx)
    });
}

fn wait(cx: &mut TestAppContext, handle: Handle, ready: impl Fn(&mut TestAppContext) -> bool) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
        draw(cx, handle);
        if ready(cx) {
            return;
        }
        assert!(Instant::now() < until, "headless app did not settle");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn ipc(cx: &mut TestAppContext, handle: Handle, name: &str) -> Reply {
    // The shipped client crosses the real socket and UI event queue, as tests/gui-ipc.sh does.
    let mut child = std::process::Command::new(std::env::var_os("FLEA_BIN").unwrap())
        .args([
            "ipc",
            "--pid",
            &std::process::id().to_string(),
            "call",
            "flea",
            name,
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(4);
    while child.try_wait().unwrap().is_none() {
        cx.run_until_parked();
        draw(cx, handle);
        if Instant::now() >= until {
            child.kill().unwrap();
            panic!("the real IPC client timed out reading {name}");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    Reply {
        code: output.status.code().unwrap(),
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}

fn assert_frame(frame: &Frame, theme: Theme) {
    assert_eq!(frame.theme, theme);
    assert_eq!(frame.panes.len(), 2, "both live panes must paint");
    for pane in &frame.panes {
        assert_eq!(pane.background, theme.background, "mixed pane ground");
        assert_eq!(pane.foreground, theme.foreground, "mixed pane ink");
        assert_eq!(pane.metadata, theme.metadata, "mixed pane metadata");
        assert_eq!(pane.accent_frame, theme.accent_frame, "mixed pane frame");
    }
    let rail = frame.rail.expect("the production rail painted");
    assert_eq!(rail.background, theme.surface, "mixed rail ground");
    assert_eq!(rail.foreground, theme.foreground, "mixed rail ink");
    assert_eq!(rail.metadata, theme.metadata, "mixed rail caption");
    assert_eq!(frame.footer, Some(theme.foreground), "mixed footer ink");
    let card = frame.card.expect("the open copy card painted");
    assert_eq!(card.surface, theme.surface, "mixed card ground");
    assert_eq!(card.foreground, theme.foreground, "mixed card ink");
    assert_eq!(card.muted, theme.muted, "mixed card caption");
    assert_eq!(card.accent_frame, theme.accent_frame, "mixed card frame");
}

fn replace(current: &Path, target: &Path) {
    let next = current.with_file_name("next-theme");
    std::os::unix::fs::symlink(target, &next).unwrap();
    // rename(2) replaces the live link atomically, as Omarchy's current/next-theme staging does.
    std::fs::rename(next, current).unwrap();
}

#[test]
#[ignore = "needs stock Omarchy themes and a built flea; run by tests/gpui-theme-gate.sh"]
fn theme_gate_live_switch_whole_window() {
    // Direct cargo runs also enter a fresh process before any application constructor reads HOME.
    if std::env::var_os("FLEA_THEME_GATE_ROOT").is_none() {
        super::isolation::isolated();
        return;
    }
    // The suite supplies a mktemp root and a child-only HOME. Never mutate the operator environment.
    let root = PathBuf::from(
        std::env::var_os("FLEA_THEME_GATE_ROOT")
            .expect("run tests/gpui-theme-gate.sh for its guarded sandbox HOME"),
    );
    assert!(
        root.is_absolute() && root.starts_with("/tmp") && root.join(".flea-test-sandbox").is_file()
    );
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    assert_eq!(home, root.join("home"));
    assert!(home.join(".flea-test-sandbox").is_file());
    let current = home.join(".local/state/omarchy/current/theme");
    std::fs::create_dir_all(current.parent().unwrap()).unwrap();
    let themes: Vec<_> = ["catppuccin", "catppuccin-latte", "solitude"]
        .into_iter()
        .map(|name| {
            let dir = root.join("palettes").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            let body =
                std::fs::read_to_string(super::stock_dir().join(name).join("colors.toml")).unwrap();
            std::fs::write(dir.join("colors.toml"), &body).unwrap();
            // Fonts remain fixed: this test isolates a palette change.
            std::fs::write(dir.join("shell.toml"), "[font]\nbase-size=14\n").unwrap();
            (name, dir, Theme::from_text(&body))
        })
        .collect();
    replace(&current, &themes[0].1);
    let files = root.join("files");
    std::fs::create_dir(&files).unwrap();
    for n in 0..100 {
        std::fs::write(files.join(format!("file-{n:03}.txt")), "theme gate\n").unwrap();
    }
    let runtime = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap());
    let server = Server::bind(&runtime).unwrap();
    let (source, initial) = Service::start();
    assert_eq!(initial.theme, themes[0].2);
    let updates = source.subscribe();
    assert_eq!(updates.try_recv().unwrap().theme, initial.theme);
    let mut cx = crate::list::bench::real_cx();
    cx.update(|cx| {
        initial.install(cx);
        cx.set_reduce_motion(true);
    });
    let settings = Settings::from_json(
        flea::jsondoc::parse(r#"{"view":"dual","places":{"sidebar":"docked"}}"#).unwrap(),
    );
    let launch = Launch::resolve(
        settings,
        Arguments {
            path: Some(files.to_str().unwrap().into()),
            select: None,
        },
        home.to_str().unwrap(),
    );
    let locale = window::locale::Snapshot {
        revision: 1,
        catalog: Arc::new(crate::test_catalogs::english()),
    };
    let handle = cx.update(|cx| window::open(launch, locale, server.events.clone(), cx).unwrap());
    edit(&mut cx, handle, |shell, window, cx| {
        shell.follow_presentation(&source, window, cx)
    });
    wait(&mut cx, handle, |cx| {
        edit(cx, handle, |shell, _, _| {
            shell.panes.iter().flatten().count() == 2
                && shell
                    .panes
                    .iter()
                    .flatten()
                    .all(|host| host.answered && host.pane.held.total == 100)
        })
    });
    edit(&mut cx, handle, |shell, window, cx| {
        for host in shell.panes.iter_mut().flatten() {
            host.pane.cursor = 22;
            host.pane.scroll_to(12. * host.pane.row_height());
            host.selected = vec![21, 23];
            host.input.selection.sync(&host.selected);
        }
        shell.open_overlay_copy_to(false, files.to_str().unwrap(), window, cx);
    });
    // Allow the modal to finish opening before watching every paint, including implicit effect-flush draws.
    cx.run_until_parked();
    draw(&mut cx, handle);
    let identity = edit(&mut cx, handle, |shell, _, _| {
        shell.overlay.as_ref().unwrap().entity_id()
    });
    let kept = edit(&mut cx, handle, |shell, _, _| {
        shell
            .panes
            .iter()
            .flatten()
            .map(|host| {
                (
                    host.path.clone(),
                    host.pane.cursor,
                    host.pane.scroll_y,
                    host.selected.clone(),
                    host.pane.held.listing,
                )
            })
            .collect::<Vec<_>>()
    });
    let pid = std::process::id();
    cx.update(|cx| cx.set_global(Frames::default()));
    draw(&mut cx, handle);
    let mut previous = themes[0].2;
    // Dark to light, light to dark, then low chroma and back to dark.
    for target in [1, 0, 2, 0] {
        let (name, directory, expected) = &themes[target];
        cx.update(|cx| cx.global::<Frames>().frames.borrow_mut().clear());
        let publications = source.publications();
        replace(&current, directory);
        wait(&mut cx, handle, |cx| {
            cx.read(|cx| cx.global::<Theme>() == expected)
        });
        let first = updates
            .try_recv()
            .expect("the presentation source must fire");
        assert_eq!(first.theme, *expected);
        assert!(updates.try_recv().is_err(), "duplicate source delivery");
        // More than two source polling periods: a second stat observation must publish nothing.
        let until = Instant::now() + Duration::from_millis(1150);
        while Instant::now() < until {
            cx.run_until_parked();
            draw(&mut cx, handle);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            updates.try_recv().is_err(),
            "source fired twice for one atomic replacement"
        );
        assert_eq!(
            source.publications(),
            publications + 1,
            "the source must publish exactly once, including coalesced deliveries"
        );
        cx.read(|cx| {
            let audit = cx.global::<Frames>();
            let frames = audit.frames.borrow();
            assert!(!frames.is_empty());
            let mut changed = false;
            for frame in frames.iter() {
                if frame.theme == *expected {
                    changed = true;
                }
                assert_frame(frame, if changed { *expected } else { previous });
            }
            assert!(changed, "no frame painted the new palette");
            println!(
                "THEME_GATE LIVE {name} source=1 atomic_frames={} pid={pid}",
                frames.len()
            );
        });
        assert_eq!(
            ipc(&mut cx, handle, "themeForeground"),
            Reply::value(crate::ipc::rows::colour(expected.foreground))
        );
        assert_eq!(
            ipc(&mut cx, handle, "selectedIndices"),
            Reply::value("21,23")
        );
        assert_eq!(
            ipc(&mut cx, handle, "path"),
            Reply::value(files.to_str().unwrap())
        );
        edit(&mut cx, handle, |shell, _, _| {
            assert_eq!(
                shell.overlay.as_ref().unwrap().entity_id(),
                identity,
                "the open card restarted"
            );
            let now = shell
                .panes
                .iter()
                .flatten()
                .map(|host| {
                    (
                        host.path.clone(),
                        host.pane.cursor,
                        host.pane.scroll_y,
                        host.selected.clone(),
                        host.pane.held.listing,
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(
                now, kept,
                "a theme switch changed folder, cursor, scroll, marks or listing generation"
            );
        });
        assert_eq!(std::process::id(), pid);
        previous = *expected;
    }
    assert_eq!(
        ipc(&mut cx, handle, "presentedFrame").code,
        2,
        "the test platform must not invent compositor receipts"
    );
    // The real socket server is alive for this same application process throughout the switch.
    assert!(
        std::fs::symlink_metadata(runtime.join(format!("flea-ipc/{pid}.sock")))
            .unwrap()
            .file_type()
            .is_socket()
    );
    edit(&mut cx, handle, |shell, window, cx| shell.close(window, cx));
    println!("THEME_GATE LIVE DONE transitions=4 failed=0");
}

use std::os::unix::fs::FileTypeExt;
