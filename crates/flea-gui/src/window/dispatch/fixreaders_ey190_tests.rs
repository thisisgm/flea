//! EY-190: with Quick Look open on a picture, previewPictureRect and previewSurfaceRect answer the boxes the card draws.
use super::super::gh_preview_proof_tests::opened_at;
use super::super::preview_tests::{answer, close, draw, file, press, sent, token};
use super::super::test_shell::{keystroke, land, set_cursor};
use crate::window::u07_tests::read;
use gpui::{Bounds, Pixels, TestAppContext};

// The photo's pixel size, which the meta answer must repeat; 4:1 letterboxes in the card, so the picture box is not the surface's.
const PHOTO: (u32, u32) = (2400, 600);
// How long the decode may take, and how often the test looks.
const DECODE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);
const POLL: std::time::Duration = std::time::Duration::from_millis(5);

/// The marked run root, removed when the test ends, pass or fail, and only while it holds the marker.
struct Sandbox(std::path::PathBuf);

impl Drop for Sandbox {
    fn drop(&mut self) {
        if self.0.join(".flea-test-sandbox").is_file() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn rect(bounds: Bounds<Pixels>) -> String {
    format!(
        "{} {} {} {}",
        f32::from(bounds.left()).round(),
        f32::from(bounds.top()).round(),
        f32::from(bounds.size.width).round(),
        f32::from(bounds.size.height).round()
    )
}

// A rect reader's reply, for example "412 180 1096 822".
fn edges(text: &str) -> [f32; 4] {
    let numbers: Vec<f32> = text
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    numbers
        .try_into()
        .unwrap_or_else(|_| panic!("a rect: {text:?}"))
}

#[gpui::test]
fn fixreaders_ey190_picture_and_surface_rects_read_the_open_quick_look(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let parent = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.superpowers/fixreaders-ey190");
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
    std::fs::write(root.join(".flea-test-sandbox"), "EY-190 picture reader").unwrap();
    let sandbox = Sandbox(root.clone());
    image::RgbaImage::from_pixel(PHOTO.0, PHOTO.1, image::Rgba([40, 120, 200, 255]))
        .save(root.join("photo.png"))
        .unwrap();
    let (shell, cx, mut requests) = opened_at(cx, &root);
    land(
        &shell,
        cx,
        &root.to_string_lossy(),
        vec![file("photo.png", "image-png")],
        1,
    );
    set_cursor(&shell, cx, 0);
    assert_eq!(read(&shell, cx, "previewPictureRect", &[]), "");
    press(cx, "space");
    let asks = sent(&mut requests, "meta");
    assert_eq!(asks.len(), 1);
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"meta","row":0,"token":{},"w":{},"h":{},"orient":1,"target":""}}"#,
            token(&asks[0]),
            PHOTO.0,
            PHOTO.1
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
        assert!(start.elapsed() < DECODE_DEADLINE, "the picture decodes");
        std::thread::sleep(POLL);
    }
    let card = cx.debug_bounds("pv1-card").expect("the image card draws");
    let surface = cx
        .debug_bounds("pv1-surface")
        .expect("the picture area draws");
    let picture = cx.debug_bounds("pv1-picture").expect("the picture draws");
    assert_eq!(read(&shell, cx, "previewPictureRect", &[]), rect(picture));
    assert_eq!(read(&shell, cx, "previewSurfaceRect", &[]), rect(surface));
    let [sx, sy, sw, sh] = edges(&rect(surface));
    let [px, py, pw, ph] = edges(&rect(picture));
    let [cx0, cy0, cw, ch] = edges(&rect(card));
    // The surface is the card's picture area: inside the card, holding the fitted picture.
    assert!(sx >= cx0 && sy >= cy0 && sx + sw <= cx0 + cw && sy + sh <= cy0 + ch);
    assert!(px >= sx && py >= sy && px + pw <= sx + sw && py + ph <= sy + sh);
    assert_ne!(
        rect(picture),
        rect(surface),
        "the letterboxed picture is not the whole area"
    );
    // At fit there is no caption bar, so the surface is the whole card.
    assert_eq!(rect(surface), rect(card));
    // Zoomed, the caption bar takes the card's foot, so the surface is no longer the whole card.
    let zoom_in = flea::keys::binding_rows("default", flea::keys::Frontend::Gui)
        .find(|binding| binding.action == "zoomIn")
        .map(keystroke)
        .expect("default binds zoomIn");
    cx.simulate_event(zoom_in);
    draw(cx);
    let zoomed_card = cx.debug_bounds("pv1-card").expect("the zoomed card draws");
    assert_eq!(
        rect(zoomed_card),
        rect(card),
        "zoom keeps the card's own box"
    );
    let surface = cx
        .debug_bounds("pv1-surface")
        .expect("the zoomed picture area draws");
    let zoomed_picture = cx
        .debug_bounds("pv1-picture")
        .expect("the zoomed picture draws");
    assert_eq!(read(&shell, cx, "previewSurfaceRect", &[]), rect(surface));
    assert_eq!(
        read(&shell, cx, "previewPictureRect", &[]),
        rect(zoomed_picture)
    );
    assert_ne!(rect(zoomed_picture), rect(picture), "zoomIn took effect");
    let [zx, zy, zw, zh] = edges(&rect(surface));
    assert_eq!(
        [zx, zy, zw, zh],
        [sx, sy, sw, sh - crate::g2::quick_look_overlay::look::BAR],
        "the caption bar takes only the card's foot"
    );
    // A closed Quick Look answers no box, never the last one drawn.
    press(cx, "escape");
    draw(cx);
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
    assert_eq!(read(&shell, cx, "previewPictureRect", &[]), "");
    assert_eq!(read(&shell, cx, "previewSurfaceRect", &[]), "");
    close(&shell, cx);
    drop(sandbox);
}
