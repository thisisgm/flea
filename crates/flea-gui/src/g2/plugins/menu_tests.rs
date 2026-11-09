//! Mounted Shell menus use real example executables at the transport boundary.
use super::*;
use crate::{
    backend::{Backend, Event},
    g2::context_menus::menu::Outcome,
    state::{tests::shell_with, Shell},
    window::{pane::PaneHost, root::Root},
};
use flea::{
    jsondoc,
    plugin_runtime::{Config, Runtime},
    protocol::{Reply, RowFacts},
    ui_logic_plugin_settings::{
        manifest,
        wire::{Plugin, Source},
    },
};
use gpui::{AppContext, Context, TestAppContext, Window, WindowHandle};
use std::{
    cell::RefCell,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    rc::Rc,
    time::Duration,
};

struct Fixture {
    handle: WindowHandle<Root>,
    lines: Box<dyn FnMut() -> Option<String>>,
    plugins: Vec<Plugin>,
    real_host: bool,
    drained: Option<async_channel::Receiver<()>>,
    _scratch: Scratch,
}

/// Removes a scratch root when its test ends, pass or fail, and only a root `scratch` made.
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let made = self.0.is_absolute()
            && self.0.parent() == Some(Path::new("/tmp"))
            && self
                .0
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("flea-plugin-menu."))
            && std::fs::read(self.0.join(".flea-test-sandbox")).is_ok_and(|m| m == SANDBOX_MARK);
        if made {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

const SANDBOX_MARK: &[u8] = b"plugin menu fixture\n";

const CHECKSUM: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn update<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| f(shell, window, cx))
        })
        .unwrap()
}

fn scratch() -> PathBuf {
    let made = Command::new("mktemp")
        .args(["-d", "/tmp/flea-plugin-menu.XXXXXX"])
        .output()
        .unwrap();
    assert!(made.status.success());
    let root = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
    assert!(root.is_absolute() && root.starts_with("/tmp") && root.as_os_str().len() > 1);
    std::fs::write(root.join(".flea-test-sandbox"), SANDBOX_MARK).unwrap();
    for home in ["data", "config", "cache", "state", "home"] {
        std::fs::create_dir(root.join(home)).unwrap();
    }
    std::fs::write(root.join("notes.txt"), b"abc").unwrap();
    std::fs::write(root.join("photo.png"), b"not the selected file").unwrap();
    std::fs::write(root.join("other.txt"), b"other").unwrap();
    std::fs::write(root.join("third.txt"), b"third").unwrap();
    std::fs::create_dir(root.join("folder")).unwrap();
    root
}

fn plugin(example: &str, root: &Path) -> (Plugin, Runtime) {
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/plugins")
        .join(example);
    let folder = root.join(example);
    std::fs::create_dir(&folder).unwrap();
    let manifest = manifest::parse(
        &std::fs::read_to_string(source.join("manifest.json")).unwrap(),
        false,
    )
    .unwrap();
    let script = std::fs::read_to_string(source.join("plugin.py")).unwrap();
    let script = script.replace("        request = json.loads(line)", &format!(
        "        request = json.loads(line)\n        with open('requests.jsonl', 'a') as receipt:\n            receipt.write(json.dumps(request) + '\\n')\n        os.environ['XDG_DATA_HOME'] = {0:?} + '/data'\n        os.environ['XDG_CONFIG_HOME'] = {0:?} + '/config'\n        os.environ['XDG_CACHE_HOME'] = {0:?} + '/cache'\n        os.environ['XDG_STATE_HOME'] = {0:?} + '/state'", root.to_string_lossy()));
    std::fs::write(folder.join("plugin.py"), script).unwrap();
    std::fs::set_permissions(
        folder.join("plugin.py"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let runtime = Runtime::new(Config {
        folder,
        manifest: manifest.clone(),
        hello: jsondoc::parse(r#"{"op":"hello","protocolVersion":1,"settings":{}}"#).unwrap(),
        ready_timeout: Duration::from_secs(2),
        request_timeout: Duration::from_secs(2),
        idle_timeout: Duration::from_secs(60),
    });
    runtime.ensure_ready().unwrap();
    (
        Plugin {
            manifest,
            source: Source::Folder(source.to_string_lossy().into()),
            enabled: true,
            failure: None,
            mark_path: String::new(),
            settings: Vec::new(),
            shelf: None,
        },
        runtime,
    )
}

fn open(cx: &mut TestAppContext, root: PathBuf, plugin: Plugin) -> Fixture {
    let mut plugins = flea::ui_logic_plugin_settings::builtin::manifests()
        .into_iter()
        .map(|(manifest, enabled)| Plugin {
            manifest,
            enabled,
            source: Source::Builtin,
            failure: None,
            mark_path: String::new(),
            settings: Vec::new(),
            shelf: None,
        })
        .collect::<Vec<_>>();
    plugins.push(plugin.clone());
    let (backend, lines) = Backend::test_lines();
    let path = root.to_string_lossy().into_owned();
    let handle = cx.add_window(move |window, cx| {
        let shell = cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            let mut host = PaneHost::new(path.clone(), None, 620., &shell.locale);
            host.backend = shell.panes[0].as_mut().unwrap().backend.take();
            host.pane.use_executor(cx.background_executor().clone());
            host.pane
                .receive(Reply::Listed {
                    path: path.clone(),
                    total: 5,
                })
                .unwrap();
            host.pane
                .receive(Reply::Rows {
                    start: 0,
                    listing: 2,
                    rows: vec![
                        RowFacts {
                            name: "notes.txt".into(),
                            mode: 0o100644,
                            ..Default::default()
                        },
                        RowFacts {
                            name: "photo.png".into(),
                            mode: 0o100644,
                            icon: "image-x-generic".into(),
                            ..Default::default()
                        },
                        RowFacts {
                            name: "other.txt".into(),
                            mode: 0o100644,
                            ..Default::default()
                        },
                        RowFacts {
                            name: "third.txt".into(),
                            mode: 0o100644,
                            ..Default::default()
                        },
                        RowFacts {
                            name: "folder".into(),
                            mode: 0o40755,
                            dir: true,
                            ..Default::default()
                        },
                    ],
                })
                .unwrap();
            shell.panes[0] = Some(host);
            shell.output.archive[0] = Some(vec!["zip".into()]);
            shell.menu.plugin_runtime.plugins.push(plugin);
            shell.menu.plugin_runtime.mimes = Some(Arc::new(mime::Db::from_str(
                "50:text/plain:*.txt\n50:image/png:*.png\n",
            )));
            shell
        });
        shell.update(cx, |shell, cx| window.focus(&shell.focus, cx));
        Root { shell }
    });
    Fixture {
        handle,
        lines: Box::new(lines),
        plugins,
        real_host: false,
        drained: None,
        _scratch: Scratch(root),
    }
}

impl Fixture {
    fn drain(&mut self) -> Vec<Json> {
        std::iter::from_fn(&mut self.lines)
            .map(|line| jsondoc::parse(&line).unwrap())
            .collect()
    }
    fn feed(&self, cx: &mut TestAppContext, reply: Json) {
        update(cx, self.handle, |shell, window, cx| {
            shell.receive(
                0,
                Event::Producer(flea::ui_logic_plugin_settings::wire::compact(&reply)),
                window,
                cx,
            )
        });
    }
    fn menu(&mut self, cx: &mut TestAppContext, cursor: usize, selected: Vec<usize>) -> u64 {
        let plugins = self.plugins.clone();
        let real_host = self.real_host;
        update(cx, self.handle, |shell, window, cx| {
            shell.panes[0].as_mut().unwrap().selected = selected;
            let targets = shell.menu.plugin_runtime.targets.clone();
            shell.open_menu(0, Some(cursor), (40., 40.), window, cx);
            if !real_host {
                shell.menu.plugin_runtime.plugins = plugins;
                shell.menu.plugin_runtime.targets = targets;
            }
        });
        let requests = self.drain();
        let id = requests
            .iter()
            .find(|r| r.get("op").and_then(Json::as_str) == Some("snapshot"))
            .unwrap()
            .get("id")
            .unwrap()
            .clone();
        self.feed(
            cx,
            Json::Obj(vec![
                ("t".into(), Json::Str("menuaction".into())),
                ("op".into(), Json::Str("snapshot".into())),
                ("id".into(), id.clone()),
                ("ok".into(), Json::Bool(true)),
                ("kinds".into(), Json::Arr(vec![])),
            ]),
        );
        for request in self.drain() {
            if request.get("op").and_then(Json::as_str) == Some("applications") {
                self.feed(
                    cx,
                    Json::Obj(vec![
                        ("t".into(), Json::Str("menuaction".into())),
                        ("op".into(), Json::Str("applications".into())),
                        ("id".into(), id.clone()),
                        ("ok".into(), Json::Bool(true)),
                        ("applications".into(), Json::Arr(vec![])),
                    ]),
                );
            }
        }
        id.as_f64().unwrap() as u64
    }
    fn entries(&self, cx: &mut TestAppContext) -> Vec<Entry> {
        update(cx, self.handle, |shell, _, _| {
            shell
                .menu
                .menu
                .borrow()
                .card
                .as_ref()
                .unwrap()
                .entries
                .clone()
        })
    }
    fn choose(&mut self, cx: &mut TestAppContext, action: &str, leaf: &str) -> Option<Json> {
        update(cx, self.handle, |shell, window, cx| {
            shell.menu_outcome(
                Outcome::Fire {
                    action: action.into(),
                    leaf: leaf.into(),
                },
                window,
                cx,
            )
        });
        self.drain()
            .into_iter()
            .find(|r| r.get("op").and_then(Json::as_str) == Some("validate"))
    }
    fn validated(&self, cx: &mut TestAppContext, request: &Json, paths: &[PathBuf]) {
        self.feed(
            cx,
            Json::Obj(vec![
                ("t".into(), Json::Str("menuaction".into())),
                ("op".into(), Json::Str("validate".into())),
                ("id".into(), request.get("id").unwrap().clone()),
                ("ok".into(), Json::Bool(true)),
                ("action".into(), request.get("action").unwrap().clone()),
                (
                    "paths".into(),
                    Json::Arr(
                        paths
                            .iter()
                            .map(|p| Json::Str(p.to_string_lossy().into()))
                            .collect(),
                    ),
                ),
            ]),
        );
    }

    fn submit_input(&mut self, cx: &mut TestAppContext, value: &str) -> Option<Json> {
        update(cx, self.handle, |shell, window, cx| {
            let open = shell
                .plugins
                .input
                .as_mut()
                .expect("Flea's native input card is open");
            assert_eq!(open.descriptor.title, "Verify Checksum");
            assert_eq!(open.descriptor.label, "Checksum");
            assert_eq!(open.descriptor.submit, "Verify");
            open.value = value.into();
            shell.finish_plugin_input(true, window, cx);
        });
        cx.run_until_parked();
        self.drain()
            .into_iter()
            .find(|r| r.get("op").and_then(Json::as_str) == Some("validate"))
    }

    fn cancel_input(&mut self, cx: &mut TestAppContext) -> Vec<Json> {
        update(cx, self.handle, |shell, window, cx| {
            shell.finish_plugin_input(false, window, cx)
        });
        cx.run_until_parked();
        self.drain()
    }
}

#[path = "menu_cases.rs"]
mod cases;

#[path = "menu_builtin_tests.rs"]
mod builtin_tests;

#[path = "menu_host_tests.rs"]
mod host_tests;

#[path = "menu_input_tests.rs"]
mod input_tests;

#[path = "menu_selection_tests.rs"]
mod selection_tests;

#[path = "menu_external_tests.rs"]
mod external_tests;

#[path = "menu_shelf_join_tests.rs"]
mod shelf_join_tests;
