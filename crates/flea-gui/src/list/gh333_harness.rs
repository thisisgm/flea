//! The #333 harness: a marked scratch root, the reported ui.json, and the production Shell with its probes.
use crate::{
    backend::{Backend, Event as BackendEvent},
    ipc::server::Event,
    list::columns::Column,
    navigation::folder_state::Writer,
    presentation::Fonts,
    state::Shell,
    window::{
        locale::Snapshot,
        startup::{Arguments, Launch, Settings},
    },
};
use flea::{
    ipc::wire::Request,
    jsondoc::{self, Json},
    protocol::{Reply, RowFacts},
};
use gpui::{px, size, Entity, VisualTestContext};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};

fn scratch() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/gh333-width-tests")
}

// A marked mktemp root under the scratch parent, removed only when its test passes.
pub(super) struct Root(pub(super) PathBuf);

impl Drop for Root {
    fn drop(&mut self) {
        let parent = scratch().canonicalize().unwrap();
        let ours = self.0.is_absolute()
            && self.0.file_name().is_some()
            && self.0.join(".flea-test-sandbox").is_file()
            && self.0.parent().and_then(|path| path.canonicalize().ok()) == Some(parent);
        if ours && !std::thread::panicking() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

pub(super) fn root() -> Root {
    std::fs::create_dir_all(scratch()).unwrap();
    let parent = scratch().canonicalize().unwrap();
    let output = Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("test.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let path = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(path.is_absolute() && path.parent() == Some(parent.as_path()));
    std::fs::write(path.join(".flea-test-sandbox"), "gh333 width proof\n").unwrap();
    Root(path)
}

// The issue's columns and widths, plus keys no column code reads, which must survive every step.
pub(super) fn document(text_size: &str) -> String {
    format!(
        r#"{{"view":"list","columns":["name","size","date","kind"],"columnWidths":{{"size":480,"kind":295,"date":480,"mode":480}},"places":{{"rail":"shown","sidebarWidth":192,"showRecent":true}},"keyHints":true,"startFolder":"/keep/me","wrapAtEnds":true,"display":{{"textSize":{text_size},"hyprlandIcons":true}}}}"#
    )
}

const UNRELATED: [&str; 5] = ["keyHints", "startFolder", "wrapAtEnds", "places", "display"];

pub(super) fn rows() -> Vec<RowFacts> {
    (0..60)
        .map(|ix| RowFacts {
            name: format!("report-{ix:03}-quarterly-summary.pdf"),
            size: 987_654_321 + ix,
            mtime: 1_790_000_000 + ix as i64 * 3600,
            mode: 0o100644,
            icon: "application-pdf".into(),
            real_modes: true,
            ..RowFacts::default()
        })
        .collect()
}

// The production constructor with only the backend process replaced, launched from the store's document.
pub(super) fn open(
    cx: &mut gpui::TestAppContext,
    data: Json,
    width: f32,
) -> (Entity<Shell>, &mut VisualTestContext, Arc<AtomicUsize>) {
    let launch = Launch::resolve(
        Settings::from_json(data),
        Arguments {
            path: Some("/reports".into()),
            select: None,
        },
        "/home/test",
    );
    let locale = Snapshot {
        revision: 1,
        catalog: Arc::new(crate::test_catalogs::english()),
    };
    let (_sender, events) = async_channel::unbounded();
    cx.update(|cx| {
        cx.set_global(Fonts::from_sources(
            "[font]\nbase-size=14",
            "",
            "JetBrains Mono".into(),
        ));
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, cx| {
                let (backend, requests) = Backend::test_channel();
                cx.on_release(move |_, _| drop(requests)).detach();
                Ok(backend)
            },
            monitor: None,
        })
    });
    let (shell, vcx) =
        cx.add_window_view(|window, cx| Shell::new(launch, locale, events, window, cx));
    let done = Arc::new(AtomicUsize::new(0));
    shell.update(vcx, |shell, _| {
        let writer = &shell.panes[0].as_ref().unwrap().nav.writer;
        assert_eq!(writer.borrow().sent, 0, "the launch wrote preferences");
        // Live but never run, so a remembered edge stays readable as its patch and nothing leaves the root.
        *writer.borrow_mut() = Writer::held(Arc::clone(&done));
    });
    vcx.simulate_resize(size(px(width), px(900.)));
    vcx.run_until_parked();
    let rows = rows();
    let total = rows.len();
    vcx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            for reply in [
                Reply::Listed {
                    path: "/reports".into(),
                    total,
                },
                Reply::Rows {
                    start: 0,
                    rows,
                    listing: 2,
                },
            ] {
                shell.receive(0, BackendEvent::Reply(reply), window, cx);
            }
        })
    });
    for _ in 0..4 {
        vcx.executor().advance_clock(Duration::from_millis(400));
        draw(vcx);
    }
    (shell, vcx, done)
}

pub(super) fn draw(vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    vcx.run_until_parked();
}

pub(super) fn ask(
    shell: &Entity<Shell>,
    vcx: &mut VisualTestContext,
    function: &str,
    args: &[&str],
) -> String {
    let reply = vcx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: function.into(),
                        arguments: args.iter().map(|v| (*v).into()).collect(),
                    },
                    answer,
                },
                window,
                cx,
            );
            receive.try_recv().unwrap().reply
        })
    });
    assert_eq!(reply.code, 0, "{function}: {}", reply.stderr);
    reply.stdout.trim_end_matches('\n').into()
}

// What one presented frame drew: header and row columns, their cells, and the face's minimums.
pub(super) struct Drawn {
    pub(super) header_names: String,
    pub(super) row_names: String,
    pub(super) cells: [Option<(f32, f32)>; 4],
    pub(super) name: (f32, f32),
    pub(super) name_min: f32,
    pub(super) least: [f32; 4],
    pub(super) base: f32,
    pub(super) gap: f32,
    pub(super) row: f32,
    pub(super) base_size: f32,
    pub(super) stored: [Option<u32>; 4],
    pub(super) sent: usize,
}

pub(super) fn drawn(shell: &Entity<Shell>, vcx: &mut VisualTestContext) -> Drawn {
    shell.read_with(vcx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let list = host.list.borrow();
        assert!(list.header.drawn.frames > 0, "HeaderElement never painted");
        assert!(list.drawn.frames > 0, "ListElement never painted");
        let tokens = list.observed_column_tokens().expect("a painted face");
        let layout = list.drawn.layout;
        let stored = list.view.stored;
        Drawn {
            header_names: list.header.drawn.layout.names(),
            row_names: layout.names(),
            cells: Column::ALL.map(|c| layout.cell(c).map(|cell| (cell.x, cell.width))),
            name: (layout.name.x, layout.name.width),
            name_min: tokens.name_min,
            least: Column::ALL.map(|c| tokens.wanted(c, stored).min(tokens.measured[c.slot()])),
            base: 2. * tokens.pad + tokens.icon + tokens.gap + tokens.name_min,
            gap: tokens.gap,
            row: f32::from(list.drawn.bounds.size.width) - list.drawn.metrics.lane,
            base_size: f32::from(list.typeface.base),
            stored: Column::ALL.map(|c| stored.get(c)),
            sent: host
                .nav
                .writer
                .borrow()
                .patches
                .iter()
                .filter(|patch| !jsondoc::parse(patch).is_ok_and(|json| session_only(&json)))
                .count(),
        }
    })
}

pub(super) fn bytes(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

pub(super) fn unrelated(data: &Json) -> Vec<String> {
    UNRELATED
        .map(|key| jsondoc::render(data.get(key).unwrap_or(&Json::Null)))
        .to_vec()
}

// The tab session record carries only lastPath and lastTabs; any other key in a patch is a preference write.
pub(super) fn session_only(patch: &Json) -> bool {
    matches!(patch, Json::Obj(members) if !members.is_empty()
        && members.iter().all(|(key, _)| key == "lastPath" || key == "lastTabs"))
}
