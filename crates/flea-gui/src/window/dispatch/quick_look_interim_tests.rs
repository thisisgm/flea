//! The column's mounted image is only first paint; Quick Look still lands the original.
use super::{preview_quality_tests::Root, previewtypes_tests as host, test_shell::land};
use crate::{
    backend::Backend,
    navigation::folder_state::Writer,
    state::Shell,
    window::{
        locale::Snapshot,
        selection_preview::{Frame, Kind, Mounted},
        startup::{Arguments, Launch, Settings},
        u07_tests::read,
    },
};
use gpui::{px, size, AppContext, Entity, ImageId, RenderImage, TestAppContext, VisualTestContext};
use std::sync::Arc;

fn root() -> Root {
    let root = Root::new();
    image::RgbaImage::from_pixel(80, 60, image::Rgba([20, 40, 220, 255]))
        .save(root.0.join("picture.png"))
        .unwrap();
    root
}

fn meta_line(token: u64, w: u32, h: u32) -> String {
    format!(
        r#"{{"t":"meta","row":0,"token":{token},"w":{w},"h":{h},"orient":1,"ms":0,"rate":0,"entries":0,"unpacked":0,"afailed":false,"names":[],"lines":0,"partial":false,"lfailed":false,"frames":0,"photo":null,"target":"","targetdir":false,"raw":null,"owner":"gm"}}"#
    )
}

fn interim_shown(shell: &Entity<Shell>, interim: ImageId, cx: &gpui::App) -> bool {
    let look = shell.read(cx).quick_look.as_ref().unwrap().view.read(cx);
    look.svg_raster().is_some_and(|image| image.id == interim)
}

fn retired(shell: &Entity<Shell>, image: ImageId, cx: &gpui::App) -> bool {
    let look = shell.read(cx).quick_look.as_ref().unwrap().view.read(cx);
    look.retired_ids.contains(&image)
}

fn column<'a>(
    cx: &'a mut TestAppContext,
    root: &Root,
) -> (
    Entity<Shell>,
    &'a mut VisualTestContext,
    ImageId,
    super::preview_tests::Lines,
) {
    let path = root.0.to_str().unwrap();
    let settings = flea::jsondoc::parse(
        r#"{"keys":"default","view":"columns","preview":{"column":true,"thumbnails":"off"}}"#,
    )
    .unwrap();
    let launch = Launch::resolve(
        Settings::from_json(settings),
        Arguments {
            path: Some(path.into()),
            select: None,
        },
        &std::env::var("HOME").unwrap(),
    );
    let locale = Snapshot {
        revision: 1,
        catalog: Arc::new(crate::test_catalogs::english()),
    };
    let (_sender, events) = async_channel::unbounded();
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, cx| {
                let (backend, requests) = Backend::test_channel();
                cx.on_release(move |_, _| drop(requests)).detach();
                Ok(backend)
            },
            monitor: None,
        });
    });
    let (root_view, cx) = cx.add_window_view(|window, cx| crate::window::root::Root {
        shell: cx.new(|cx| Shell::new(launch, locale, events, window, cx)),
    });
    let shell = root_view.read_with(cx, |root, _| root.shell.clone());
    let (backend, mut requests) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        let pane = shell.panes[0].as_mut().unwrap();
        pane.backend = Some(backend);
        *pane.nav.writer.borrow_mut() = Writer::default();
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.run_until_parked();
    land(
        &shell,
        cx,
        path,
        vec![host::file(
            "picture.png",
            "image-png",
            "PNG image",
            0o100644,
        )],
        1,
    );
    host::answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"fsinfo","path":"{path}","fs":"tmpfs","dev":1,"total":100,"free":90,"class":"local"}}"#
        ),
    );
    host::pump(cx);
    let lines: Vec<String> = std::iter::from_fn(&mut requests).collect();
    host::meta(&shell, cx, &lines, r#","w":80,"h":60,"orient":1"#);
    host::pump(cx);
    let interim = Arc::new(RenderImage::from_bgra(4, 3, [0, 0, 240, 255].repeat(12)).unwrap());
    shell.update(cx, |shell, cx| {
        let preview = shell.selection_preview(0).unwrap();
        preview.update(cx, |view, _| {
            let mut frame = view.drawn.take().or_else(|| view.pending.take()).unwrap();
            assert!(frame.facts.as_ref().is_some_and(|meta| meta.w == 80));
            frame.source = Some(root.0.join("picture.png").into());
            frame.image = Some(interim.clone());
            frame.failed = false;
            view.pending = None;
            view.drawn = Some(frame);
            assert!(view.visible);
        });
    });
    (shell, cx, interim.id, Box::new(requests))
}

#[gpui::test]
fn quick_look_column_interim_paints_without_cap_and_lands_the_original(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = root();
    let (shell, cx, interim, _requests) = column(cx, &root);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(shell.open_quick_look(0, window, cx));
            let look = shell.quick_look.as_ref().unwrap().view.read(cx);
            let first = look
                .svg_raster()
                .expect("the current column supplies first paint");
            assert!(first.id == interim, "the decoded column frame is reused");
            assert!(
                look.is_revealed(),
                "drawable content reveals without the cap"
            );
            let observed = look.observations.borrow();
            assert!(!observed.landed(), "interim is not final");
            assert_eq!(observed.reveal_caps, 0);
        });
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let token = shell.quick_look.as_ref().unwrap().token;
            shell.receive(
                0,
                Backend::test_reply_line(&meta_line(token, 80, 60)),
                window,
                cx,
            );
        });
        let look = shell.read(cx).quick_look.as_ref().unwrap().view.read(cx);
        assert!(
            look.svg_raster().is_some() && look.is_revealed(),
            "Quick Look's own meta reply keeps the first paint up"
        );
        assert!(!look.observations.borrow().revealed_early());
    });
    let started = std::time::Instant::now();
    loop {
        host::pump(cx);
        if shell.read_with(cx, |shell, cx| {
            shell
                .quick_look
                .as_ref()
                .unwrap()
                .view
                .read(cx)
                .svg_raster()
                .is_some_and(|image| image.id != interim)
        }) {
            break;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "the original replaces the interim"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    shell.read_with(cx, |shell, cx| {
        let look = shell.quick_look.as_ref().unwrap().view.read(cx);
        let final_image = look.svg_raster().expect("the original decoded");
        assert!(final_image.id != interim, "interim must be replaced");
        assert_eq!(
            (final_image.size(0).width.0, final_image.size(0).height.0),
            (80, 60)
        );
        assert_eq!(&final_image.as_bytes(0).unwrap()[..4], &[220, 40, 20, 255]);
    });
    host::pump(cx);
    cx.update(|_, cx| {
        assert!(
            !retired(&shell, interim, cx),
            "the column still paints its interim, so Quick Look keeps the tile"
        );
    });
    let landed = read(&shell, cx, "previewSwapState", &[]);
    assert!(landed.contains(r#""landed":true"#), "{landed}");
    assert!(landed.contains(r#""revealedEarly":false"#), "{landed}");
    assert!(landed.contains(r#""revealCaps":0"#), "{landed}");
    cx.simulate_keystrokes("escape");
    host::pump(cx);
}

fn reject_interim(cx: &mut TestAppContext, pending: bool) {
    let root = root();
    let (shell, cx, interim, _requests) = column(cx, &root);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.selection_preview(0).unwrap().update(cx, |view, _| {
                if pending {
                    let drawn = view.drawn.as_ref().unwrap();
                    view.pending = Some(Frame {
                        subject: drawn.subject.clone(),
                        generation: drawn.generation + 1,
                        kind: Kind::Image,
                        mounted: Mounted::Basic,
                        meta: None,
                        facts: None,
                        thumb_answered: false,
                        thumb_observation: None,
                        source: None,
                        image: None,
                        animation: None,
                        animation_source: None,
                        quality: Default::default(),
                        trace: None,
                        failed: false,
                        manual: false,
                        remote: false,
                        _watch: None,
                    });
                    assert!(view.drawn.is_some(), "only the pending guard can refuse");
                } else {
                    view.drawn.as_mut().unwrap().subject.row.mtime += 1;
                }
            });
            assert!(shell.open_quick_look(0, window, cx));
            assert!(
                !shell
                    .quick_look
                    .as_ref()
                    .unwrap()
                    .view
                    .read(cx)
                    .svg_raster()
                    .is_some_and(|image| image.id == interim),
                "a stale or pending column cannot supply first paint"
            );
        });
    });
    cx.simulate_keystrokes("escape");
    host::pump(cx);
}

#[gpui::test]
fn quick_look_column_interim_rejects_changed_row(cx: &mut TestAppContext) {
    reject_interim(cx, false);
}

#[gpui::test]
fn quick_look_column_interim_rejects_pending_frame(cx: &mut TestAppContext) {
    reject_interim(cx, true);
}

#[gpui::test]
fn quick_look_column_interim_metadata_refusal_lands_the_error_card(cx: &mut TestAppContext) {
    let root = root();
    let (shell, cx, interim, _requests) = column(cx, &root);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(shell.open_quick_look(0, window, cx));
        });
        assert!(
            interim_shown(&shell, interim, cx),
            "the refusal arrives over the interim"
        );
        shell.update(cx, |shell, cx| {
            let token = shell.quick_look.as_ref().unwrap().token;
            shell.receive(
                0,
                Backend::test_reply_line(&meta_line(token, 0, 0)),
                window,
                cx,
            );
        });
    });
    host::pump(cx);
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "This file could not be read."
    );
    let state = read(&shell, cx, "previewSwapState", &[]);
    assert!(state.contains(r#""landed":true"#), "{state}");
    assert!(state.contains(r#""revealedEarly":false"#), "{state}");
    assert!(state.contains(r#""revealCaps":0"#), "{state}");
    cx.simulate_keystrokes("escape");
    host::pump(cx);
}

#[gpui::test]
fn quick_look_column_interim_raster_refusal_drops_the_interim(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = root();
    let (shell, cx, interim, _requests) = column(cx, &root);
    std::fs::write(root.0.join("picture.png"), b"not a png").unwrap();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(shell.open_quick_look(0, window, cx));
        });
        assert!(interim_shown(&shell, interim, cx));
    });
    let started = std::time::Instant::now();
    while cx.update(|_, cx| interim_shown(&shell, interim, cx)) {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "a refused original removes the interim"
        );
        host::pump(cx);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    host::pump(cx);
    cx.update(|_, cx| {
        let look = shell.read(cx).quick_look.as_ref().unwrap().view.read(cx);
        assert!(
            look.svg_raster().is_none() && look.svg_refused(),
            "the refusal card replaces the interim"
        );
        assert!(
            !retired(&shell, interim, cx),
            "the column still paints its interim, so Quick Look keeps the tile"
        );
    });
    let state = read(&shell, cx, "previewSwapState", &[]);
    assert!(state.contains(r#""landed":true"#), "{state}");
    cx.simulate_keystrokes("escape");
    host::pump(cx);
}

#[gpui::test]
fn quick_look_column_interim_frees_the_tile_the_column_let_go(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = root();
    let (shell, cx, interim, _requests) = column(cx, &root);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(shell.open_quick_look(0, window, cx));
            shell.selection_preview(0).unwrap().update(cx, |view, _| {
                view.drawn.as_mut().unwrap().image = None;
            });
        });
        assert!(interim_shown(&shell, interim, cx));
    });
    let started = std::time::Instant::now();
    while !cx.update(|_, cx| retired(&shell, interim, cx)) {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "Quick Look frees an interim it alone holds once the original lands"
        );
        host::pump(cx);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    cx.update(|_, cx| {
        let look = shell.read(cx).quick_look.as_ref().unwrap().view.read(cx);
        assert!(look.svg_raster().is_some_and(|image| image.id != interim));
    });
    cx.simulate_keystrokes("escape");
    host::pump(cx);
}
