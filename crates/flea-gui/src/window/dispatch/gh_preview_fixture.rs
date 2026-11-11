use super::preview_tests::{answer, body, close, draw, file, sent, token};
use super::test_shell::{keystroke, land, set_cursor};
use crate::{
    backend::Backend,
    g2::quick_look_overlay::{Body, QuickLook},
    state::Shell,
    window::{
        locale::Snapshot,
        startup::{Arguments, Launch, Settings},
        u07_tests::read,
    },
};
use flea::protocol::RowFacts;
use gpui::{
    px, size, AppContext, Bounds, Entity, ImgResourceLoader, Pixels, RenderImage, Resource,
    TestAppContext, VisualTestContext,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

// The test owns its timeout, so a completed test cannot terminate a later one.
struct Watchdog(std::sync::mpsc::Sender<()>);
impl Drop for Watchdog {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}
fn watchdog(secs: u64) -> Watchdog {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if receiver.recv_timeout(Duration::from_secs(secs))
            == Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        {
            eprintln!("gh-preview-proof watchdog: {secs} s passed, exiting 97");
            std::process::exit(97);
        }
    });
    Watchdog(sender)
}

/// A marked mktemp root under the checkout's own .superpowers, removed only when its test passes.
struct Root(PathBuf);

impl Drop for Root {
    fn drop(&mut self) {
        let ours = self.0.is_absolute()
            && self.0.join(".gh-preview-proof").is_file()
            && self
                .0
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("run."));
        if ours && !std::thread::panicking() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

fn root() -> Root {
    let parent =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/gh-preview-proof-run");
    std::fs::create_dir_all(&parent).unwrap();
    let parent = parent.canonicalize().unwrap();
    let out = std::process::Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("run.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(out.status.success());
    let path = PathBuf::from(String::from_utf8(out.stdout).unwrap().trim());
    assert!(path.is_absolute() && path.parent() == Some(parent.as_path()));
    std::fs::write(path.join(".gh-preview-proof"), "gh-preview-proof\n").unwrap();
    Root(path)
}

/// The test_shell constructor opened on a real folder, the pane's backend recording each request line.
pub(super) fn opened_at<'a>(
    cx: &'a mut TestAppContext,
    dir: &Path,
) -> (
    Entity<Shell>,
    &'a mut VisualTestContext,
    super::preview_tests::Lines,
) {
    let document = flea::jsondoc::parse(
        r#"{"keys":"default","view":"list","places":{"rail":"shown","showHome":true,"showTrash":true}}"#,
    )
    .unwrap();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    let launch = Launch::resolve(
        Settings::from_json(document),
        Arguments {
            path: Some(dir.to_string_lossy().into_owned()),
            select: None,
        },
        &home,
    );
    let locale = Snapshot {
        revision: 1,
        catalog: Arc::new(crate::test_catalogs::english()),
    };
    let (_sender, events) = async_channel::unbounded();
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::NoSettingsWorker);
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, cx| {
                let (backend, requests) = Backend::test_channel();
                cx.on_release(move |_, _| drop(requests)).detach();
                Ok(backend)
            },
            monitor: None,
        })
    });
    let (root, cx) = cx.add_window_view(|window, cx| crate::window::root::Root {
        shell: cx.new(|cx| Shell::new(launch, locale, events, window, cx)),
    });
    let shell = root.read_with(cx, |root, _| root.shell.clone());
    shell.update(cx, |shell, _| {
        let writer = &shell.panes[0].as_ref().unwrap().nav.writer;
        *writer.borrow_mut() = crate::navigation::folder_state::Writer::default();
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.run_until_parked();
    let (backend, requests) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    (shell, cx, Box::new(requests))
}

fn meta_reply(row: usize, w: u32, h: u32, token: u64) -> String {
    format!(
        r#"{{"t":"meta","row":{row},"w":{w},"h":{h},"orient":1,"ms":0,"rate":0,"entries":0,"unpacked":0,"afailed":false,"names":[],"lines":0,"partial":false,"lfailed":false,"frames":0,"photo":null,"target":"","targetdir":false,"raw":null,"owner":"gm","token":{token}}}"#
    )
}

fn look(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Entity<QuickLook> {
    shell.read_with(cx, |shell, _| {
        shell
            .quick_look
            .as_ref()
            .expect("Quick Look is mounted")
            .view
            .clone()
    })
}

fn png(path: &Path, w: u32, h: u32) {
    image::RgbaImage::from_pixel(w, h, image::Rgba([40, 120, 200, 255]))
        .save(path)
        .unwrap();
}

/// GPUI's decoded picture for this file once its loader has answered.
fn decoded(
    cx: &mut VisualTestContext,
    shell: &Entity<Shell>,
    path: &Path,
) -> Option<Arc<RenderImage>> {
    let look = look(shell, cx);
    let image = look.read_with(cx, |look, _| look.svg_raster());
    if image.is_some() {
        assert!(
            !cx.update(|_, cx| cx.has_asset::<ImgResourceLoader>(&Resource::Path(Arc::from(path)))),
            "the full-source GPUI loader must never run for Quick Look pictures"
        );
    }
    image
}

fn wait<T>(
    cx: &mut VisualTestContext,
    what: &str,
    mut probe: impl FnMut(&mut VisualTestContext) -> Option<T>,
) -> T {
    let start = Instant::now();
    loop {
        cx.executor().advance_clock(Duration::from_millis(50));
        draw(cx);
        if let Some(value) = probe(cx) {
            return value;
        }
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "{what} within 60 s"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
