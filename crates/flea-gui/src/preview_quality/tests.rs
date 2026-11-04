//! The real asset path must retain its first pixels, then cover the physical rect after resizing.
use super::*;
use gpui::{div, img, prelude::*, px, Render, TestAppContext, VisualTestContext};
use std::{
    path::Path,
    time::{Duration, Instant},
};

struct Probe {
    refinement: Refinement,
    path: PathBuf,
    width: f64,
    height: f64,
    image: Arc<RenderImage>,
    active: bool,
    first: Option<(i32, i32)>,
}
impl Render for Probe {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.first.is_none() {
            let size = self.image.size(0);
            self.first = Some((size.width.0, size.height.0));
        }
        if self.active {
            let target = physical(self.width, self.height, window.scale_factor().into());
            if let Some(Ok(image)) =
                self.refinement
                    .poll(self.path.clone(), target, (1, 1), window, cx)
            {
                self.image = image;
            }
        }
        // Laid out as the column's fitted picture is, so the probe measures what production draws.
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                img(self.image.clone())
                    .w(px(self.width as f32))
                    .h(px(self.height as f32))
                    .object_fit(gpui::ObjectFit::Fill)
                    .debug_selector(|| "quality-probe".into()),
            )
    }
}
pub(super) struct Root(pub(super) PathBuf);
impl Root {
    pub(super) fn new() -> Self {
        let output = std::process::Command::new("mktemp")
            .arg("-d")
            .arg(std::env::temp_dir().join("preview-refinement.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(root.is_absolute());
        std::fs::write(root.join(".flea-test-sandbox"), "PREVIEW-QUALITY").unwrap();
        Self(root)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        assert!(
            self.0.is_absolute()
                && self.0.starts_with(std::env::temp_dir())
                && self.0.join(".flea-test-sandbox").is_file()
        );
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}
fn wait(view: &gpui::Entity<Probe>, cx: &mut VisualTestContext, size: (u32, u32)) {
    let start = Instant::now();
    loop {
        draw(cx);
        let actual = view.read_with(cx, |view, _| {
            let size = view.image.size(0);
            (size.width.0 as u32, size.height.0 as u32)
        });
        if actual == size {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "wanted {size:?}, got {actual:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn svg(path: &Path) {
    std::fs::write(path, r#"<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="800"><path d="M0 0L1200 800M0 800L1200 0" stroke="black" stroke-width="0.7"/></svg>"#).unwrap();
}

#[gpui::test]
fn preview_quality_refines_without_a_blank_or_geometry_change(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = Root::new();
    let path = root.0.join("fine.svg");
    svg(&path);
    let (view, cx) = cx.add_window_view(|_, _| Probe {
        refinement: Refinement::default(),
        path,
        width: 301.,
        height: 199.,
        image: Arc::new(RenderImage::from_bgra(8, 8, vec![255; 256]).unwrap()),
        active: true,
        first: None,
    });
    draw(cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.first),
        Some((8, 8)),
        "the thumbnail paints before refinement"
    );
    for scale in [1., 1.25, 1.5, 2.] {
        cx.simulate_scale_factor_change(scale);
        draw(cx);
        let before = cx.debug_bounds("quality-probe").unwrap();
        wait(&view, cx, physical(301., 199., scale.into()));
        assert_eq!(
            cx.debug_bounds("quality-probe").unwrap(),
            before,
            "only pixels change"
        );
    }
    view.update(cx, |v, cx| {
        v.active = false;
        v.refinement.clear(cx);
    });
}

#[gpui::test]
fn preview_quality_vectors_rasterize_again_for_size_and_cancel_old_work(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = Root::new();
    let path = root.0.join("fine.svg");
    svg(&path);
    let (view, cx) = cx.add_window_view(|_, _| Probe {
        refinement: Refinement::default(),
        path,
        width: 101.,
        height: 67.,
        image: Arc::new(RenderImage::from_bgra(8, 8, vec![255; 256]).unwrap()),
        active: true,
        first: None,
    });
    cx.simulate_scale_factor_change(1.25);
    wait(&view, cx, physical(101., 67., 1.25));
    let prior = view.read_with(cx, |v, _| {
        v.refinement.requested.as_ref().unwrap().current.clone()
    });
    view.update(cx, |v, _| {
        v.width = 211.;
        v.height = 139.;
    });
    wait(&view, cx, physical(211., 139., 1.25));
    assert!(
        !prior.load(Ordering::Acquire),
        "resizing cancels the old request"
    );
    let current = view.read_with(cx, |v, _| {
        v.refinement.requested.as_ref().unwrap().current.clone()
    });
    view.update(cx, |v, cx| {
        v.active = false;
        v.refinement.clear(cx);
    });
    assert!(
        !current.load(Ordering::Acquire),
        "selection retirement cancels queued work"
    );
}

#[test]
fn preview_quality_refined_picture_keeps_the_basis_it_was_sized_from() {
    let picture = |w: u32, h: u32| {
        Arc::new(RenderImage::from_bgra(w, h, vec![0; (w * h * 4) as usize]).unwrap())
    };
    let (first, refined, other) = (picture(256, 171), picture(187, 124), picture(300, 200));
    let mut basis = Basis::default();
    assert_eq!(basis.of(&first), (256, 171));
    basis.made(&refined, (256, 171));
    assert_eq!(
        basis.of(&refined),
        (256, 171),
        "a refined picture never resizes its own request"
    );
    assert_eq!(
        basis.of(&other),
        (300, 200),
        "any other picture is its own basis"
    );
}

#[gpui::test]
fn preview_quality_a_dropped_refinement_releases_its_picture(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = Root::new();
    let path = root.0.join("fine.svg");
    svg(&path);
    let (view, cx) = cx.add_window_view(|_, _| Probe {
        refinement: Refinement::default(),
        path,
        width: 101.,
        height: 67.,
        image: Arc::new(RenderImage::from_bgra(8, 8, vec![255; 256]).unwrap()),
        active: true,
        first: None,
    });
    cx.simulate_scale_factor_change(1.25);
    wait(&view, cx, physical(101., 67., 1.25));
    let key = view.read_with(cx, |v, _| v.refinement.requested.clone().unwrap());
    // Replaced without clear, as a dropped owner is; the next refinement's request sweeps it.
    view.update(cx, |v, _| {
        v.refinement = Refinement::default();
        v.width = 103.;
    });
    wait(&view, cx, physical(103., 67., 1.25));
    let kept = view.update(cx, |_, cx| {
        matches!(cx.fetch_asset::<Loader>(&key), Some(Ok(_)))
    });
    assert!(!kept, "the dropped request's picture left the asset cache");
    view.update(cx, |v, cx| {
        v.active = false;
        v.refinement.clear(cx);
    });
}
