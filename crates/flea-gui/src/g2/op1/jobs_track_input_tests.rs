//! Mounted regression cases sharing the parent fixture.
use super::*;

#[test]
fn wheel_under_an_open_settings_card_leaves_the_track_still() {
    for view in [View::List, View::Columns] {
        let (mut cx, handle, _received) = open(view);
        reset(&mut cx, handle);
        update(&mut cx, handle, |shell, window, cx| {
            shell.open_settings(window, cx)
        });
        frame(&mut cx, handle);
        assert!(update(&mut cx, handle, |shell, _, cx| {
            shell.card_holds_keyboard(cx)
        }));
        let track = track(&mut cx, handle, view);
        let at = point(track.left() + px(5.), track.top() + px(30.));
        wheel_handler(&mut cx, handle, view, at);
        assert_eq!(
            scroll(&mut cx, handle),
            0.,
            "{view:?}: a wheel under the open settings card moved the track"
        );
    }
}

#[test]
fn wheel_above_the_jobs_card_moves_the_track_and_keeps_its_glide() {
    for view in [View::List, View::Columns] {
        let (mut cx, handle, _received) = open(view);
        start(&mut cx, handle);
        reset(&mut cx, handle);
        let jobs = bounds(&mut cx, handle, "op1-jobs");
        let outside = point(jobs.left() + px(5.), jobs.top() - px(20.));
        assert!(
            track(&mut cx, handle, view).contains(&outside),
            "{view:?}: the point beside the jobs card lies on the track"
        );
        wheel(&mut cx, handle, outside);
        assert!(
            scroll(&mut cx, handle) > 0.,
            "{view:?}: a wheel beside the jobs card must scroll"
        );
        super::super::super::atomic_tests::advance(&mut cx, handle, 16);
        frame(&mut cx, handle);
        assert!(
            moving(&mut cx, handle, view),
            "{view:?}: the glide must survive a frame beside the jobs card"
        );
    }
}

#[test]
fn jobs_whitespace_blocks_columns_left_and_right_press_but_keeps_cancel() {
    for button in [MouseButton::Left, MouseButton::Right] {
        let (mut cx, handle, mut received) = open(View::Columns);
        start(&mut cx, handle);
        reset(&mut cx, handle);
        let jobs = bounds(&mut cx, handle, "op1-jobs");
        let rule = bounds(&mut cx, handle, "op1-jobs-rule-1");
        let job = bounds(&mut cx, handle, "op1-job-13");
        let points = [
            point(jobs.left() + px(5.), jobs.top() + px(5.)),
            point(jobs.left() + px(5.), rule.center().y),
            point(jobs.left() + px(5.), job.bottom() - px(10.)),
        ];
        for at in points {
            let row = column_row(&mut cx, handle, at);
            press(&mut cx, handle, at, button);
            update(&mut cx, handle, |shell, _, _| {
                let host = shell.panes[0].as_ref().unwrap();
                assert_eq!(
                    (host.pane.cursor, host.selected.as_slice()),
                    (0, &[0][..]),
                    "covered press changed Columns: button={button:?}, point={at:?}"
                );
            });
            let packet = received();
            assert!(
                packet.is_none(),
                "covered right press must not request a menu snapshot: button={button:?}, point={at:?}, packet={packet:?}"
            );
            fold(&mut cx, handle);
            press(&mut cx, handle, at, button);
            // Finder rule 5: a right click opens the row's menu and leaves the cursor where it was.
            let cursor = if button == MouseButton::Right { 0 } else { row };
            update(&mut cx, handle, |shell, _, _| {
                assert_eq!(
                    shell.panes[0].as_ref().unwrap().pane.cursor,
                    cursor,
                    "folded control must reach the real covered row"
                );
            });
            if button == MouseButton::Right {
                let packet = received();
                assert!(
                    packet
                        .as_deref()
                        .is_some_and(|line| line.contains("\"op\":\"snapshot\"")
                            && line.contains(&format!("\"cursor\":{row}}}"))),
                    "folded control: button={button:?}, point={at:?}, packet={packet:?}"
                );
            }
            reset(&mut cx, handle);
            while received().is_some() {}
            fold(&mut cx, handle);
        }
        let outside = point(jobs.left() + px(5.), jobs.top() - px(20.));
        let row = column_row(&mut cx, handle, outside);
        press(&mut cx, handle, outside, button);
        let cursor = if button == MouseButton::Right { 0 } else { row };
        update(&mut cx, handle, |shell, _, _| {
            assert_eq!(
                shell.panes[0].as_ref().unwrap().pane.cursor,
                cursor,
                "outside-card press must still reach Columns"
            );
        });
        if button == MouseButton::Right {
            let packet = received();
            assert!(
                packet
                    .as_deref()
                    .is_some_and(|line| line.contains("\"op\":\"snapshot\"")
                        && line.contains(&format!("\"cursor\":{row}}}"))),
                "outside control: button={button:?}, point={outside:?}, packet={packet:?}"
            );
        }
        reset(&mut cx, handle);
        while received().is_some() {}
        let cancel = point(job.right() - px(5.), job.bottom() - px(10.));
        press(&mut cx, handle, cancel, MouseButton::Left);
        assert_eq!(
            received().as_deref(),
            Some(r#"{"c":"jobs","op":"cancel","id":13}"#),
            "real Cancel: button=Left, point={cancel:?}"
        );
        assert!(received().is_none());
    }
}

#[test]
fn jobs_padding_blocks_grid_press_at_a_measured_tile_overlap() {
    let (mut cx, handle, _received) = open(View::Grid);
    start(&mut cx, handle);
    reset(&mut cx, handle);
    let jobs = bounds(&mut cx, handle, "op1-jobs");
    let (at, row) = update(&mut cx, handle, |shell, _, _| {
        let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
        let state = grid.state.borrow();
        let padding = [
            (
                jobs.left() + px(3.),
                jobs.top() + px(3.),
                jobs.left() + px(12.),
                jobs.bottom() - px(3.),
            ),
            (
                jobs.right() - px(12.),
                jobs.top() + px(3.),
                jobs.right() - px(3.),
                jobs.bottom() - px(3.),
            ),
            (
                jobs.left() + px(3.),
                jobs.top() + px(3.),
                jobs.right() - px(3.),
                jobs.top() + px(12.),
            ),
            (
                jobs.left() + px(3.),
                jobs.bottom() - px(12.),
                jobs.right() - px(3.),
                jobs.bottom() - px(3.),
            ),
        ];
        state
            .drawn()
            .iter()
            .filter_map(|tile| {
                let left = state.bounds.left() + px(tile.slot.x);
                let right = left + px(tile.slot.width);
                let top = state.bounds.top() + px(tile.slot.y);
                let bottom = top + px(tile.slot.height);
                padding.iter().find_map(|&(l, t, r, b)| {
                    let x = l.max(left + px(1.));
                    let y = t.max(top + px(1.));
                    (x < r.min(right) && y < b.min(bottom) && tile.listing != 0)
                        .then_some((point(x, y), tile.listing))
                })
            })
            .next()
            .expect("real Grid tile must overlap card padding")
    });
    press(&mut cx, handle, at, MouseButton::Left);
    update(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!((host.pane.cursor, host.selected.as_slice()), (0, &[0][..]));
    });
    fold(&mut cx, handle);
    press(&mut cx, handle, at, MouseButton::Left);
    update(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            (host.pane.cursor, host.selected.as_slice()),
            (row, &[row][..]),
            "folded control must select the covered real Grid tile"
        );
    });
}
