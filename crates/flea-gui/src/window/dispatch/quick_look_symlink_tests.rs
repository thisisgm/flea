//! The production Quick Look reply path keeps a link's bar above a large target image.
use super::super::gh_preview_proof_tests::opened_at;
use super::super::preview_tests::{answer, body, close, draw, file, press, sent, token};
use super::super::test_shell::{land, set_cursor};
use crate::g2::pv1::Body;
use gpui::{px, TestAppContext};

fn image(cx: &mut TestAppContext, linked: bool) {
    cx.executor().allow_parking();
    let parent = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.superpowers/quick-look-symlink-join");
    std::fs::create_dir_all(&parent).unwrap();
    let output = std::process::Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("run.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = std::path::PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
        .canonicalize()
        .unwrap();
    assert!(root.is_absolute());
    std::fs::write(root.join(".flea-test-sandbox"), "Quick Look symlink join").unwrap();
    let target = if linked { "photo.png" } else { "link.png" };
    image::RgbaImage::from_pixel(1600, 1200, image::Rgba([40, 120, 200, 255]))
        .save(root.join(target))
        .unwrap();
    if linked {
        std::os::unix::fs::symlink("photo.png", root.join("link.png")).unwrap();
    }
    let (shell, cx, mut requests) = opened_at(cx, &root);
    let mut row = file("link.png", "image-x-generic");
    if linked {
        row.mode = 0o120777;
        row.link = Some("photo.png".into());
    }
    land(&shell, cx, &root.to_string_lossy(), vec![row], 1);
    set_cursor(&shell, cx, 0);
    press(cx, "space");
    let asks = sent(&mut requests, "meta");
    assert_eq!(asks.len(), 1);
    let target = if linked { "photo.png" } else { "" };
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"meta","row":0,"token":{},"w":1600,"h":1200,"orient":1,"target":"{target}"}}"#,
            token(&asks[0])
        ),
    );
    let start = std::time::Instant::now();
    loop {
        draw(cx);
        if shell.read_with(cx, |shell, cx| {
            shell
                .quick_look
                .as_ref()
                .unwrap()
                .view
                .read(cx)
                .svg_raster()
                .is_some()
        }) {
            break;
        }
        assert!(
            start.elapsed() < std::time::Duration::from_secs(30),
            "the target decodes"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let card = cx.debug_bounds("pv1-card").expect("the image card draws");
    let picture = cx
        .debug_bounds("pv1-picture")
        .expect("the target image draws");
    if linked {
        assert!(
            matches!(body(&shell, cx), Some(Body::Link { target, .. }) if target == "→ photo.png")
        );
        let bar = cx.debug_bounds("pv1-bar").expect("the link bar draws");
        assert!(cx.debug_bounds("pv1-bar-mark:flea-mark:symlink").is_some());
        assert_eq!(bar.origin.y, card.origin.y);
        assert_eq!(bar.size.height, px(27.));
        assert_eq!(picture.origin.y, bar.origin.y + bar.size.height);
        assert_eq!(picture.size.height, card.size.height - bar.size.height);
        assert_eq!(picture.size.width, px(729.));
        assert!(shell.read_with(cx, |shell, cx| {
            shell
                .quick_look
                .as_ref()
                .unwrap()
                .view
                .read(cx)
                .subject()
                .unwrap()
                .name
                == "link.png"
        }));
    } else {
        assert!(matches!(body(&shell, cx), Some(Body::Image { .. })));
        assert!(cx.debug_bounds("pv1-bar").is_none());
        assert_eq!(picture.origin.y, card.origin.y);
        assert_eq!(picture.size.height, card.size.height);
        assert_eq!(picture.size.width, px(765.));
    }
    close(&shell, cx);
}

#[gpui::test]
fn a_large_link_target_starts_below_the_link_bar(cx: &mut TestAppContext) {
    image(cx, true);
}

#[gpui::test]
fn a_plain_copy_named_link_png_draws_no_bar(cx: &mut TestAppContext) {
    image(cx, false);
}
