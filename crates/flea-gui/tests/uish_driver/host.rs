//! Mounted window host: production Root windows, their IPC servers and test input delivery.
use flea::{
    i18n::I18n,
    jsondoc::{self, Json},
};
use flea_gui::{
    ipc::server::Server,
    palette::Theme,
    window::{
        self,
        locale::Snapshot,
        root::Root,
        startup::{Arguments, Launch, Settings},
    },
};
use gpui::{
    point, px, size, AppContext, ClipboardItem, Keystroke, Modifiers, MouseButton, ScrollDelta,
    ScrollWheelEvent, TestAppContext, TestDispatcher, TouchPhase, VisualTestContext, WindowHandle,
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
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(10);

pub(super) fn object(fields: impl IntoIterator<Item = (&'static str, Json)>) -> Json {
    Json::Obj(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
pub(super) fn text(value: impl ToString) -> Json {
    Json::Str(value.to_string())
}
pub(super) fn number(value: impl ToString) -> Json {
    Json::Num(value.to_string())
}
fn word<'a>(value: &'a Json, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| format!("missing string {key}"))
}
fn coordinate(value: &Json, key: &str) -> Result<f32, String> {
    value
        .get(key)
        .and_then(Json::as_f64)
        .filter(|n| n.is_finite() && n.abs() <= 100_000.)
        .map(|n| n as f32)
        .ok_or_else(|| format!("invalid coordinate {key}"))
}

pub(super) struct Mounted {
    window: WindowHandle<Root>,
    server: Server,
    identity: String,
    socket: PathBuf,
    draws: u64,
}

pub(super) struct Host {
    cx: TestAppContext,
    root: PathBuf,
    directory: File,
    pub(super) windows: BTreeMap<String, Mounted>,
    next: u64,
}

impl Host {
    pub(super) fn new() -> Self {
        let root =
            fs::canonicalize(std::env::var_os("FLEA_UISH_ROOT").expect("owned root")).unwrap();
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
        self.cx.executor().advance_clock(Duration::from_millis(2));
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
            Settings::from_json(jsondoc::parse(r#"{"view":"list"}"#)?, true),
            Arguments {
                path: Some(path.to_string_lossy().into_owned()),
                select: None,
            },
            &std::env::var("HOME").unwrap(),
        );
        let locale = Snapshot {
            revision: 1,
            catalog: Arc::new(I18n::default()),
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
        self.cx
            .simulate_window_resize(window.into(), size(px(1100.), px(700.)));
        self.windows.insert(
            identity.clone(),
            Mounted {
                window,
                server,
                identity: identity.clone(),
                socket,
                draws: 0,
            },
        );
        self.pump();
        self.describe(&identity)
    }

    fn describe(&mut self, identity: &str) -> Result<Json, String> {
        let mounted = self
            .windows
            .get(identity)
            .ok_or("foreign or closed window")?;
        let metadata = fs::symlink_metadata(&mounted.socket).map_err(|e| e.to_string())?;
        let active = self
            .cx
            .update(|cx| mounted.window.is_active(cx))
            .ok_or("closed window")?;
        Ok(object([
            ("window", text(&mounted.identity)),
            ("active", Json::Bool(active)),
            ("gpui_window", number(mounted.window.window_id().as_u64())),
            ("pid", number(std::process::id())),
            ("root_type", text(std::any::type_name::<Root>())),
            ("socket", text(mounted.socket.display())),
            (
                "socket_identity",
                Json::Arr(vec![number(metadata.dev()), number(metadata.ino())]),
            ),
            (
                "runtime",
                text(format!(
                    "/proc/{}/fd/{}/runtime",
                    std::process::id(),
                    self.directory.as_raw_fd()
                )),
            ),
            ("draw_calls", number(mounted.draws)),
        ]))
    }

    pub(super) fn command(&mut self, command: &Json) -> Result<Json, String> {
        let op = word(command, "op")?;
        if op == "open" {
            return self.open(command);
        }
        let identity = word(command, "window")?;
        let mounted = self
            .windows
            .get(identity)
            .ok_or("foreign or closed window")?;
        let window = mounted.window;
        if !self.cx.windows().contains(&window.into()) {
            return Err("closed window".into());
        }
        let mut visual = VisualTestContext::from_window(window.into(), &self.cx);
        match op {
            "text" => {
                for character in word(command, "text")?.chars() {
                    self.cx.dispatch_keystroke(
                        window.into(),
                        Keystroke {
                            key: "text".into(),
                            key_char: Some(character.to_string()),
                            ..Default::default()
                        },
                    );
                }
            }
            "key" | "paste" => {
                let mut key = match op {
                    "paste" => {
                        self.cx.write_to_clipboard(ClipboardItem::new_string(
                            word(command, "text")?.into(),
                        ));
                        Keystroke::parse(
                            command
                                .get("key")
                                .and_then(Json::as_str)
                                .unwrap_or("ctrl-v"),
                        )
                        .map_err(|e| e.to_string())?
                    }
                    _ => Keystroke::parse(word(command, "key")?).map_err(|e| e.to_string())?,
                };
                if op == "key"
                    && key.key.chars().count() == 1
                    && key.modifiers == Modifiers::default()
                {
                    key.key_char = Some(key.key.clone());
                }
                self.cx.dispatch_keystroke(window.into(), key);
            }
            "mouse" => {
                let position = point(px(coordinate(command, "x")?), px(coordinate(command, "y")?));
                let modifiers = Keystroke::parse(
                    command
                        .get("modifiers")
                        .and_then(Json::as_str)
                        .unwrap_or("a"),
                )
                .map_err(|e| e.to_string())?
                .modifiers;
                let button = match command
                    .get("button")
                    .and_then(Json::as_str)
                    .unwrap_or("left")
                {
                    "left" => MouseButton::Left,
                    "right" => MouseButton::Right,
                    "middle" => MouseButton::Middle,
                    _ => return Err("invalid mouse button".into()),
                };
                match word(command, "phase")? {
                    "move" => visual.simulate_mouse_move(position, None, modifiers),
                    "down" => visual.simulate_mouse_down(position, button, modifiers),
                    "up" => visual.simulate_mouse_up(position, button, modifiers),
                    "click" => {
                        visual.simulate_mouse_down(position, button, modifiers);
                        visual.simulate_mouse_up(position, button, modifiers);
                    }
                    _ => return Err("invalid mouse phase".into()),
                }
            }
            "wheel" => visual.simulate_event(ScrollWheelEvent {
                position: point(px(coordinate(command, "x")?), px(coordinate(command, "y")?)),
                delta: ScrollDelta::Pixels(point(
                    px(coordinate(command, "dx")?),
                    px(coordinate(command, "dy")?),
                )),
                modifiers: Modifiers::default(),
                touch_phase: TouchPhase::Moved,
            }),
            "focus" => match command.get("active").and_then(Json::as_bool) {
                Some(true) => visual.update(|window, _| window.activate_window()),
                Some(false) => visual.deactivate_window(),
                None => return Err("missing boolean active".into()),
            },
            "capabilities" => {
                return Ok(visual.update(|window, _| {
                    let raster = match window.render_to_image() {
                        Ok(image) => {
                            let file = self.root.join(format!("capture-{}.rgba", self.next));
                            match fs::write(&file, image.as_raw()) {
                                Ok(()) => object([
                                    ("status", text("available")),
                                    ("path", text(file.display())),
                                    ("width", number(image.width())),
                                    ("height", number(image.height())),
                                ]),
                                Err(error) => object([
                                    ("status", text("unsupported")),
                                    ("reason", text(error)),
                                ]),
                            }
                        }
                        Err(error) => {
                            object([("status", text("unsupported")), ("reason", text(error))])
                        }
                    };
                    object([
                        ("raster", raster),
                        ("window_events", text("available")),
                        ("production_ipc", text("available")),
                        ("native_input", text("unsupported")),
                        ("hyprland_client", text("unsupported")),
                        ("wayland_dnd", text("unsupported")),
                        ("media_frames", text("unsupported")),
                        ("presented_frame_timing", text("unsupported")),
                        (
                            "executor_clock",
                            text("GPUI test clock; advanced in 2 ms steps"),
                        ),
                    ])
                }))
            }
            "describe" => return self.describe(identity),
            "close" => {
                self.cx
                    .dispatch_keystroke(window.into(), Keystroke::parse("ctrl-q").unwrap());
                let deadline = Instant::now() + WAIT;
                while self.cx.windows().contains(&window.into()) {
                    if Instant::now() >= deadline {
                        return Err("owned window close timed out".into());
                    }
                    self.pump();
                    thread::sleep(Duration::from_millis(2));
                }
                let mounted = self.windows.remove(identity).unwrap();
                let socket = mounted.socket.clone();
                drop(mounted.server);
                if socket.exists() {
                    return Err("owned socket survived teardown".into());
                }
                return Ok(object([("closed", Json::Bool(true))]));
            }
            _ => return Err("unknown control operation".into()),
        }
        self.pump();
        self.describe(identity)
    }
}
