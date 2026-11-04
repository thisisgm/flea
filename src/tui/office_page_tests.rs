use super::tests::*;
use crate::tui::{graphics::Graphics, stage, wire};
use std::{
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::Path,
    time::Duration,
};

extern "C" {
    fn dup(fd: i32) -> i32;
    fn dup2(from: i32, to: i32) -> i32;
}

// Sends fd 1 to one capture file until dropped, so a graphics delete on stdout can be checked.
struct StdoutTo {
    saved: OwnedFd,
}
impl StdoutTo {
    fn to(path: &Path) -> Self {
        let file = std::fs::File::create(path).unwrap();
        let saved = unsafe { dup(1) };
        assert!(saved >= 0);
        let saved = unsafe { OwnedFd::from_raw_fd(saved) };
        assert_eq!(unsafe { dup2(file.as_raw_fd(), 1) }, 1);
        Self { saved }
    }
}
impl Drop for StdoutTo {
    fn drop(&mut self) {
        use std::io::Write;
        let _ = std::io::stdout().flush();
        unsafe { dup2(self.saved.as_raw_fd(), 1) };
    }
}

#[test]
fn tui_office_r4_stdout_captures_close_saved_descriptors() {
    const CHILD: &str = "FLEA_OFFICE_FD_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new("/usr/bin/setsid")
            .arg("-w")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "tui::office::page_tests::tui_office_r4_stdout_captures_close_saved_descriptors",
                "--test-threads=1",
            ])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env(CHILD, "1")
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{stdout}{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(stdout.contains("test result: ok. 1 passed;"), "{stdout}");
        return;
    }
    let root = Fixture::new();
    let capture = root.path("capture.txt");
    let count = || std::fs::read_dir("/proc/self/fd").unwrap().count();
    let before = count();
    for _ in 0..16 {
        drop(StdoutTo::to(&capture));
    }
    assert_eq!(count(), before, "stdout captures leaked saved descriptors");
    assert!(std::panic::catch_unwind(|| {
        let _capture = StdoutTo::to(&capture);
        panic!("unwind a stdout capture");
    })
    .is_err());
    assert_eq!(count(), before, "unwinding leaked the saved descriptor");
}

#[test]
fn tui_office_thumbnail_shows_until_the_page_replaces_it() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = Fixture::new();
    let source = root.path(DOCX);
    let converted = root.path("converted.pdf");
    let thumb = root.path("thumb.png");
    std::fs::write(&source, container(b"docx bytes")).unwrap();
    std::fs::write(&converted, pdf(3, "", "")).unwrap();
    std::fs::write(&thumb, b"thumbnail bytes").unwrap();
    let mut m = model(&root, DOCX, "Word 2007 document");
    m.rows.get_mut(&0).unwrap().thumbnail = true;
    m.capabilities.graphics = crate::tui::capabilities::Protocol::Kitty;
    m.preview_state.cell = Some((8, 16));
    m.media_probe = Some((800, 600, 0));
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    graphics.protocol = crate::tui::capabilities::Protocol::Kitty;
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    let thumb_request = wire
        .events
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(
        wire::text(&thumb_request, "c"),
        "thumb",
        "the embedded thumbnail is asked for at once"
    );
    let (id, _) = request(&mut m, &mut graphics, &mut shown, &mut wire);
    receive(
        &mut m,
        &mut wire,
        &format!(r#"{{"t":"thumbed","row":0,"file":"{}"}}"#, thumb.display()),
    );
    assert_eq!(
        m.image_file.as_deref(),
        Some(thumb.as_path()),
        "the thumbnail shows while the document converts"
    );
    assert!(m.pdf.is_none());
    receive(&mut m, &mut wire, &ready(id, &converted));
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.pdf
            .as_ref()
            .is_some_and(|pdf| pdf.pages == 3 && super::page_drawn(pdf))
    });
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    assert!(
        m.image_file.is_none() && m.thumb_index.is_none(),
        "the converted page replaces the thumbnail"
    );
    drop(graphics);
    wire::finish(wire, reader);
}

#[test]
fn tui_office_thumbnail_holds_until_the_page_is_drawn() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = Fixture::new();
    let source = root.path(DOCX);
    let converted = root.path("converted.pdf");
    let thumb = root.path("thumb.png");
    std::fs::write(&source, container(b"docx bytes")).unwrap();
    std::fs::write(&converted, pdf(3, "", "")).unwrap();
    std::fs::write(&thumb, b"thumbnail bytes").unwrap();
    let mut m = model(&root, DOCX, "Word 2007 document");
    m.rows.get_mut(&0).unwrap().thumbnail = true;
    m.capabilities.graphics = crate::tui::capabilities::Protocol::Kitty;
    m.preview_state.cell = Some((8, 16));
    m.media_probe = Some((800, 600, 0));
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    graphics.protocol = crate::tui::capabilities::Protocol::Kitty;
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    let thumb_request = wire
        .events
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(
        wire::text(&thumb_request, "c"),
        "thumb",
        "the embedded thumbnail is asked for at once"
    );
    let (id, _) = request(&mut m, &mut graphics, &mut shown, &mut wire);
    receive(
        &mut m,
        &mut wire,
        &format!(r#"{{"t":"thumbed","row":0,"file":"{}"}}"#, thumb.display()),
    );
    receive(&mut m, &mut wire, &ready(id, &converted));
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.pdf.is_some()
    });
    let drawn = m.pdf.as_ref().expect("the converted page starts rendering");
    assert!(!super::page_drawn(drawn), "the page is not drawn yet");
    assert_eq!(
        m.image_file.as_deref(),
        Some(thumb.as_path()),
        "the thumbnail stays while the page renders"
    );
    let place = crate::tui::picture::place(&m);
    assert!(
        place.cells.0 > 0 && place.cells.1 > 0,
        "the thumbnail keeps its box"
    );
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.pdf.as_ref().is_some_and(super::page_drawn)
    });
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    assert!(
        m.image_file.is_none() && m.thumb_index.is_none(),
        "the drawn page replaces the thumbnail"
    );
    let cancel = wire
        .events
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(
        wire::text(&cancel, "c"),
        "thumbcancel",
        "the thumbnail request is cancelled once the page is drawn"
    );
    drop(graphics);
    wire::finish(wire, reader);
}

#[test]
fn tui_office_r3_frame_keeps_thumbnail_until_page_bytes_land() {
    let root = Fixture::new();
    let converted = root.path("converted.pdf");
    std::fs::write(&converted, pdf(3, "", "")).unwrap();
    let mut m = model(&root, DOCX, "Word 2007 document");
    m.pdf = Some(crate::tui::pdf::Pdf::new(
        converted,
        crate::tui::capabilities::Protocol::Kitty,
        48,
        21,
        (384, 336),
        "#101315",
        false,
    ));
    let mut graphics = Graphics::new();
    graphics.protocol = crate::tui::capabilities::Protocol::Kitty;
    graphics.bytes = crate::tui::graphics::PLACE.to_vec();
    graphics.transmit = b"thumb-transmit".to_vec();
    let (image, take) = crate::tui::frame_image(m.pdf.as_ref(), &graphics.bytes, true);
    assert_eq!(
        image,
        crate::tui::graphics::PLACE,
        "empty page bytes keep the thumbnail picture"
    );
    assert!(take, "the thumbnail transmission still goes out");
    let transmit = if take {
        std::mem::take(&mut graphics.transmit)
    } else {
        Vec::new()
    };
    assert_eq!(transmit, b"thumb-transmit");
    m.pdf.as_mut().unwrap().bytes = b"page-bytes".to_vec();
    graphics.transmit = b"thumb-transmit".to_vec();
    let (image, take) = crate::tui::frame_image(m.pdf.as_ref(), &graphics.bytes, true);
    assert_eq!(
        image, b"page-bytes",
        "landed page bytes replace the thumbnail"
    );
    assert!(
        !take && graphics.transmit == b"thumb-transmit",
        "a drawn page takes no thumbnail transmission"
    );
}

#[test]
fn tui_office_r3_handoff_keeps_the_page_image() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    const CHILD: &str = "FLEA_OFFICE_HANDOFF_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new("/usr/bin/setsid")
            .arg("-w")
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "tui::office::page_tests::tui_office_r3_handoff_keeps_the_page_image", "--nocapture"])
            .env(CHILD, "1")
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "{stdout}{}", String::from_utf8_lossy(&output.stderr));
        assert!(stdout.contains("OFFICE_HANDOFF_ASSERTIONS_PASSED"), "the isolated assertion path did not run: {stdout}");
        return;
    }
    let root = Fixture::new();
    let source = root.path(DOCX);
    let converted = root.path("converted.pdf");
    let thumb = root.path("thumb.png");
    std::fs::write(&source, container(b"docx bytes")).unwrap();
    std::fs::write(&converted, pdf(3, "", "")).unwrap();
    std::fs::write(&thumb, b"thumbnail bytes").unwrap();
    let mut m = model(&root, DOCX, "Word 2007 document");
    m.rows.get_mut(&0).unwrap().thumbnail = true;
    m.capabilities.graphics = crate::tui::capabilities::Protocol::Kitty;
    m.preview_state.cell = Some((8, 16));
    m.media_probe = Some((800, 600, 0));
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    graphics.protocol = crate::tui::capabilities::Protocol::Kitty;
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    let thumb_request = wire
        .events
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(wire::text(&thumb_request, "c"), "thumb");
    let (id, _) = request(&mut m, &mut graphics, &mut shown, &mut wire);
    receive(
        &mut m,
        &mut wire,
        &format!(r#"{{"t":"thumbed","row":0,"file":"{}"}}"#, thumb.display()),
    );
    receive(&mut m, &mut wire, &ready(id, &converted));
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.pdf.as_ref().is_some_and(super::page_drawn)
    });
    assert_eq!(
        m.image_file.as_deref(),
        Some(thumb.as_path()),
        "the hand-off has not run yet"
    );
    graphics.bytes = crate::tui::graphics::PLACE.to_vec();
    let capture = root.path("capture.txt");
    let guard = StdoutTo::to(&capture);
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    drop(guard);
    let written = String::from_utf8_lossy(&std::fs::read(&capture).unwrap()).into_owned();
    assert!(
        m.image_file.is_none(),
        "the drawn page replaced the thumbnail"
    );
    assert!(
        !written.contains("a=d,d=I,i=42"),
        "the hand-off deleted the page image id 42: {written:?}"
    );
    println!("OFFICE_HANDOFF_ASSERTIONS_PASSED");
    drop(graphics);
    wire::finish(wire, reader);
}

#[test]
fn tui_office_count_label_follows_the_group() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = Fixture::new();
    let source = root.path("deck.odp");
    let converted = root.path("deck.pdf");
    std::fs::write(&source, container(b"odp bytes")).unwrap();
    std::fs::write(&converted, pdf(3, "", "")).unwrap();
    let mut m = model(&root, "deck.odp", "OpenDocument presentation");
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    let (id, _) = request(&mut m, &mut graphics, &mut shown, &mut wire);
    receive(&mut m, &mut wire, &ready(id, &converted));
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.pdf.as_ref().is_some_and(|pdf| pdf.pages == 3)
    });
    let text = screen(&m);
    assert!(
        text.contains("Slides · 3") && !text.contains("Pages · 3"),
        "a presentation counts its slides: {text}"
    );
    drop(graphics);
    wire::finish(wire, reader);
}
