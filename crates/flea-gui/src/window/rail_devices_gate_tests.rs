//! Readiness, settings and lazy lifecycle controls for the same mounted fake service.
use super::*;

#[gpui::test]
fn rail_devices_draw_with_lsblk_and_phones_join_when_gio_answers(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    frame(cx);
    assert_eq!(labels(&shell, cx), ["Home", "Downloads", "Trash"]);
    // SW-07: the drives never wait on gio mount -li, which is a hundred times slower than lsblk.
    service.answer("lsblk", 0, BLOCKS);
    frame(cx);
    assert_eq!(
        labels(&shell, cx),
        [
            "Home",
            "Downloads",
            "Trash",
            "testhost",
            "Archive",
            "Backup",
            "Photos"
        ]
    );
    service.answer("gio", 0, PHONE);
    frame(cx);
    assert_eq!(labels(&shell, cx).last().unwrap(), "Pixel 8");
}

#[gpui::test]
fn rail_devices_draw_in_the_first_frame_from_the_launch_seed(cx: &mut TestAppContext) {
    let seed = crate::g2::pl1::source::seed::Seed {
        listing: BLOCKS.into(),
        hostname: "testhost\n".into(),
    };
    let (shell, _service, cx) = open_seeded("{}", Some(seed), cx);
    frame(cx);
    assert_eq!(
        labels(&shell, cx),
        [
            "Home",
            "Downloads",
            "Trash",
            "testhost",
            "Archive",
            "Backup",
            "Photos"
        ],
        "SW-07: DEVICES is in the first frame, before the rail's own lsblk answers"
    );
}

#[gpui::test]
fn rail_devices_hidden_rail_ignores_the_launch_seed(cx: &mut TestAppContext) {
    let seed = crate::g2::pl1::source::seed::Seed {
        listing: BLOCKS.into(),
        hostname: "testhost\n".into(),
    };
    let (shell, _service, cx) = open_seeded(r#"{"places":{"rail":"hidden"}}"#, Some(seed), cx);
    frame(cx);
    assert!(
        source(&shell, cx).read_with(cx, |source, _| source.devices.entries.is_empty()),
        "a rail hidden at launch lists fresh when it is revealed"
    );
}

#[gpui::test]
fn rail_heading_line_box_preserves_the_board_group_pitch(cx: &mut TestAppContext) {
    let (shell, service, cx) = open(SETTINGS, cx);
    ready(&service, cx);
    frame(cx);
    let producer = producer(&shell, cx);
    cx.update(|window, cx| {
        let producer = producer.read(cx);
        let laid = producer.laid.borrow();
        let mut cache = crate::g2::pl4::rows::text::Cache::default();
        assert!(
            laid.headers.len() >= 2,
            "PLACES, FAVORITES and DEVICES draw"
        );
        for header in &laid.headers {
            let run = cache.get(
                &header.text,
                gpui::px(header.font_size),
                laid.width - 28.,
                1.,
                &producer.typeface,
                window,
            );
            let line = header.rect.height - header.top_padding - laid.row_gap;
            assert_eq!(
                header.text_height, line,
                "paint uses the measured text band"
            );
            assert_eq!(
                line,
                run.ascent.ceil() + (run.height - run.ascent).ceil(),
                "{} heading line box",
                header.text
            );
        }
    });
}

#[gpui::test]
fn rail_devices_unmounted_setting_keeps_optical_and_removable_rows(cx: &mut TestAppContext) {
    let (shell, service, cx) = open(r#"{"places":{"showUnmounted":false}}"#, cx);
    ready(&service, cx);
    assert_eq!(
        labels(&shell, cx),
        [
            "Home",
            "Downloads",
            "Trash",
            "testhost",
            "Archive",
            "Photos",
            "Pixel 8"
        ]
    );
}

#[gpui::test]
fn rail_devices_hidden_host_starts_no_jobs_and_withdrawal_parks_polling(cx: &mut TestAppContext) {
    let (shell, service, cx) = open(r#"{"places":{"rail":"hidden"}}"#, cx);
    frame(cx);
    assert!(service.jobs.borrow().is_empty());
    let state = flea::jsondoc::parse(r#"{"places":{"rail":"shown"}}"#).unwrap();
    shell.update(cx, |shell, cx| shell.rail_state(&state, cx));
    frame(cx);
    assert_eq!(service.jobs.borrow().len(), 2);
    ready(&service, cx);
    let hidden = flea::jsondoc::parse(r#"{"places":{"rail":"hidden"}}"#).unwrap();
    shell.update(cx, |shell, cx| shell.rail_state(&hidden, cx));
    frame(cx);
    cx.executor().advance_clock(Duration::from_millis(20_000));
    frame(cx);
    assert_eq!(
        service.jobs.borrow().len(),
        2,
        "hidden rail does not enumerate"
    );
}
