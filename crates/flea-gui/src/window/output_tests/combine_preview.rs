//! Combine borrows the listing's decoded images through the production Shell join.
use super::*;
use crate::g2::op4::model::Action;
use flea::ui_logic_op4::{output::Kind, selection::Source};
use std::time::Duration;

#[gpui::test]
fn vf_look_combine_receives_held_images_and_late_thumbnail_answers(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let handle = open(cx, backend);
    shell(cx, handle, |shell, _, _| {
        hold(
            shell,
            vec![
                row("photo.png", "image-x-generic"),
                row("sunset.jpg", "image-x-generic"),
            ],
        );
    });
    frame(cx, handle);
    shell(cx, handle, |shell, window, cx| {
        shell.receive(
            0,
            Backend::test_reply_line(r#"{"t":"fsinfo","path":"/files","class":""}"#),
            window,
            cx,
        );
    });
    frame(cx, handle);
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    frame(cx, handle);
    assert!(
        sent(&mut requests, "thumb").is_some(),
        "the real viewport requested thumbnails"
    );
    let picture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/images/transfer.png"
    );
    let answer = |row| format!(r#"{{"t":"thumbed","row":{row},"file":"{picture}","ms":3}}"#);
    shell(cx, handle, |shell, window, cx| {
        shell.receive(0, Backend::test_reply_line(&answer(0)), window, cx);
        let files = ["photo.png", "sunset.jpg"]
            .map(|name| Source {
                path: format!("/files/{name}"),
                kind: Kind::Image,
            })
            .to_vec();
        assert!(shell.open_output(0, Action::Combine, 1, files, window, cx));
    });
    cx.run_until_parked();
    frame(cx, handle);
    shell(cx, handle, |shell, window, cx| {
        shell.receive(0, Backend::test_reply_line(&answer(1)), window, cx);
    });
    cx.run_until_parked();
    frame(cx, handle);
    shell(cx, handle, |shell, _, cx| {
        let view = shell.output.view.as_ref().unwrap().read(cx);
        assert_eq!(
            view.thumbnails.len(),
            2,
            "the card must receive both held pictures"
        );
        assert_eq!(view.thumbnails[0].path, "/files/photo.png");
        assert_eq!(view.thumbnails[1].path, "/files/sunset.jpg");
        assert!(
            view.image_at(100., 139., 518.).is_some(),
            "the mounted preview has image geometry"
        );
    });
}
