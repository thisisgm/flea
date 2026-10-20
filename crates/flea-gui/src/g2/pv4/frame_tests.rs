//! The frame worker: prepaint only hands frames out, a moving view reuses its buffers, pixels reach GPUI with no decode, every presented frame leaves the atlas, and the real cost per mesh size and scale.
use super::{
    live::{Job, Live, Model},
    seam::Host,
    tests::{frame, load, open, stl_box, with, DARK},
};
use flea::ui_logic_pv4::{
    camera,
    color::Palette,
    control::Surface,
    raster::{Raster, Style, TOOLPATH_SEGMENTS},
    scene::{Layer, Scene, Segment, CPU_LIVE_TRIANGLES},
};
use gpui::{AppContext, TestAppContext};
use std::sync::Arc;

#[gpui::test]
fn a_frame_is_drawn_off_the_main_thread_and_presented_when_it_lands(cx: &mut TestAppContext) {
    let (h, _) = open(cx, Surface::QuickLook, true);
    load(cx, h, "/p/bracket.stl", "/c/bracket.3d", "stl");
    let rasters = |cx: &mut TestAppContext| with(cx, h, |v, _| v.live.borrow().counters.rasters);
    let before = rasters(cx);
    with(cx, h, |v, cx| {
        v.host = Host {
            animations: false,
            ..Host::default()
        };
        cx.notify();
    });
    assert!(
        with(cx, h, |v, _| v.live.borrow().busy()),
        "the redraw handed the still to a worker"
    );
    assert_eq!(rasters(cx), before, "prepaint rasterized nothing itself");
    cx.run_until_parked();
    assert_eq!(rasters(cx), before + 1);
    assert!(with(cx, h, |v, _| v.presented.borrow().image.is_some()));
}

// A wavy sheet of `n` or a few more triangles over 60 by 40 mm, a stand-in for a scanned part.
fn sheet(n: usize) -> Scene {
    let k = ((n / 2) as f32).sqrt().ceil() as u32;
    let mut s = Scene::default();
    for j in 0..=k {
        for i in 0..=k {
            let (u, v) = (i as f32 / k as f32, j as f32 / k as f32);
            s.positions.push([
                20. + 60. * u,
                20. + 40. * v,
                10. + 6. * (u * 19.).sin() * (v * 13.).cos(),
            ]);
        }
    }
    let at = |i: u32, j: u32| j * (k + 1) + i;
    for j in 0..k {
        for i in 0..k {
            s.triangles.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
            s.triangles.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
        }
    }
    s
}

// Lands a frame as the worker and the atlas do: the pixels become the picture as they are, and its buffer goes back to the model once nothing holds it; answers the picture's bytes and whether the raster came back empty.
fn land(live: &mut Live, job: Job) -> (Vec<u8>, bool) {
    let mut drawn = job.draw();
    let released = drawn.raster.width() == 0;
    let image = drawn.picture().expect("a picture the frame's size");
    let picture = image.as_bytes(0).expect("one frame").to_vec();
    assert!(live.finish(drawn));
    live.recycle(image);
    (picture, released)
}

#[test]
fn the_ui_thread_only_hands_out_frames_and_a_moving_view_reuses_its_buffers() {
    let mut live = Live::new(Surface::QuickLook);
    live.set(
        Some(Model::build(stl_box([20., 20., 0.], [80., 60., 40.]), 0, DARK).unwrap()),
        false,
    );
    let host = Host::default();
    let job = live
        .frame(0., &host, (900., 545.), 1., DARK)
        .job
        .expect("the first frame is handed out");
    assert!(
        live.busy()
            && live
                .frame(0.016, &host, (900., 545.), 1., DARK)
                .job
                .is_none(),
        "one frame in flight at a time"
    );
    assert_eq!(
        live.counters.rasters, 0,
        "nothing is rasterized on the UI thread"
    );
    let (picture, _) = land(&mut live, job);
    assert!(
        picture.len() == 900 * 545 * 4 && picture.iter().skip(3).step_by(4).all(|&a| a == 255),
        "an opaque BGRA picture the card's size"
    );
    let grown = live.counters.allocations;
    assert!(grown > 0);
    assert_eq!(
        live.counters.pixel_allocations, 1,
        "the first frame grows the pixel buffer once"
    );
    for i in 2..32 {
        let frame = live.frame(i as f64 / 60., &host, (900., 545.), 1., DARK);
        assert!(frame.moving);
        land(&mut live, frame.job.expect("a moving frame"));
        assert!(
            live.counters.last.triangles <= 6,
            "only faces toward the viewer fill: {:?}",
            live.counters.last
        );
        assert_eq!(live.counters.last.samples, 900 * 545);
        assert_eq!(
            live.counters.last.resolved, 0,
            "a moving frame hands over the samples it drew, with no copy"
        );
    }
    assert_eq!(
        live.counters.allocations, grown,
        "moving frames reuse every buffer"
    );
    assert_eq!(
        live.counters.pixel_allocations, 1,
        "moving frames reuse the pixel buffer, so each frame grows none"
    );
    assert_eq!(
        live.counters.recycled, live.counters.rasters,
        "each frame's buffer comes back from its picture"
    );
    assert!(
        !live.model.as_ref().unwrap().has_edges(),
        "edges wait for the wireframe"
    );
    let still = Host {
        animations: false,
        ..Host::default()
    };
    let job = live
        .frame(1., &still, (900., 545.), 2., DARK)
        .job
        .expect("the still at scale 2");
    assert!(
        land(&mut live, job).1,
        "a still hands its buffers back empty"
    );
    assert_eq!(live.counters.last.samples, 1800 * 1090 * 4);
    assert_eq!(
        live.counters.last.resolved,
        1800 * 1090,
        "a still resolves its four samples a pixel"
    );
    assert!(
        live.frame(1.1, &still, (900., 545.), 2., DARK)
            .job
            .is_none(),
        "an unchanged still is not drawn again"
    );
    live.viewer.as_mut().unwrap().wire = true;
    let job = live
        .frame(1.2, &still, (900., 545.), 2., DARK)
        .job
        .expect("the wireframe");
    land(&mut live, job);
    assert!(live.model.as_ref().unwrap().has_edges());
}

#[test]
fn a_frame_drawn_for_an_old_model_is_dropped_when_it_lands() {
    let mut live = Live::new(Surface::QuickLook);
    live.set(
        Some(Model::build(stl_box([20., 20., 0.], [80., 60., 40.]), 0, DARK).unwrap()),
        false,
    );
    let job = live
        .frame(0., &Host::default(), (300., 200.), 1., DARK)
        .job
        .unwrap();
    live.set(
        Some(Model::build(stl_box([0.; 3], [10.; 3]), 0, DARK).unwrap()),
        false,
    );
    assert!(!live.finish(job.draw()), "the old frame is never presented");
    assert!(
        live.frame(0., &Host::default(), (300., 200.), 1., DARK)
            .job
            .is_some(),
        "the new model draws at once"
    );
}

#[test]
fn a_bad_scene_fails_the_build_and_a_mesh_past_the_cpu_limit_is_decimated() {
    let mut past = stl_box([0.; 3], [10.; 3]);
    past.triangles[3] = [0, 1, 99];
    let mut far = stl_box([0.; 3], [10.; 3]);
    far.positions[2][1] = f32::INFINITY;
    let plate = Scene {
        bed: Some([1e30, 250.]),
        ..stl_box([0.; 3], [10.; 3])
    };
    for scene in [past, far, plate, Scene::default()] {
        assert!(Model::build(scene, 0, DARK).is_err());
    }
    let big = sheet(CPU_LIVE_TRIANGLES + 20_000);
    assert!(big.triangles.len() > CPU_LIVE_TRIANGLES);
    let model = Model::build(big, 0, DARK).unwrap();
    assert!(
        model.scene.triangles.len() <= CPU_LIVE_TRIANGLES
            && model.scene.triangles.len() > CPU_LIVE_TRIANGLES / 4,
        "{}",
        model.scene.triangles.len()
    );
}

#[gpui::test]
fn every_presented_frame_leaves_the_atlas_across_motion_a_hide_and_a_file_change(
    cx: &mut TestAppContext,
) {
    let (h, _) = open(cx, Surface::QuickLook, true);
    load(cx, h, "/p/bracket.stl", "/c/bracket.3d", "stl");
    let presented = with(cx, h, |v, _| v.presented.clone());
    for _ in 0..6 {
        frame(cx, h);
        cx.run_until_parked();
    }
    let first = presented.borrow().image.clone().expect("a frame on screen");
    let atlas = |cx: &mut TestAppContext, image: &Arc<gpui::RenderImage>| {
        cx.update_window(h.into(), |_, window, _| window.has_image_atlas_entry(image))
            .unwrap()
    };
    assert!(atlas(cx, &first), "the frame on screen is in the atlas");
    for _ in 0..6 {
        frame(cx, h);
        cx.run_until_parked();
    }
    let (shown, dropped) = (presented.borrow().shown, presented.borrow().dropped);
    assert!(shown >= 6, "{shown}");
    assert_eq!(
        dropped + presented.borrow().retired(),
        shown - 1,
        "every frame but the one on screen is retired"
    );
    assert!(!atlas(cx, &first), "a retired frame left the atlas");
    let last = presented.borrow().image.clone().unwrap();
    with(cx, h, |v, cx| v.show("/p/knob.stl", 0, cx));
    cx.run_until_parked();
    assert!(
        presented.borrow().image.is_none() && !atlas(cx, &last),
        "another file takes the old frame off at once"
    );
    load(cx, h, "/p/knob.stl", "/c/knob.3d", "stl");
    frame(cx, h);
    cx.run_until_parked();
    let knob = presented
        .borrow()
        .image
        .clone()
        .expect("the new file's frame");
    with(cx, h, |v, cx| v.hide(cx));
    cx.run_until_parked();
    assert!(!atlas(cx, &knob), "hiding frees the last frame");
    assert_eq!(presented.borrow().dropped, presented.borrow().shown);
}

extern "C" {
    fn clock_gettime(clock: i32, now: *mut [i64; 2]) -> i32;
}

// This thread's CPU time in seconds (CLOCK_THREAD_CPUTIME_ID), which other work on the bench host does not inflate.
fn cpu() -> f64 {
    let mut now = [0i64; 2];
    // SAFETY: `now` has the layout of a 64-bit Linux timespec and outlives the call.
    unsafe { clock_gettime(3, &mut now) };
    now[0] as f64 + now[1] as f64 / 1e9
}

// The real cost per mesh size and scale in thread CPU time, three runs each, printed for RESULT.md; run with --ignored at opt-level 3.
#[gpui::test]
#[ignore]
fn frame_costs_off_and_on_the_ui_thread(_: &mut TestAppContext) {
    let host = Host::default();
    for (name, scene) in [
        ("box 12", stl_box([20., 20., 0.], [80., 60., 40.])),
        ("sheet 100k", sheet(100_000)),
        ("sheet 2M", sheet(2_000_000)),
        ("spiral 200k", spiral(200, 1_000)),
        ("spiral 4M", spiral(2_000, 2_000)),
    ] {
        for (scale, ss) in [(1., 1), (2., 1), (2., 2)] {
            for rep in 0..3 {
                let model = Model::build(scene.clone(), 0, DARK).unwrap();
                let tris = model.scene.triangles.len();
                let mut live = Live::new(Surface::QuickLook);
                live.set(Some(model), false);
                let still = Host {
                    animations: ss == 1,
                    ..host
                };
                let (mut ui, mut draw, mut handoff, frames) = (0f64, 0f64, 0f64, 20);
                for i in 0..frames {
                    let t = cpu();
                    let job = live.frame(
                        i as f64 / 60. + scale as f64 * 10. + ss as f64,
                        &still,
                        (900., 545.),
                        scale,
                        if i % 2 == 0 {
                            DARK
                        } else {
                            Palette {
                                ground: [0.09; 3],
                                ..DARK
                            }
                        },
                    );
                    ui += cpu() - t;
                    let Some(job) = job.job else { continue };
                    let t = cpu();
                    let mut drawn = job.draw();
                    draw += cpu() - t;
                    let t = cpu();
                    let image = drawn.picture().unwrap();
                    handoff += cpu() - t;
                    live.finish(drawn);
                    live.recycle(image);
                }
                let ms = |s: f64| s * 1000. / frames as f64;
                let (samples, segments) = (live.counters.last.samples, live.counters.last.segments);
                let c = live.counters;
                eprintln!("PV4COST {name} ({tris} drawn, {samples} samples, {segments} extrusions) scale {scale} ss {ss} rep {rep}: ui {:.3} ms, worker raster {:.2} ms, worker handoff {:.3} ms, sum {:.2} ms, pixel grows {}, recycled {}", ms(ui), ms(draw), ms(handoff), ms(draw + handoff), c.pixel_allocations, c.recycled);
            }
        }
    }
}

// A print of `layers` layers of `per_layer` extrusions each, a spiral from the centre out, `2 * layers * per_layer` moves in all but one per extrusion.
fn spiral(layers: usize, per_layer: usize) -> Scene {
    let mut s = Scene {
        bed: Some([250., 210.]),
        ..Scene::default()
    };
    for l in 0..layers {
        let z = 0.2 * (l + 1) as f32;
        let start = s.segments.len() as u32;
        let mut prev = [125., 105., z];
        for k in 0..per_layer {
            let a = k as f32 * 0.07;
            let r = 4. + 90. * k as f32 / per_layer as f32;
            let p = [125. + r * a.cos(), 105. + r * a.sin(), z];
            s.segments.push(Segment {
                a: prev,
                b: p,
                extrude: true,
            });
            prev = p;
        }
        s.layers.push(Layer {
            segments: start..s.segments.len() as u32,
            z,
        });
    }
    s
}

#[test]
fn a_four_million_segment_print_draws_within_the_toolpath_budget() {
    let mut live = Live::new(Surface::QuickLook);
    live.set(
        Some(Model::build(spiral(2_000, 2_000), 0, DARK).unwrap()),
        false,
    );
    let host = Host {
        animations: false,
        ..Host::default()
    };
    let job = live
        .frame(0., &host, (900., 545.), 1., DARK)
        .job
        .expect("the first frame");
    land(&mut live, job);
    let drawn = live.counters.last.segments;
    assert!(
        drawn > 0 && drawn <= TOOLPATH_SEGMENTS + 2_000,
        "drew {drawn} of 4000000 extrusions"
    );
}

#[test]
fn a_part_far_from_the_origin_previews_like_one_at_the_origin() {
    let picture = |scene: Scene| {
        let mut live = Live::new(Surface::QuickLook);
        live.set(Some(Model::build(scene, 0, DARK).unwrap()), false);
        let job = live
            .frame(0., &Host::default(), (300., 200.), 1., DARK)
            .job
            .expect("the first frame");
        land(&mut live, job).0
    };
    let origin = picture(stl_box([0., 0., 0.], [100., 80., 40.]));
    assert!(
        picture(stl_box([15_000., 0., 0.], [15_100., 80., 40.])) == origin,
        "a part at x = 15 m"
    );
    // Corners exact in f32 at 1e9 mm (a 64 mm step there), so the two pictures match bit for bit.
    let origin = picture(stl_box([0., 0., 0.], [128., 128., 40.]));
    assert!(
        picture(stl_box([1e9 - 128., 1e9 - 128., 0.], [1e9, 1e9, 40.])) == origin,
        "a part at the parser's 1e9 mm"
    );
}

#[gpui::test]
fn moving_frames_reuse_their_pixel_buffers_through_the_atlas(cx: &mut TestAppContext) {
    let (h, _) = open(cx, Surface::QuickLook, true);
    load(cx, h, "/p/bracket.stl", "/c/bracket.3d", "stl");
    let counters = |cx: &mut TestAppContext| with(cx, h, |v, _| v.live.borrow().counters);
    for _ in 0..10 {
        frame(cx, h);
        cx.run_until_parked();
    }
    let warm = counters(cx);
    for _ in 0..30 {
        frame(cx, h);
        cx.run_until_parked();
    }
    let c = counters(cx);
    assert!(c.rasters >= warm.rasters + 20, "{c:?}");
    assert!(
        warm.pixel_allocations <= 3,
        "a buffer drawing, one on screen and one leaving the atlas: {warm:?}"
    );
    assert_eq!(
        c.pixel_allocations, warm.pixel_allocations,
        "warm moving frames grow no pixel buffer: {c:?}"
    );
    assert_eq!(
        c.recycled - warm.recycled,
        c.rasters - warm.rasters,
        "each picture that leaves the screen gives its buffer back: {c:?}"
    );
}

#[test]
fn a_warm_moving_frame_reaches_gpui_without_a_decode_or_a_new_buffer() {
    let mut live = Live::new(Surface::QuickLook);
    live.set(
        Some(Model::build(stl_box([20., 20., 0.], [80., 60., 40.]), 0, DARK).unwrap()),
        false,
    );
    let mut counts = Vec::new();
    for i in 0..8 {
        let job = live
            .frame(i as f64 / 60., &Host::default(), (900., 545.), 2., DARK)
            .job
            .expect("a moving frame");
        let before = crate::list::bench::allocations();
        let mut drawn = job.draw();
        let image = drawn.picture().expect("the frame's picture");
        counts.push(crate::list::bench::allocations() - before);
        assert!(live.finish(drawn));
        live.recycle(image);
    }
    assert!(
        counts[0] > 1,
        "the first frame sizes its buffers: {counts:?}"
    );
    assert!(counts[1..].iter().all(|&n| n == 1), "a warm frame allocates only the picture's shared handle, so no decode runs and no buffer grows: {counts:?}");
}

#[gpui::test]
fn the_handed_over_pixels_are_the_bytes_the_ppm_decode_gave(cx: &mut TestAppContext) {
    let svg = cx.update(|cx| cx.svg_renderer());
    let model = Model::build(stl_box([20., 20., 0.], [80., 60., 40.]), 0, DARK).unwrap();
    let (view, rect, _) = camera::home(&model.scene, 0, 450., 272.).unwrap();
    for ss in [1, 2] {
        let mut raster = Raster::default();
        raster.resize(450, 272, ss);
        raster.render(
            &model.scene,
            &model.prepared,
            0,
            &view,
            &Style {
                palette: DARK,
                wire: None,
                layer: None,
                plate: Some(rect),
                ground: Some(DARK.ground),
            },
        );
        let (mut rgb, mut bgra) = (Vec::new(), Vec::new());
        raster.rgb_into(&mut rgb);
        raster.frame_into(&mut bgra);
        let mut ppm = b"P6\n450 272\n255\n".to_vec();
        ppm.extend(rgb);
        let decoded = gpui::Image::from_bytes(gpui::ImageFormat::Pnm, ppm)
            .to_image_data(svg.clone())
            .unwrap();
        let handed = gpui::RenderImage::from_bgra(450, 272, bgra).unwrap();
        assert!(
            decoded.as_bytes(0) == handed.as_bytes(0),
            "{ss} samples a side: the atlas gets the same bytes the decode gave"
        );
    }
}
