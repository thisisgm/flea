//! Mounted menu fixtures keep every filesystem write inside a marked worktree scratch root.
use crate::{
    backend::{Backend, Event},
    state::{tests::shell_with, Shell},
    window::{pane::PaneHost, root::Root},
};
use flea::{
    jsondoc::{self, Json},
    protocol::{Reply, RowFacts},
};
use gpui::{AppContext, Context, TestAppContext, Window, WindowHandle};
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

#[path = "makeexec_backend.rs"]
mod backend;
pub(super) use backend::Process;

pub(super) struct Fixture {
    pub handle: WindowHandle<Root>,
    pub root: PathBuf,
    requests: Box<dyn FnMut() -> Option<String>>,
}

pub(super) fn update<R>(
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

impl Fixture {
    pub fn new(cx: &mut TestAppContext, mode: u32) -> Self {
        Self::listed(cx, mode, true)
    }

    /// The same sandbox with the listing drawn empty, so a key menu opens on the folder itself.
    pub fn empty(cx: &mut TestAppContext) -> Self {
        Self::listed(cx, 0o644, false)
    }

    fn listed(cx: &mut TestAppContext, mode: u32, rows: bool) -> Self {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/m9-makeexec");
        assert!(parent.is_absolute());
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let created = Command::new("mktemp")
            .arg("-d")
            .arg(parent.join("test.XXXXXX"))
            .output()
            .unwrap();
        assert!(created.status.success());
        let root = PathBuf::from(String::from_utf8(created.stdout).unwrap().trim_end());
        assert!(root.is_absolute() && root.starts_with(&parent));
        std::fs::write(root.join(".flea-test-sandbox"), "makeexec fixture\n").unwrap();
        for (name, text) in [
            ("build.sh", "#!/bin/sh\necho built\n"),
            ("notes.txt", "plain notes\n"),
        ] {
            assert!(root.join(".flea-test-sandbox").is_file());
            std::fs::write(root.join(name), text).unwrap();
            std::fs::set_permissions(
                root.join(name),
                std::fs::Permissions::from_mode(if name == "build.sh" { mode } else { 0o644 }),
            )
            .unwrap();
        }
        let (backend, receive) = Backend::test_lines();
        let directory = root.to_string_lossy().into_owned();
        let handle = cx.add_window(|window, cx| {
            let shell = cx.new(|cx| {
                let mut shell = shell_with(backend, cx);
                let mut host = PaneHost::new(directory.clone(), None, 620., &shell.locale);
                host.backend = shell.panes[0].as_mut().unwrap().backend.take();
                host.pane.use_executor(cx.background_executor().clone());
                host.pane
                    .receive(Reply::Listed {
                        path: directory,
                        total: if rows { 2 } else { 0 },
                    })
                    .unwrap();
                host.pane
                    .receive(Reply::Rows {
                        start: 0,
                        rows: [
                            RowFacts {
                                name: "build.sh".into(),
                                mode: 0o100000 | mode,
                                ..RowFacts::default()
                            },
                            RowFacts {
                                name: "notes.txt".into(),
                                mode: 0o100644,
                                ..RowFacts::default()
                            },
                        ]
                        .into_iter()
                        .filter(|_| rows)
                        .collect(),
                        listing: 2,
                    })
                    .unwrap();
                shell.panes[0] = Some(host);
                shell
            });
            shell.update(cx, |shell, cx| window.focus(&shell.focus, cx));
            Root { shell }
        });
        Self {
            handle,
            root,
            requests: Box::new(receive),
        }
    }

    // The folder's blank-space menu, which has no snapshot to take.
    pub fn open_background(&mut self, cx: &mut TestAppContext) {
        update(cx, self.handle, |shell, window, cx| {
            shell.open_menu(0, None, (40., 40.), window, cx);
        });
        self.drain();
    }

    pub fn open(
        &mut self,
        cx: &mut TestAppContext,
        row: usize,
        selection: Vec<usize>,
    ) -> Option<Json> {
        update(cx, self.handle, |shell, window, cx| {
            shell.panes[0].as_mut().unwrap().selected = selection;
            shell.open_menu(0, Some(row), (40., 40.), window, cx);
        });
        let requests = self.drain();
        let snapshot = requests
            .iter()
            .find(|r| r.get("op").and_then(Json::as_str) == Some("snapshot"))
            .unwrap();
        let id = number(snapshot);
        self.receive(cx, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"snapshot\",\"ok\":true,\"kinds\":[32768]}}"));
        let asks = self.drain();
        if asks
            .iter()
            .any(|r| r.get("op").and_then(Json::as_str) == Some("applications"))
        {
            self.receive(cx, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"applications\",\"ok\":true,\"applications\":[]}}"));
        }
        assert_eq!(
            super::super::super::join_tests::read(cx, self.handle, "contextMenuVisible").stdout,
            "true\n"
        );
        requests
            .into_iter()
            .find(|r| r.get("c").and_then(Json::as_str) == Some("shebang"))
    }

    pub fn drain(&mut self) -> Vec<Json> {
        let mut out = Vec::new();
        while let Some(line) = (self.requests)() {
            out.push(jsondoc::parse(&line).unwrap());
        }
        out
    }

    pub fn receive(&self, cx: &mut TestAppContext, line: &str) {
        update(cx, self.handle, |shell, window, cx| {
            shell.receive(0, Event::Producer(line.into()), window, cx)
        });
        cx.run_until_parked();
    }

    pub fn probe(&self, cx: &mut TestAppContext, request: &Json) {
        let line = Process::new(&self.root).request(request, "shebang");
        self.receive(cx, &line);
        update(cx, self.handle, |shell, _, _| {
            assert!(
                shell.menu.shebang.last_reply.is_some(),
                "receipt not delivered: {line}; reader {}, ask {:?}, captured {:?}",
                shell.menu.shebang.reader.is_some(),
                shell.menu.shebang.ask,
                shell.menu.captured
            );
            if shell.menu.shebang.id == Some(number(request)) && shell.menu.captured.is_some() {
                assert!(
                    shell.menu.shebang.reply.is_some(),
                    "rejected {line}; ask {:?}, captured {:?}, actual identity {}",
                    shell.menu.shebang.ask,
                    shell.menu.captured,
                    shell.menu_identity()
                );
            }
        });
    }

    pub fn model(&self, cx: &mut TestAppContext) -> Json {
        jsondoc::parse(
            super::super::super::join_tests::read(cx, self.handle, "contextMenuModel")
                .stdout
                .trim(),
        )
        .unwrap()
    }

    pub fn has_action(&self, cx: &mut TestAppContext) -> bool {
        self.model(cx)
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row.get("action").and_then(Json::as_str) == Some("makeExecutable"))
    }

    pub fn choose(&mut self, cx: &mut TestAppContext) -> Json {
        update(cx, self.handle, |shell, window, cx| {
            shell.menu_outcome(
                crate::g2::context_menus::menu::Outcome::Fire {
                    action: "makeExecutable".into(),
                    leaf: String::new(),
                },
                window,
                cx,
            )
        });
        let requests = self.drain();
        let validate = requests
            .iter()
            .find(|r| r.get("op").and_then(Json::as_str) == Some("validate"))
            .expect("the real action validates its captured target");
        self.receive(cx, &format!("{{\"t\":\"menuaction\",\"id\":{},\"op\":\"validate\",\"ok\":true,\"action\":\"makeExecutable\",\"paths\":[\"{}\"]}}", number(validate), flea::json::escape(&self.root.join("build.sh").to_string_lossy())));
        self.drain()
            .into_iter()
            .find(|r| r.get("c").and_then(Json::as_str) == Some("permissionsBatch"))
            .expect("the action reuses the permissions batch")
    }
}

pub(super) fn number(value: &Json) -> u64 {
    value.get("id").and_then(Json::as_f64).unwrap() as u64
}
