//! Preparation limits apply before Space and are lifted for the normal open.
use super::*;

const PNG: [u8; 73] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 4, 0, 0, 0, 4, 8, 2, 0,
    0, 0, 38, 147, 9, 41, 0, 0, 0, 16, 73, 68, 65, 84, 120, 156, 99, 56, 81, 161, 1, 71, 12, 196,
    113, 0, 83, 82, 22, 129, 25, 170, 14, 207, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

#[gpui::test]
fn pb_rest_prepare_refuses_a_file_that_grew_past_64_kib_after_listing(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (shell, cx) = open(&fixture, cx, "network");
    std::fs::write(fixture.0.join("readme.md"), vec![b'x'; 65_537]).unwrap();
    line(
        &shell,
        cx,
        &format!(
            r#"{{"t":"fsinfo","path":"{}","fs":"ext4","free":1000,"class":"local"}}"#,
            fixture.0.display()
        ),
    );
    cx.executor().advance_clock(Duration::from_millis(120));
    super::super::menus_tests::settle(cx);
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let refused = shell.read_with(cx, |shell, cx| {
            let warm = &shell.panes[0].as_ref().unwrap().look_warm;
            warm.prepared.as_ref().is_some_and(|doc| {
                let View::Markdown(view) = &doc.view else {
                    return false;
                };
                let view = view.read(cx);
                view.doc.status == crate::g2::pv2::markdown::Status::TooLarge
                    && view.counts.parsed_bytes == 0
                    && view.doc.text.is_none()
            })
        });
        if refused {
            break;
        }
        assert!(
            Instant::now() < until,
            "the preparation did not enforce its byte limit"
        );
        super::super::preview_tests::draw(cx);
        std::thread::sleep(Duration::from_millis(5));
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rest_prepare_caps_pictures_and_space_loads_the_remaining_pictures(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let mut large = PNG.to_vec();
    large.resize(262_145, 0);
    std::fs::write(fixture.0.join("image0.png"), large).unwrap();
    for index in 1..9 {
        std::fs::write(fixture.0.join(format!("image{index}.png")), PNG).unwrap();
    }
    let markdown = (0..9)
        .map(|index| format!("![picture](image{index}.png)\n\n"))
        .collect::<String>();
    std::fs::write(fixture.0.join("readme.md"), markdown).unwrap();
    let (shell, cx) = open(&fixture, cx, "local");
    ready(&shell, cx);
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let capped = shell.read_with(cx, |shell, cx| {
            let doc = shell.panes[0]
                .as_ref()
                .unwrap()
                .look_warm
                .prepared
                .as_ref()
                .unwrap();
            let View::Markdown(view) = &doc.view else {
                return false;
            };
            let view = view.read(cx);
            view.doc.pictures.len() == 8 && view.doc.assets.pictures.len() == 7
        });
        if capped {
            break;
        }
        assert!(
            Instant::now() < until,
            "the preparation did not cap pictures at eight and 256 KiB"
        );
        super::super::preview_tests::draw(cx);
        std::thread::sleep(Duration::from_millis(5));
    }
    cx.simulate_keystrokes("space");
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let loaded = shell.read_with(cx, |shell, cx| {
            let host = shell.quick_look.as_ref().unwrap();
            let Body::Doc(doc) = &host.view.read(cx).subject().unwrap().body else {
                return false;
            };
            let View::Markdown(view) = &doc.view else {
                return false;
            };
            view.read(cx).doc.assets.pictures.len() == 9
        });
        if loaded {
            break;
        }
        assert!(
            Instant::now() < until,
            "Space left preparation limits on the normal document"
        );
        super::super::preview_tests::draw(cx);
        std::thread::sleep(Duration::from_millis(5));
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
