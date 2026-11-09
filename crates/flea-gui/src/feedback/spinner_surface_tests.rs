use super::*;
use crate::{
    feedback::spinner::Mode,
    g2::file_ops::{host::Overlay, ipc::OmiAdapter, palette::CardPalette},
    ipc::Adapter,
};
use std::sync::Arc;

#[gpui::test]
fn two_loading_panes_share_one_callback_without_pane_renders(cx: &mut TestAppContext) {
    let (handle, _keep) = open(cx, true);
    let first = load_side(cx, handle, 0);
    let second = load_side(cx, handle, 1);
    let before = (first.counters.others.get(), second.counters.others.get());
    for _ in 0..64 {
        assert_eq!(frame(cx, handle), 1);
    }
    assert_eq!(
        (first.counters.others.get(), second.counters.others.get()),
        before
    );
    assert!(first.counters.requests.get() >= 64 && second.counters.requests.get() >= 64);
}

fn job(id: usize, kind: &str, state: &str) -> String {
    format!(
        r#"{{"t":"job","job":{{"id":{id},"kind":"{kind}","head":"Test job","file":"test","done":0,"total":1,"bytes":0,"totalBytes":100,"itemBytes":0,"rate":0,"left":null,"state":"{state}","reason":"","output":[],"undo":false,"cancelled":false}},"unfinished":1}}"#
    )
}

fn reveal_jobs(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    assert!(cx.debug_bounds("op1-job-1-mark").is_none());
    cx.executor()
        .advance_clock(Duration::from_millis(crate::pane::swap::HOLD_MS - 1));
    cx.run_until_parked();
    assert!(cx.debug_bounds("op1-job-1-mark").is_none());
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    assert!(cx.debug_bounds("op1-job-1-mark").is_some());
}

#[gpui::test]
fn job_ipc_reports_modes_from_backend_kinds_and_failed_jobs_have_no_omi(cx: &mut TestAppContext) {
    let (overlay, vcx) = cx.add_window_view(|window, _| {
        let palette =
            crate::list::Palette::inherited(&window.text_style(), crate::palette::Theme::default());
        Overlay::new(
            Arc::new(crate::test_catalogs::english()),
            CardPalette::from_list(&palette, 8.),
            "default",
        )
    });
    let kinds = [
        ("copy", Mode::Working),
        ("move", Mode::Working),
        ("send", Mode::Upload),
        ("fetch", Mode::Download),
        ("nearby", Mode::Thinking),
    ];
    overlay.update(vcx, |overlay, cx| {
        for (id, (kind, _)) in kinds.iter().enumerate() {
            overlay.line(&job(id + 1, kind, "running"), cx);
        }
        overlay.line(&job(6, "copy", "failed"), cx);
    });
    reveal_jobs(vcx);
    overlay.read_with(vcx, |overlay, _| {
        let reply = OmiAdapter {
            overlay: Some(overlay),
        }
        .call("omiState", &[])
        .unwrap();
        let data = flea::jsondoc::parse(reply.stdout.trim()).unwrap();
        let rows = data.as_array().unwrap();
        for (id, (_, mode)) in kinds.iter().enumerate() {
            let row = rows
                .iter()
                .find(|row| {
                    row.get("id").and_then(flea::jsondoc::Json::as_f64) == Some((id + 1) as f64)
                })
                .unwrap();
            assert_eq!(
                row.get("mode").and_then(flea::jsondoc::Json::as_str),
                Some(mode.id())
            );
            assert_eq!(
                row.get("label").and_then(flea::jsondoc::Json::as_str),
                Some(mode.label())
            );
            assert_eq!(
                row.get("shown").and_then(flea::jsondoc::Json::as_bool),
                Some(true)
            );
        }
        let failed = rows
            .iter()
            .find(|row| row.get("id").and_then(flea::jsondoc::Json::as_f64) == Some(6.))
            .unwrap();
        assert_eq!(
            failed.get("shown").and_then(flea::jsondoc::Json::as_bool),
            Some(false)
        );
    });
    for _ in 0..64 {
        vcx.executor().advance_clock(FRAME);
        assert_eq!(vcx.update(|window, cx| window.simulate_next_frame(cx)), 1);
    }
    overlay.update(vcx, |overlay, cx| overlay.fold_jobs(cx));
    vcx.run_until_parked();
    // Deliver the callback already queued before the fold; hidden elements queue no successor.
    vcx.update(|window, cx| window.simulate_next_frame(cx));
    for _ in 0..64 {
        vcx.executor().advance_clock(FRAME);
        assert_eq!(vcx.update(|window, cx| window.simulate_next_frame(cx)), 0);
    }
}

#[test]
fn accessible_names_use_upstream_mode_labels_and_stable_distinct_ids() {
    use crate::g2::file_ops::glyph::{Glyph, Paint};
    use gpui::Element;
    let first = super::super::spinner::Slot(Rc::new(Crawl::default()));
    let second = super::super::spinner::Slot(Rc::new(Crawl::default()));
    assert_ne!(first.id(), second.id());
    let mut node = gpui::accesskit::Node::new(gpui::Role::Image);
    first.write_a11y_info(&mut node);
    assert_eq!(node.label(), Some(Mode::Thinking.label()));
    for mode in [Mode::Thinking, Mode::Working, Mode::Upload, Mode::Download] {
        let glyph = Glyph {
            paint: Paint::Spiral {
                id: 1,
                mode,
                still: None,
            },
            side: 16.,
            ink: gpui::Hsla::default(),
        };
        glyph.write_a11y_info(&mut node);
        assert_eq!(node.label(), Some(mode.label()));
    }
}

#[gpui::test]
fn a_loading_hero_and_running_cards_share_one_window_callback(cx: &mut TestAppContext) {
    let (handle, _keep) = open(cx, false);
    let mark = load(cx, handle);
    let entity = shell(cx, handle, |shell, window, cx| {
        let palette =
            crate::list::Palette::inherited(&window.text_style(), crate::palette::Theme::default());
        let entity = cx.new(|_| {
            Overlay::new(
                shell.locale.catalog.clone(),
                CardPalette::from_list(&palette, 8.),
                "default",
            )
        });
        shell.overlay = Some(entity.clone());
        cx.notify();
        entity
    });
    entity.update(cx, |overlay, cx| {
        overlay.line(&job(1, "copy", "running"), cx);
    });
    let mut visual = gpui::VisualTestContext::from_window(handle.into(), cx);
    reveal_jobs(&mut visual);
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    shell(cx, handle, |shell, window, cx| {
        shell.ipc(
            crate::ipc::server::Event::Call {
                request: flea::ipc::wire::Request::Call {
                    target: "flea".into(),
                    function: "omiState".into(),
                    arguments: vec![],
                },
                answer: send,
            },
            window,
            cx,
        );
    });
    let reply = receive.try_recv().unwrap().reply;
    assert_eq!(reply.code, 0);
    let described = flea::jsondoc::parse(reply.stdout.trim()).unwrap();
    let job = &described.as_array().unwrap()[0];
    assert_eq!(
        job.get("mode").and_then(flea::jsondoc::Json::as_str),
        Some("working")
    );
    assert_eq!(
        job.get("label").and_then(flea::jsondoc::Json::as_str),
        Some(Mode::Working.label())
    );
    let before = mark.counters.others.get();
    for _ in 0..64 {
        assert_eq!(frame(cx, handle), 1);
    }
    assert_eq!(mark.counters.others.get(), before);
}

#[gpui::test]
fn a_hidden_window_samples_paints_and_requests_nothing(cx: &mut TestAppContext) {
    let (handle, _keep) = open(cx, false);
    let mark = load(cx, handle);
    cx.simulate_window_visibility_change(handle.into(), gpui::WindowVisibility::Hidden);
    frame(cx, handle);
    let counts = (
        mark.counters.frames.get(),
        mark.counters.writes.get(),
        mark.counters.requests.get(),
    );
    for _ in 0..64 {
        assert_eq!(frame(cx, handle), 0);
    }
    assert_eq!(
        counts,
        (
            mark.counters.frames.get(),
            mark.counters.writes.get(),
            mark.counters.requests.get()
        )
    );
}

#[gpui::test]
fn operation_spiral_uses_the_muted_frame_role_when_accent_inks_differ(cx: &mut TestAppContext) {
    let (overlay, vcx) = cx.add_window_view(|window, _| {
        let list =
            crate::list::Palette::inherited(&window.text_style(), crate::palette::Theme::default());
        let mut palette = CardPalette::from_list(&list, 8.);
        palette.accent_frame = palette.background;
        Overlay::new(
            Arc::new(crate::test_catalogs::english()),
            palette,
            "default",
        )
    });
    overlay.update(vcx, |overlay, cx| {
        overlay.line(&job(1, "copy", "running"), cx);
    });
    reveal_jobs(vcx);
    let bounds = vcx.debug_bounds("op1-job-1-mark").unwrap();
    // TransferCard.qml's spiral takes the muted frame, as the jobs board draws it.
    let (frame, accent) = overlay.read_with(vcx, |overlay, _| {
        (overlay.palette.muted_frame, overlay.palette.accent)
    });
    assert_ne!((frame.h, frame.s, frame.l), (accent.h, accent.s, accent.l));
    vcx.update(|window, _| {
        let square = bounds.scale(window.scale_factor());
        let quads = window.painted_quads();
        let marks: Vec<_> = quads
            .iter()
            .filter(|quad| quad.bounds.is_contained_within(&square))
            .collect();
        assert!(!marks.is_empty());
        assert!(marks
            .iter()
            .all(|quad| quad
                .background
                .as_solid()
                .is_some_and(|ink| (ink.h, ink.s, ink.l) == (frame.h, frame.s, frame.l))));
    });
}

#[gpui::test]
fn a_hidden_mark_can_retarget_without_samples_paints_or_frame_requests(cx: &mut TestAppContext) {
    let (handle, _keep) = open(cx, false);
    let mark = load(cx, handle);
    for _ in 0..10 {
        frame(cx, handle);
    }
    land(cx, handle);
    let before = (
        mark.counters.writes.get(),
        mark.counters.frames.get(),
        mark.counters.requests.get(),
    );
    for mode in [Mode::Working, Mode::Upload, Mode::Download] {
        mark.set(mode);
        handle
            .update(cx, |_, window, cx| {
                mark.prepare(true, None, window, cx);
                mark.paint(window);
            })
            .unwrap();
        assert_eq!(frame(cx, handle), 0);
        assert_eq!(
            before,
            (
                mark.counters.writes.get(),
                mark.counters.frames.get(),
                mark.counters.requests.get()
            )
        );
    }
    let resumed = load(cx, handle);
    assert!(Rc::ptr_eq(&mark, &resumed));
    assert_eq!(mark.mode(), Mode::Download);
    assert_eq!(mark.offset(), 0.);
    let prepared = (
        mark.counters.writes.get(),
        mark.counters.frames.get(),
        mark.counters.requests.get(),
    );
    let (quads, scale) = crate::feedback::paint_tests::capture_rest(cx, Rc::clone(&mark));
    assert_eq!(
        (
            mark.counters.writes.get(),
            mark.counters.frames.get(),
            mark.counters.requests.get()
        ),
        (prepared.0, prepared.1 + 1, prepared.2)
    );
    assert!(complete_rest_matches(quads.clone(), &mark, scale));
    let footprint = quads.iter().fold(mark.slot().scale(scale), |area, quad| {
        area.union(&quad.bounds)
    });
    let mut outcomes = Vec::new();
    for control in [
        "baseline",
        "missing",
        "wrong_bounds",
        "extra_inside",
        "extra_outside",
        "extra_crossing",
    ] {
        let mut all = quads.clone();
        match control {
            "missing" => {
                all.remove(0);
            }
            "wrong_bounds" => {
                all[0].bounds.origin.x.0 += 1.;
            }
            "extra_inside" => {
                all.push(all[0]);
            }
            "extra_outside" | "extra_crossing" => {
                // The widest quad, so a crossing extra still crosses when the first quad is one pixel of a dash edge.
                let mut extra = *all
                    .iter()
                    .max_by(|a, b| a.bounds.size.width.0.total_cmp(&b.bounds.size.width.0))
                    .unwrap();
                extra.bounds.origin.x.0 =
                    footprint.right().0 + if control == "extra_outside" { 10. } else { -1. };
                assert_ne!(extra.bounds.intersect(&footprint), extra.bounds);
                all.push(extra);
            }
            _ => {}
        }
        let accepted = complete_rest_matches(all, &mark, scale);
        eprintln!("REVIEW_FIXTURE control={control} accepted={accepted}");
        outcomes.push(accepted);
    }
    assert_eq!(outcomes, [true, false, false, false, false, false]);
}

fn complete_rest_matches(mut quads: Vec<gpui::Quad>, mark: &Crawl, scale: f32) -> bool {
    let slot = mark.slot();
    // A listing mark rests as v0.3.12's spiral (EY-354): exactly its dash quads in the mark's ink.
    if mark.is_spiral() {
        let mut matched = true;
        mark.spiral_rects(scale, |bounds, coverage| {
            let bounds = bounds.scale(scale);
            match quads.iter().position(|quad| {
                quad.bounds == bounds
                    && quad.background.as_solid() == Some(mark.ink().opacity(coverage))
            }) {
                Some(index) => {
                    quads.remove(index);
                }
                None => matched = false,
            }
        });
        return matched && quads.is_empty();
    }
    let side = crate::feedback::spinner::device_side(f32::from(slot.size.width) * scale);
    let expected = omi::Player::with_options(
        &omi::Kit::bundled(),
        omi::Options {
            mode: Some("download".into()),
            body_motion: side >= 44.,
            ..Default::default()
        },
    );
    let rects = expected.mode_rects("download", 0.).unwrap();
    let bounds: Vec<_> = rects
        .iter()
        .map(|rect| {
            crate::feedback::spinner::mapped(rect, expected.kit().view, slot, scale).scale(scale)
        })
        .collect();
    for (rect, bounds) in rects.iter().zip(bounds) {
        if bounds.is_empty() {
            continue;
        }
        let Some(index) = quads.iter().position(|quad| {
            quad.bounds == bounds
                && quad.background.as_solid() == Some(mark.ink().opacity(rect.opacity as f32))
        }) else {
            return false;
        };
        quads.remove(index);
    }
    quads.is_empty()
}
