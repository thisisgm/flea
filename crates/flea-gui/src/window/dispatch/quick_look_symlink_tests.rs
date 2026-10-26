//! The production Quick Look reply path keeps a link's bar above a large target image.
use super::super::preview_tests::{answer, body, close, file, opened, press, sent, token};
use super::super::test_shell::{land, set_cursor};
use crate::g2::pv1::Body;
use gpui::{px, TestAppContext};

fn image(cx: &mut TestAppContext, linked: bool) {
    let (shell, cx, mut requests) = opened(cx);
    let mut row = file("link.png", "image-x-generic");
    if linked {
        row.mode = 0o120777;
        row.link = Some("photo.png".into());
    }
    land(&shell, cx, "/files", vec![row], 1);
    set_cursor(&shell, cx, 0);
    press(cx, "space");
    let asks = sent(&mut requests, "meta");
    assert_eq!(asks.len(), 1);
    let target = if linked { "photo.png" } else { "" };
    answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"meta","row":0,"token":{},"w":800,"h":600,"orient":1,"target":"{target}"}}"#,
            token(&asks[0])
        ),
    );
    let card = cx.debug_bounds("pv1-card").expect("the image card draws");
    let picture = cx
        .debug_bounds("pv1-picture")
        .expect("the target image draws");
    if linked {
        assert!(
            matches!(body(&shell, cx), Some(Body::Link { target, .. }) if target == "→ photo.png")
        );
        let bar = cx.debug_bounds("pv1-bar").expect("the link bar draws");
        assert!(cx.debug_bounds("pv1-bar-mark:flea-mark:symlink").is_some());
        assert_eq!(bar.origin.y, card.origin.y);
        assert_eq!(bar.size.height, px(27.));
        assert_eq!(picture.origin.y, bar.origin.y + bar.size.height);
        assert_eq!(picture.size.height, card.size.height - bar.size.height);
        assert_eq!(picture.size.width, px(729.));
        assert!(shell.read_with(cx, |shell, cx| {
            shell
                .quick_look
                .as_ref()
                .unwrap()
                .view
                .read(cx)
                .subject()
                .unwrap()
                .name
                == "link.png"
        }));
    } else {
        assert!(matches!(body(&shell, cx), Some(Body::Image { .. })));
        assert!(cx.debug_bounds("pv1-bar").is_none());
        assert_eq!(picture.origin.y, card.origin.y);
        assert_eq!(picture.size.height, card.size.height);
        assert_eq!(picture.size.width, px(765.));
    }
    close(&shell, cx);
}

#[gpui::test]
fn a_large_link_target_starts_below_the_link_bar(cx: &mut TestAppContext) {
    image(cx, true);
}

#[gpui::test]
fn a_plain_copy_named_link_png_draws_no_bar(cx: &mut TestAppContext) {
    image(cx, false);
}
