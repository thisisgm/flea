//! Quick Look media geometry is measured from the area the video painter fills.
use super::*;

pub(super) fn mounted_view(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> View {
    let Some(Body::Doc(doc)) = body(shell, cx) else {
        panic!("a document is mounted")
    };
    doc.view
}

#[gpui::test]
fn quick_look_state_video_strip_retires_hidden_paint_geometry(cx: &mut TestAppContext) {
    let (shell, cx, _requests, script) = mounted(cx);
    open_kind(&shell, cx, "clip.mp4");
    assert!(script.feeds.borrow()[0].event(Event::Loaded {
        video: true,
        duration_ms: 20_000,
    }));
    draw(cx);
    let paused = read(&shell, cx, "previewStrip", &[]);
    assert!(paused.starts_with(r#"{"visible":true,"muted":false,"mute":""#));
    assert!(!paused.contains(r#""mute":"""#));
    assert!(script.feeds.borrow()[0].event(Event::Playing));
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(4));
    draw(cx);
    let hidden = read(&shell, cx, "previewStrip", &[]);
    let state = flea::jsondoc::parse(&hidden).unwrap();
    assert_eq!(
        state.get("visible").and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
    assert_eq!(
        state.get("muted").and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
    assert_eq!(
        state.get("mute").and_then(flea::jsondoc::Json::as_str),
        Some("")
    );
    assert_eq!(read(&shell, cx, "previewSliderCentre", &[]), "");
    assert!(script.feeds.borrow()[0].event(Event::Paused));
    draw(cx);
    let paused_again = read(&shell, cx, "previewStrip", &[]);
    assert!(paused_again.starts_with(r#"{"visible":true,"muted":false,"mute":""#));
    assert!(!paused_again.contains(r#""mute":"""#));
    close(&shell, cx);
}

#[gpui::test]
fn fx_uish_video_reader_excludes_the_media_strip(cx: &mut TestAppContext) {
    let (shell, cx, _requests, script) = mounted(cx);
    open_kind(&shell, cx, "clip.mp4");
    assert!(script.feeds.borrow()[0].event(Event::Loaded {
        video: true,
        duration_ms: 20_000,
    }));
    draw(cx);
    let strip = flea::jsondoc::parse(&read(&shell, cx, "previewStrip", &[])).unwrap();
    let media: Vec<f32> = strip
        .get("mediaRect")
        .and_then(flea::jsondoc::Json::as_str)
        .unwrap()
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    let surface: Vec<f32> = read(&shell, cx, "previewSurfaceRect", &[])
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    assert_eq!(media.len(), 4);
    assert_eq!(surface.len(), 4);
    assert!(media[2] > 0. && media[3] > 0.);
    assert_eq!(media[..3], surface[..3]);
    assert!(
        media[3] < surface[3],
        "the video area must leave room for the strip"
    );
    close(&shell, cx);
}
