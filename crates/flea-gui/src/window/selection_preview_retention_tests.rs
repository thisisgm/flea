//! Count the atlas resources retained by each playback-to-selection transition, independent of RSS.
use super::*;
use crate::{g2::media_preview::engine, list::Typeface};
use gpui::{AppContext, TestAppContext};

struct UnusedDecoder;
impl engine::Backend for UnusedDecoder {
    fn start(
        &self,
        _: &engine::Request,
    ) -> Result<(Box<dyn engine::Playback>, engine::Feed), String> {
        panic!("the test supplies the decoded frame without starting a helper")
    }
}

const PICTURE_SIDE: u32 = 2;
const BGRA_PIXEL_BYTES: u32 = 4;
const PICTURE_BYTES: usize = (PICTURE_SIDE * PICTURE_SIDE * BGRA_PIXEL_BYTES) as usize;

fn picture() -> Arc<RenderImage> {
    Arc::new(RenderImage::from_bgra(PICTURE_SIDE, PICTURE_SIDE, vec![255; PICTURE_BYTES]).unwrap())
}

fn subject(step: usize) -> Subject {
    Subject {
        directory: "/fixture".into(),
        listing: step,
        index: 0,
        row: RowFacts {
            name: format!("clip-{step}.mp4"),
            icon: "video-x-generic".into(),
            kind: "MPEG-4 video".into(),
            mode: 0o100644,
            ..Default::default()
        },
        selected: Vec::new(),
        selected_count: 1,
    }
}

fn frame(step: usize, image: Arc<RenderImage>) -> Frame {
    Frame {
        subject: subject(step),
        generation: step as u64,
        kind: Kind::Video,
        mounted: Mounted::Media(None),
        meta: None,
        facts: None,
        thumb_answered: true,
        thumb_observation: None,
        source: None,
        image: Some(image),
        animation: None,
        animation_source: None,
        quality: Default::default(),
        trace: None,
        failed: false,
        manual: false,
        remote: false,
        _watch: None,
    }
}

fn walk(cx: &mut TestAppContext, busy: bool) {
    let (preview, cx) = cx.add_window_view(|_, cx| {
        let mut view = SelectionPreview::new(
            Port::default(),
            Arc::new(crate::test_catalogs::english()),
            Typeface::omarchy_default(),
            cx,
        );
        view.width = 280.;
        view.height = 600.;
        view.visible = true;
        view
    });
    let mut posters = Vec::new();
    let mut retained_per_step = Vec::new();
    for step in 0..16 {
        let poster = picture();
        let decoded = picture();
        cx.update(|_, cx| {
            preview.update(cx, |view, cx| {
                view.drawn = Some(frame(step, poster.clone()));
                cx.notify();
            });
        });
        cx.run_until_parked();
        assert!(
            cx.update(|window, _| window.has_image_atlas_entry(&poster)),
            "step {step}, busy {busy}: the poster never reached the atlas"
        );
        cx.update(|window, cx| {
            let player = cx.new(|cx| {
                let mut player = MediaPlayer::new(Rc::new(UnusedDecoder), None, cx);
                player.frame = Some(decoded.clone());
                player
            });
            preview.update(cx, |view, cx| {
                view.drawn.as_mut().unwrap().mounted = Mounted::Media(Some(player));
                let next = (!busy).then(|| subject(step + 1));
                view.follow(next, true, false, window, cx);
                cx.notify();
            });
        });
        cx.run_until_parked();
        let shown = cx.update(|_, cx| {
            preview
                .read(cx)
                .drawn
                .as_ref()
                .and_then(|f| f.image.clone())
        });
        assert!(
            shown.is_some_and(|image| Arc::ptr_eq(&image, &decoded)),
            "step {step}, busy {busy}: playback stop did not leave the decoded frame drawn"
        );
        assert!(
            cx.update(|window, _| window.has_image_atlas_entry(&decoded)),
            "step {step}, busy {busy}: the decoded frame never reached the atlas"
        );
        posters.push(poster);
        let retained = cx.update(|window, _| {
            posters
                .iter()
                .filter(|image| window.has_image_atlas_entry(image))
                .count()
        });
        println!(
            "preview step {step}: retired poster textures={retained}, pixel_bytes={}",
            retained * PICTURE_BYTES
        );
        retained_per_step.push(retained);
        cx.update(|window, cx| preview.update(cx, |view, cx| view.hide(window, cx)));
        cx.run_until_parked();
        assert!(
            !cx.update(|window, _| window.has_image_atlas_entry(&decoded)),
            "step {step}, busy {busy}: hide left the decoded frame in the atlas"
        );
    }
    assert_eq!(
        retained_per_step,
        vec![0; 16],
        "retired posters must not accumulate per preview step"
    );
}

#[gpui::test]
fn playback_to_busy_selection_releases_every_retired_poster(cx: &mut TestAppContext) {
    walk(cx, true);
}

#[gpui::test]
fn playback_to_next_subject_releases_every_retired_poster(cx: &mut TestAppContext) {
    walk(cx, false);
}

#[gpui::test]
fn playback_without_a_decoded_frame_keeps_its_poster_until_hide(cx: &mut TestAppContext) {
    let poster = picture();
    let (preview, cx) = cx.add_window_view(|_, cx| {
        let mut view = SelectionPreview::new(
            Port::default(),
            Arc::new(crate::test_catalogs::english()),
            Typeface::omarchy_default(),
            cx,
        );
        view.width = 280.;
        view.height = 600.;
        view.visible = true;
        view.drawn = Some(frame(0, poster.clone()));
        view
    });
    cx.run_until_parked();
    assert!(cx.update(|window, _| window.has_image_atlas_entry(&poster)));
    cx.update(|window, cx| {
        let player = cx.new(|cx| MediaPlayer::new(Rc::new(UnusedDecoder), None, cx));
        preview.update(cx, |view, cx| {
            view.drawn.as_mut().unwrap().mounted = Mounted::Media(Some(player));
            view.follow(None, false, false, window, cx);
            cx.notify();
        });
    });
    cx.run_until_parked();
    assert!(cx.update(|window, cx| {
        let image = preview
            .read(cx)
            .drawn
            .as_ref()
            .unwrap()
            .image
            .as_ref()
            .unwrap();
        Arc::ptr_eq(image, &poster) && window.has_image_atlas_entry(&poster)
    }));
    cx.update(|window, cx| preview.update(cx, |view, cx| view.hide(window, cx)));
    cx.run_until_parked();
    assert!(!cx.update(|window, _| window.has_image_atlas_entry(&poster)));
}
