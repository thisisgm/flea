//! Mounted window host: production Root windows, their IPC servers and their lifetimes.
use super::{
    input,
    wire::{object, text, word, STEP, WAIT},
};
use flea::jsondoc::{self, Json};
use flea_gui::{
    ipc::server::Server,
    palette::Theme,
    window::{
        self,
        lifecycle::Phase,
        locale::Snapshot,
        root::Root,
        startup::{Arguments, Launch, Settings},
    },
};
use gpui::{
    px, size, AppContext, Keystroke, Pixels, Size, TestAppContext, TestDispatcher, WindowHandle,
};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::Instant,
};

#[path = "report.rs"]
mod report;

// Every owned window opens at this size, so the controls' coordinates land on the same rows.
const WINDOW_SIZE: Size<Pixels> = size(px(1100.), px(700.));

pub(super) struct Mounted {
    window: WindowHandle<Root>,
    server: Server,
    identity: String,
    socket: PathBuf,
    draws: u64,
    captures: u64,
}

pub(super) struct Host {
    cx: TestAppContext,
    root: PathBuf,
    directory: File,
    pub(super) windows: BTreeMap<String, Mounted>,
    next: u64,
}

impl Drop for Host {
    fn drop(&mut self) {
        // Server shutdown must wake its listener before the directory alias closes.
        self.windows.clear();
    }
}

impl Host {
    pub(super) fn new() -> Self {
        Self::at(Path::new(
            &std::env::var_os("FLEA_UISH_ROOT").expect("owned root"),
        ))
    }

    pub(super) fn at(root: &Path) -> Self {
        let root = fs::canonicalize(root).unwrap();
        let worktree = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        assert!(root.starts_with(worktree.join(".superpowers/m9-uish-02")));
        assert!(root.join(".flea-test-sandbox").is_file());
        let metadata = fs::symlink_metadata(&root).unwrap();
        assert_eq!(metadata.uid(), flea::ipc::endpoint::uid());
        assert_eq!(metadata.mode() & 0o777, 0o700);
        let text_system = gpui_platform::current_platform(true).text_system();
        let mut cx =
            TestAppContext::build_with_text_system(TestDispatcher::new(0), None, text_system);
        cx.set_global(Theme::default());
        Self {
            cx,
            directory: File::open(&root).unwrap(),
            root,
            windows: BTreeMap::new(),
            next: 0,
        }
    }

    pub(super) fn pump(&mut self) {
        self.cx.executor().advance_clock(STEP);
        self.cx.run_until_parked();
        for mounted in self.windows.values_mut() {
            if self
                .cx
                .update_window(mounted.window.into(), |_, window, cx| {
                    window.refresh();
                    window.draw(cx).clear(cx);
                })
                .is_ok()
            {
                mounted.draws += 1;
            }
        }
        self.cx.run_until_parked();
    }

    fn open(&mut self, command: &Json) -> Result<Json, String> {
        let path = Path::new(word(command, "path")?)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !path.starts_with(&self.root) || !path.is_dir() {
            return Err("foreign fixture path".into());
        }
        self.next += 1;
        let runtime = self.root.join(format!("runtime/{}", self.next));
        fs::create_dir_all(&runtime).map_err(|e| e.to_string())?;
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        let alias = format!(
            "/proc/self/fd/{}/runtime/{}",
            self.directory.as_raw_fd(),
            self.next
        );
        let server = Server::bind(Path::new(&alias)).map_err(|e| e.to_string())?;
        let launch = Launch::resolve(
            Settings::from_json(jsondoc::parse(r#"{"view":"list"}"#)?),
            Arguments {
                path: Some(path.to_string_lossy().into_owned()),
                select: None,
            },
            &std::env::var("HOME").unwrap(),
        );
        let locale = Snapshot {
            revision: 1,
            catalog: Arc::new(crate::test_catalogs::english()),
        };
        let window = self
            .cx
            .update(|cx| window::open(launch, locale, server.events.clone(), cx))?;
        let identity = format!(
            "{}:{}:{}",
            self.root.file_name().unwrap().to_string_lossy(),
            std::process::id(),
            window.window_id().as_u64()
        );
        let socket = runtime.join(format!("flea-ipc/{}.sock", std::process::id()));
        self.cx.simulate_window_resize(window.into(), WINDOW_SIZE);
        self.windows.insert(
            identity.clone(),
            Mounted {
                window,
                server,
                identity: identity.clone(),
                socket,
                draws: 0,
                captures: 0,
            },
        );
        self.pump();
        self.describe(&identity)
    }

    pub(super) fn command(&mut self, command: &Json) -> Result<Json, String> {
        let op = word(command, "op")?;
        if op == "open" {
            return self.open(command);
        }
        let identity = word(command, "window")?;
        let window = self
            .windows
            .get(identity)
            .ok_or("foreign or closed window")?
            .window;
        if !self.cx.windows().contains(&window.into()) {
            self.forget(identity)?;
            return match op {
                "close" => Ok(object([("closed", Json::Bool(true))])),
                _ => Err("closed window".into()),
            };
        }
        match op {
            "capabilities" => return Ok(self.capture(identity, window)),
            "describe" => return self.describe(identity),
            "close" => {
                self.cx
                    .dispatch_keystroke(window.into(), Keystroke::parse("ctrl-q").unwrap());
                return self.wait_closed(identity, window);
            }
            _ => {
                if !input::deliver(&mut self.cx, window, op, command)? {
                    return Err("unknown control operation".into());
                }
            }
        }
        self.pump();
        // An input control may close its window, as ctrl-q does; its entry goes with it.
        if !self.cx.windows().contains(&window.into()) {
            return self.forget(identity);
        }
        if window
            .update(&mut self.cx, |root, _, cx| {
                root.shell.read(cx).lifecycle.phase()
            })
            .map_err(|error| error.to_string())?
            != Phase::Open
        {
            return self.wait_closed(identity, window);
        }
        self.describe(identity)
    }

    fn wait_closed(&mut self, identity: &str, window: WindowHandle<Root>) -> Result<Json, String> {
        let deadline = Instant::now() + WAIT;
        while self.cx.windows().contains(&window.into()) {
            if Instant::now() >= deadline {
                return Err("owned window close timed out".into());
            }
            self.pump();
            thread::sleep(STEP);
        }
        self.forget(identity)
    }

    // Removes a window behind the host's back, as a crash or a compositor close would.
    pub(super) fn remove_outside_command(&mut self, identity: &str) {
        let window = self.windows[identity].window;
        self.cx
            .update_window(window.into(), |_, window, _| window.remove_window())
            .unwrap();
        self.cx.run_until_parked();
        assert!(!self.cx.windows().contains(&window.into()));
    }

    // Closes every window still owned, so a replay ends with no window and no socket left.
    pub(super) fn teardown(&mut self) -> Result<(), String> {
        let identities: Vec<_> = self.windows.keys().cloned().collect();
        for identity in identities {
            self.command(&object([("op", text("close")), ("window", text(identity))]))?;
        }
        Ok(())
    }

    // Drops a closed window's entry and its server, which must take the socket with it.
    fn forget(&mut self, identity: &str) -> Result<Json, String> {
        let mounted = self
            .windows
            .remove(identity)
            .ok_or("foreign or closed window")?;
        let socket = mounted.socket.clone();
        drop(mounted.server);
        if socket.exists() {
            return Err("owned socket survived teardown".into());
        }
        Ok(object([("closed", Json::Bool(true))]))
    }
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    use std::{cell::Cell, panic::AssertUnwindSafe, rc::Rc, sync::mpsc};

    #[test]
    fn uish_driver_key_close_waits_for_a_later_drain() {
        let sandbox = super::super::Sandbox::new("delayed-close");
        let fixture = sandbox.0.join("fixture");
        fs::create_dir(&fixture).unwrap();
        fs::write(fixture.join("a.txt"), "a").unwrap();
        let mut host = Host::at(&sandbox.0);
        let opened = host
            .command(&object([
                ("op", text("open")),
                ("path", text(fixture.display())),
            ]))
            .unwrap();
        let identity = word(&opened, "window").unwrap();
        let socket = PathBuf::from(word(&opened, "socket").unwrap());
        let window = host.windows[identity].window;
        window
            .update(&mut host.cx, |root, _, cx| {
                root.shell.update(cx, |shell, _| {
                    assert!(shell.lifecycle.attach(1));
                });
            })
            .unwrap();
        let released = Rc::new(Cell::new(false));
        let observed = released.clone();
        // The extra drain slot keeps the window mounted beyond command's first pump.
        let later = host.cx.executor().timer(STEP * 3);
        host.cx
            .spawn(|mut cx| async move {
                later.await;
                window
                    .update(&mut cx, |root, window, cx| {
                        root.shell.update(cx, |shell, cx| {
                            assert_eq!(shell.lifecycle.phase(), Phase::Draining);
                            observed.set(true);
                            shell.lifecycle.drained(1);
                            shell.close(window, cx);
                        });
                    })
                    .unwrap();
            })
            .detach();
        host.cx.run_until_parked();
        let closed = host
            .command(&object([
                ("op", text("key")),
                ("key", text("ctrl-q")),
                ("window", text(identity)),
            ]))
            .unwrap();
        assert_eq!(closed, object([("closed", Json::Bool(true))]));
        assert!(released.get(), "close must await the later drain");
        assert!(host.windows.is_empty());
        assert!(!socket.exists(), "the socket must retire with its window");
    }

    #[test]
    fn uish_driver_panicking_host_retires_its_socket_within_the_close_bound() {
        let (send, receive) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let sandbox = super::super::Sandbox::new("panic-close");
            let fixture = sandbox.0.join("fixture");
            fs::create_dir(&fixture).unwrap();
            fs::write(fixture.join("a.txt"), "a").unwrap();
            let mut socket = None;
            let panic = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let mut host = Host::at(&sandbox.0);
                let opened = host
                    .command(&object([
                        ("op", text("open")),
                        ("path", text(fixture.display())),
                    ]))
                    .unwrap();
                socket = Some(PathBuf::from(
                    opened.get("socket").and_then(Json::as_str).unwrap(),
                ));
                panic!("exercise mounted-host unwinding");
            }));
            let _ = send.send((panic.is_err(), socket.is_some_and(|path| !path.exists())));
        });
        let (panicked, socket_gone) = receive
            .recv_timeout(WAIT)
            .expect("mounted-host unwind exceeded its existing close bound");
        assert!(panicked, "the regression must exercise unwinding");
        assert!(socket_gone, "unwinding must retire the mounted IPC socket");
        worker.join().unwrap();
    }
}
