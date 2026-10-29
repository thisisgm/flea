use super::*;

// F2 covers a scrollable mounted listing, independently of the completed eight-row F1 proof.
fn f2_check(view: View, side: usize, from: &str, to: &str, elapsed: bool) {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx, view, from);
    handle
        .update(&mut cx, |root, _, cx| {
            root.shell.update(cx, |shell, cx| {
                for host in shell.panes.iter_mut().flatten() {
                    host.pane.held = Rc::new(Held {
                        listing: host.pane.held.listing,
                        total: 100,
                        start: 0,
                        rows: bench::fixture("2k").into_iter().take(100).collect(),
                    });
                    host.selected = vec![95];
                    host.pane.cursor = 96;
                    host.input.selection.changed(&host.selected);
                }
                cx.notify();
            });
        })
        .unwrap();
    draw(&mut cx, handle);
    handle
        .update(&mut cx, |root, _, cx| {
            root.shell.update(cx, |shell, cx| {
                let host = shell.panes[side].as_mut().unwrap();
                let frame = host.input.drawn.expect("long listing painted").frame;
                let header = host.list.borrow().file_metrics().header;
                assert_eq!(frame.total, 100);
                assert!(frame.max_scroll() > 1000.);
                host.pane.scroll_to(frame.max_scroll() - header / 4.);
                cx.notify();
            });
        })
        .unwrap();
    draw(&mut cx, handle);
    handle.update(&mut cx, |root, window, cx| {
        let capture = root.shell.update(cx, |shell, cx| {
            assert!(!shell.input_blocked(cx));
            let host = shell.panes[side].as_ref().unwrap();
            let old = host.input.drawn.expect("near-bottom viewport painted").frame;
            assert!(old.scroll > 500.);
            let held = Rc::clone(&host.pane.held);
            let requests = host.pane.list_requests;
            let face = host.list.borrow().typeface.clone();
            let blank = point(old.bounds.left() + px(60.), old.bounds.bottom() - px(2.));
            assert!(old.bounds.contains(&blank) && !old.in_lane(blank));
            assert!(host.input.hit(blank).is_none(), "F2_SETUP_REAL_BLANK: trailing header allowance must expose actual blank space");
            shell.pointer_down(side, &MouseDownEvent {
                position: blank, button: MouseButton::Left, ..Default::default()
            }, window, cx);
            shell.pointer_move(side, &MouseMoveEvent {
                position: point(old.bounds.left() + px(100.), old.bounds.top() + px(40.)),
                pressed_button: Some(MouseButton::Left), ..Default::default()
            }, true, window, cx);
            let edge = if elapsed { old.bounds.top() - px(20.) } else { old.bounds.bottom() + px(20.) };
            shell.pointer_move(side, &MouseMoveEvent {
                position: point(old.bounds.left() + px(100.), edge),
                pressed_button: Some(MouseButton::Left), ..Default::default()
            }, true, window, cx);
            let now = cx.background_executor().now();
            let host = shell.panes[side].as_mut().unwrap();
            let band = host.input.band.as_ref().expect("actual pointer route created band");
            assert!(band.active && band.direction != 0.);
            assert_eq!(band.before, vec![95]);
            assert_eq!(band.cursor, 96);
            assert_ne!(host.selected, vec![95]);
            host.input.stepped_at = elapsed.then(|| now.checked_sub(std::time::Duration::from_millis(50)).unwrap());
            let patch = jsondoc::parse(&format!(r#"{{"density":"{to}"}}"#)).unwrap();
            let document = flea::uistate::patched(&shell.launch.settings.data, &patch).unwrap();
            shell.receive_settings(Update::Loaded { document, through: 0, saved: false }, window, cx);
            assert!(!shell.input_blocked(cx));
            let host = shell.panes[side].as_ref().unwrap();
            assert!(!host.pane.busy());
            assert!(Rc::ptr_eq(&held, &host.pane.held));
            assert_eq!(host.pane.list_requests, requests);
            assert!(host.list.borrow().typeface == face);
            assert!(host.input.band.as_ref().is_some_and(|band| band.active));
            let current = host.list.borrow().file_metrics();
            assert_eq!(host.pane.row_height(), current.row);
            assert_eq!(current.row != old.row, from != to);
            // Observe the actual receiver's scaled/clamped offset, not a copy of its formula.
            let receiver_scroll = host.pane.scroll_y;
            assert!(receiver_scroll > 500.);
            if !elapsed {
                assert!(current.row > old.row && receiver_scroll > old.max_scroll(), "F2_SETUP_CROSSES_STALE_LIMIT");
            }
            println!("G76_F2_READY view={view:?} side={side} from={from} to={to} old_max={} receiver={receiver_scroll} elapsed={elapsed}", old.max_scroll());
            let from = from.to_owned();
            let to = to.to_owned();
            Box::new(move |shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>| {
                let geometry = shell.geometry(window);
                let heights = std::array::from_fn(|pane| geometry.content_height_for(pane, shell.shown_view(), shell.slots.strip_height_for(pane)));
                let lists = shell.prepare_lists(heights, Some(geometry.chrome.height), window, cx);
                let prepared = lists.prepared[side].expect("actual long-list prepare");
                let host = shell.panes[side].as_ref().unwrap();
                assert_eq!(prepared.bounds, old.bounds);
                assert_eq!(prepared.listing, old.listing);
                assert_eq!(prepared.total, 100);
                if from != to {
                    assert!(host.input.band.is_none(), "F1 cancellation remains required");
                    assert_eq!(host.selected, vec![95]);
                    assert_eq!(host.pane.cursor, 96);
                    assert_eq!(lists.states[side].as_ref().unwrap().selected.as_deref(), Some(&[95][..]));
                    assert_eq!(lists.states[side].as_ref().unwrap().cursor, Some(96));
                    assert_eq!(host.pane.scroll_y, receiver_scroll, "G76_F2_VIEWPORT: stale autoscroll displaced the actual receiver offset");
                } else {
                    assert!(host.input.band.as_ref().is_some_and(|band| band.active));
                    assert!(host.pane.scroll_y < receiver_scroll, "G76_F2_CONTROL_AUTOSCROLL: unchanged geometry must still move upward");
                }
                assert_eq!(prepared.scroll, host.pane.scroll_y);
                println!("G76_F2_PASS view={view:?} side={side} from={from} to={to} elapsed={elapsed} receiver={receiver_scroll} prepared={}", prepared.scroll);
                prepared
            }) as RenderCapture
        });
        assert!(root.capture.is_none() && root.prepared.is_none());
        root.capture = Some(capture);
        cx.notify();
    }).unwrap();
    draw(&mut cx, handle);
    handle
        .update(&mut cx, |root, _, cx| {
            assert!(
                root.capture.is_none(),
                "genuine ac631 render consumed capture"
            );
            let prepared = root.prepared.take().expect("genuine render produced frame");
            root.shell.read_with(cx, |shell, _| {
                let host = shell.panes[side].as_ref().unwrap();
                let mounted = host
                    .input
                    .drawn
                    .expect("fresh actual mounted viewport")
                    .frame;
                if from != to {
                    assert_eq!(
                        mounted.scroll, prepared.scroll,
                        "actual painted viewport offset"
                    );
                } else {
                    assert!(
                        mounted.scroll <= prepared.scroll,
                        "a still-active upward edge may advance again in the mounted render"
                    );
                }
                assert_eq!(
                    host.list.borrow().drawn.scroll_y,
                    mounted.scroll,
                    "actual list content viewport offset"
                );
                assert_eq!(mounted.row, prepared.row);
            });
        })
        .unwrap();
}

#[test]
fn g76_f2_density_change_preserves_scaled_near_bottom_viewport() {
    for (view, side) in [(View::List, 0), (View::Dual, 0), (View::Dual, 1)] {
        f2_check(view, side, "compact", "normal", false);
    }
    for from in DENSITIES {
        for to in DENSITIES {
            if from == to {
                continue;
            }
            for (view, side) in [(View::List, 0), (View::Dual, 0), (View::Dual, 1)] {
                f2_check(view, side, from, to, true);
            }
        }
    }
}

#[test]
fn g76_f2_same_geometry_still_autoscrolls_long_listing() {
    for density in DENSITIES {
        for (view, side) in [(View::List, 0), (View::Dual, 0), (View::Dual, 1)] {
            f2_check(view, side, density, density, true);
        }
    }
}
