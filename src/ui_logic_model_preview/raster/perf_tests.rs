//! Where a moving frame's time goes, stage by stage in thread CPU time, for the 3D view's cost fixtures at the 900 by 545 card, and the pictures those fixtures must keep.
use super::{Prepared, Raster, Style};
use crate::ui_logic_model_preview::camera::{Home, View};
use crate::ui_logic_model_preview::color::Palette;
use crate::ui_logic_model_preview::control::{Surface, Viewer};
use crate::ui_logic_model_preview::raster_tests::DARK;
use crate::ui_logic_model_preview::scene::{decimate, Layer, Scene, Segment, CPU_LIVE_TRIANGLES};
use crate::ui_logic_model_preview::tests::stl_box;

extern "C" {
    fn clock_gettime(clock: i32, now: *mut [i64; 2]) -> i32;
}

// This thread's CPU time in nanoseconds (CLOCK_THREAD_CPUTIME_ID), which other work on the bench host does not inflate.
fn cpu() -> u64 {
    let mut now = [0i64; 2];
    // SAFETY: `now` has the layout of a 64-bit Linux timespec and outlives the call.
    unsafe { clock_gettime(3, &mut now) };
    now[0] as u64 * 1_000_000_000 + now[1] as u64
}

// A wavy sheet of `n` or a few more triangles over 60 by 40 mm, the same as flea-gui's frame_tests.
fn sheet(n: usize) -> Scene {
    let k = ((n / 2) as f32).sqrt().ceil() as u32;
    let mut s = Scene::default();
    for j in 0..=k {
        for i in 0..=k {
            let (u, v) = (i as f32 / k as f32, j as f32 / k as f32);
            s.positions.push([20. + 60. * u, 20. + 40. * v, 10. + 6. * (u * 19.).sin() * (v * 13.).cos()]);
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

// A print of `layers` layers of `per_layer` extrusions spiralling out from the centre, the same as flea-gui's frame_tests.
fn spiral(layers: usize, per_layer: usize) -> Scene {
    let mut s = Scene { bed: Some([250., 210.]), ..Scene::default() };
    for l in 0..layers {
        let z = 0.2 * (l + 1) as f32;
        let start = s.segments.len() as u32;
        let mut prev = [125., 105., z];
        for k in 0..per_layer {
            let a = k as f32 * 0.07;
            let r = 4. + 90. * k as f32 / per_layer as f32;
            let p = [125. + r * a.cos(), 105. + r * a.sin(), z];
            s.segments.push(Segment { a: prev, b: p, extrude: true });
            prev = p;
        }
        s.layers.push(Layer { segments: start..s.segments.len() as u32, z });
    }
    s
}

// The scene as the live view builds it: centred, travel dropped from a toolpath, a mesh decimated to the CPU limit.
fn model(mut scene: Scene) -> (Scene, Prepared, Home) {
    scene.check().unwrap();
    scene.recentre();
    let scene = if scene.is_toolpath() {
        scene.keep_extrusions();
        scene
    } else {
        decimate(&scene, 0, CPU_LIVE_TRIANGLES)
    };
    let home = Home::new(&scene, 0).unwrap();
    let prepared = Prepared::new(&scene, DARK);
    (scene, prepared, home)
}

// The turntable's frame `i` on a `size` card at `scale`, palettes alternating as the cost bench's do, so every frame differs.
fn frames(scene: &Scene, home: Home, size: (f32, f32), scale: f32, count: usize) -> Vec<(View, Style<'static>)> {
    let mut viewer = Viewer::from_home(Surface::QuickLook, home, (0, 1), scene.layers.len(), size.0, size.1);
    viewer.opened(0., true);
    (0..count)
        .map(|i| {
            viewer.advance(i as f64 / 60.);
            let v = viewer.view;
            let palette = if i % 2 == 0 { DARK } else { Palette { ground: [0.09; 3], ..DARK } };
            let view = View { scale: v.scale * scale, cx: v.cx * scale, cy: v.cy * scale, ..v };
            (view, Style { palette, wire: None, layer: viewer.layer, plate: Some(viewer.rect), ground: Some(palette.ground) })
        })
        .collect()
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3))
}

// The FNV-1a hash of each frame's RGB bytes.
fn pictures(scene: &Scene, prepared: &Prepared, shots: &[(View, Style)], w: usize, h: usize, ss: usize) -> Vec<u64> {
    let (mut raster, mut rgb) = (Raster::default(), Vec::new());
    shots
        .iter()
        .map(|(view, style)| {
            raster.resize(w, h, ss);
            raster.render(scene, prepared, 0, view, style);
            raster.rgb_into(&mut rgb);
            fnv(&rgb)
        })
        .collect()
}

#[test]
fn every_cost_fixture_keeps_its_picture_bit_for_bit() {
    let fixtures = [("box 12", stl_box([20., 20., 0.], [80., 60., 40.])), ("sheet 20k", sheet(20_000)), ("spiral 20k", spiral(40, 500)), ("spiral 40k", spiral(200, 200))];
    let mut got = Vec::new();
    for (name, scene) in fixtures {
        let (scene, prepared, home) = model(scene);
        let shots = frames(&scene, home, (450., 272.), 2., 3);
        got.push((name, pictures(&scene, &prepared, &shots, 900, 544, 1), pictures(&scene, &prepared, &shots[..1], 900, 544, 2)));
    }
    // The pictures the tree before the PERF1 rounds drew: three moving frames and one still each.
    let pinned: [(&str, [u64; 3], [u64; 1]); 4] = [
        ("box 12", [0xd20e_8d0f_b8fc_d402, 0xe6d4_c0f7_28a8_2e96, 0x4026_91ab_d034_b099], [0xf120_ebe9_5a5f_0861]),
        ("sheet 20k", [0x0cdb_6245_9056_f23f, 0x8085_dbef_bc5e_a177, 0x084b_a6e5_c9ee_5536], [0xf946_b19e_b004_5c75]),
        ("spiral 20k", [0xda12_2dba_0051_3c64, 0xcd53_7fb6_6159_f203, 0xee15_8544_d50f_13e5], [0xd31c_9baf_cce9_64c1]),
        ("spiral 40k", [0xfc9c_a084_9679_c8a4, 0xbb3a_4d7d_04e5_c69c, 0xb03e_a117_c1f6_c935], [0xde75_1f65_6e2a_6c3a]),
    ];
    let changed: Vec<String> = got
        .iter()
        .zip(pinned)
        .filter(|((name, moving, still), (pname, pm, ps))| name != pname || moving[..] != pm[..] || still[..] != ps[..])
        .map(|(got, _)| format!("{got:#018x?}"))
        .collect();
    assert!(changed.is_empty(), "pictures changed: {changed:?}");
}

#[test]
fn the_fast_byte_conversions_are_the_division_and_the_rounding_exactly() {
    for i in 0..=255u32 {
        let v = i as f32 / std::hint::black_box(255.);
        assert_eq!(super::pixels::unpack(i << 24 | i << 16 | i << 8 | i), ([v; 3], v), "byte {i}");
    }
    let rounded = |v: f32| ((v.clamp(0., 1.) * 255.).round() as u32) & 0xff;
    let mut probes = vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0., -1e-30, -0.5, f32::from_bits(1f32.to_bits() + 1), 2., f32::MIN_POSITIVE, 1e-45];
    // Every byte centre and every half way point between two bytes, with the 64 floats each side.
    for n in 0..=510u32 {
        let bits = (n as f32 / 510.).to_bits();
        probes.extend((bits.saturating_sub(64)..=bits + 64).map(f32::from_bits));
    }
    probes.extend((0..=1f32.to_bits()).step_by(997).map(f32::from_bits));
    for v in probes {
        assert_eq!(super::pixels::byte(v), rounded(v), "{v:e}");
    }
}

#[test]
fn the_tone_index_is_the_tone_step_exactly() {
    let mut lights = vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0., 0., -0.3, 0.95, f32::MAX, f32::MIN];
    // The floats each side of every light that lands half way between two steps or on one.
    for k in 0..=20 {
        let bits = (k as f32 / 20. * 1.25 - 0.3).to_bits();
        lights.extend((bits.saturating_sub(64)..=bits.saturating_add(64)).map(f32::from_bits));
    }
    lights.extend((0..=u32::MAX).step_by(997).map(f32::from_bits));
    for light in lights {
        let step = crate::ui_logic_model_preview::color::tone_step(light);
        match crate::ui_logic_model_preview::color::tone_index(light) {
            Some(k) => assert_eq!((k as f32 / 10.).to_bits(), step.to_bits(), "{light:e}"),
            None => assert!(step.is_nan(), "{light:e}"),
        }
    }
}

#[test]
fn a_solid_line_sample_is_the_blend_it_skips_whatever_the_sample_held() {
    let mut colors: Vec<f32> = (0..=1020).map(|i| i as f32 / 1020.).collect();
    // The floats each side of every half way point between two bytes, where a rounding in the blend could move the byte.
    for k in 0..255 {
        let bits = ((k as f32 + 0.5) / 255.).to_bits();
        colors.extend((bits - 32..=bits + 32).map(f32::from_bits));
    }
    colors.extend([f32::NAN, f32::INFINITY, -1., 2., 1e-30]);
    let (mut raster, mut solid, mut blended) = (Raster::default(), 0, 0);
    for v in colors {
        raster.resize(1, 1, 1);
        raster.solids.clear();
        let p = raster.solid([v, 0.25, 1.]);
        for d in 0..=255u32 {
            raster.set(0, 255 << 24 | d << 16 | d << 8 | d);
            raster.blend(0, [v, 0.25, 1.], 1., None, None);
            if let Some(p) = p {
                assert_eq!(p, raster.get(0), "{v:e} over byte {d}");
            }
        }
        solid += usize::from(p.is_some());
        blended += usize::from(p.is_none());
    }
    assert!(solid > 1000 && blended > 0, "most colours write solid and a few must blend: {solid} solid, {blended} blended");
}

// Stage costs per moving frame, printed for RESULT.md; run with --ignored at opt-level 3.
#[test]
#[ignore]
fn moving_frame_stage_costs() {
    // Each fixture with the first moving frame and the still the tree before the PERF1 rounds drew at full size.
    let fixtures = [
        ("box 12", stl_box([20., 20., 0.], [80., 60., 40.]), 0xf549_49b8_ee4b_e2b9, 0x857b_5fe4_b177_4874),
        ("sheet 100k", sheet(100_000), 0xa297_248a_e50c_1b6c, 0xce9f_65f6_fbaa_81b8),
        ("sheet 2M", sheet(2_000_000), 0x2ac3_c891_7247_bb02, 0x89e3_b468_2ac1_591d),
        ("spiral 200k", spiral(200, 1_000), 0xefdc_47e3_09af_4296, 0x1c0f_f8d8_4b9b_42f2),
        ("spiral 4M", spiral(2_000, 2_000), 0xe24d_16e8_9f09_7439, 0x3b74_770f_07bd_6ab0),
    ];
    let (count, (w, h)) = (20, (1800, 1090));
    for (name, scene, first, rest) in fixtures {
        let (scene, prepared, home) = model(scene);
        let shots = frames(&scene, home, (900., 545.), 2., count);
        let (still, moving) = (pictures(&scene, &prepared, &shots[..1], w, h, 2), pictures(&scene, &prepared, &shots[..1], w, h, 1));
        eprintln!("PV4HASH {name}: moving {:#018x} still {:#018x}", moving[0], still[0]);
        assert_eq!((moving[0], still[0]), (first, rest), "{name}: the full-size pictures are the ones the tree before the rounds drew");
        let (mut raster, mut out, mut check) = (Raster::default(), Vec::new(), Vec::new());
        for rep in 0..3 {
            let mut ns = [0u64; 6];
            let mut stats = super::Stats::default();
            for (i, (view, style)) in shots.iter().enumerate() {
                raster.lend(std::mem::take(&mut out));
                raster.resize(w, h, 1);
                let t = cpu();
                let view = raster.sampled(view);
                raster.clear(style);
                let t1 = cpu();
                let basis = view.basis();
                let under = basis.d[2] < 0.;
                if !under {
                    raster.plate(&view, style.plate.unwrap(), &style.palette, super::GRID_ALPHA, 1.);
                }
                let t2 = cpu();
                let range = raster.project(&scene, &prepared, 0, &view, &basis);
                let t3 = cpu();
                raster.faces(&scene, &prepared, range, &view, style);
                let t4 = cpu();
                raster.toolpath(&scene, &view, &basis, style);
                let t5 = cpu();
                if under {
                    raster.plate(&view, style.plate.unwrap(), &style.palette, super::GRID_UNDER, super::EDGE_UNDER);
                }
                let t6 = cpu();
                if rep == 0 && i == 0 {
                    raster.rgb_into(&mut check);
                    assert_eq!(fnv(&check), moving[0], "{name}: the staged frame is the rendered frame");
                }
                raster.frame_into(&mut out);
                let t7 = cpu();
                for (k, d) in [t1 - t, t2 - t1 + t6 - t5, t3 - t2, t4 - t3, t5 - t4, t7 - t6].into_iter().enumerate() {
                    ns[k] += d;
                }
                stats = raster.stats;
            }
            let ms = |v: u64| v as f64 / 1e6 / count as f64;
            let total: u64 = ns.iter().sum();
            eprintln!(
                "PV4STAGE {name} rep {rep}: total {:.3} ms, clear {:.3}, plate {:.3}, project {:.3}, faces {:.3}, toolpath {:.3}, output {:.3}; {} faces, {} extrusions, {} blends, {} solid, {} samples",
                ms(total), ms(ns[0]), ms(ns[1]), ms(ns[2]), ms(ns[3]), ms(ns[4]), ms(ns[5]), stats.triangles, stats.segments, stats.blends, stats.solid, stats.samples
            );
        }
    }
}
