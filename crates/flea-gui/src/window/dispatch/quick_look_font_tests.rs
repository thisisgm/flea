//! Font specimens use the live text-size ratio and share Quick Look's card ground.
use super::preview_tests::{body, close, draw, file, opened, sent};
use super::test_shell::{land, set_cursor};
use crate::{
    g2::{
        pv1::{doc::View, Body, Look},
        pv2::markdown::style::hex,
    },
    palette::Theme,
    presentation::Fonts,
};
use flea::jsondoc::{self, Json};
use gpui::TestAppContext;

#[gpui::test]
fn quick_look_font_samples_follow_the_live_oem_base_and_pinned_text_size(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (shell, cx, mut requests) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![file("Inter.ttf", "font-x-generic")],
        1,
    );
    set_cursor(&shell, cx, 0);
    for (base, pinned, logical) in [
        (12, None, [22, 15, 12]),
        (14, None, [22, 15, 12]),
        (16, None, [22, 15, 12]),
        (12, Some(14), [26, 18, 14]),
        (14, Some(12), [19, 13, 10]),
        (16, Some(14), [19, 13, 11]),
        (0, Some(14), [22, 15, 12]),
    ] {
        cx.update(|window, cx| {
            let mut fonts = Fonts::from_sources(
                &format!("[font]\nbase-size={base}\nbody=18"),
                "",
                "monospace".into(),
            );
            fonts.base = base;
            cx.set_global(fonts);
            shell.update(cx, |shell, cx| {
                shell.launch.settings.data = pinned.map_or(Json::Null, |size| {
                    jsondoc::parse(&format!(
                        r#"{{"display":{{"textSize":{{"mode":{size}}}}}}}"#
                    ))
                    .unwrap()
                });
                shell.sync_font_settings(cx);
                assert!(shell.open_quick_look(0, window, cx));
            });
        });
        draw(cx);
        let asks = sent(&mut requests, "fontlook");
        assert_eq!(asks.len(), 1);
        let scale = cx.update(|window, _| f64::from(window.scale_factor()));
        let expected: Vec<Json> = logical
            .map(|px| Json::Num(format!("{:.0}", (f64::from(px) * scale).round())))
            .into();
        assert_eq!(
            asks[0].get("sizes").and_then(Json::as_array),
            Some(expected.as_slice()),
            "base {base}, pinned {pinned:?}: sample sizes round before device scaling"
        );
        cx.simulate_keystrokes("escape");
        draw(cx);
    }
    close(&shell, cx);
}

#[gpui::test]
fn quick_look_font_samples_have_no_background_plate_on_the_card(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (shell, cx, mut requests) = opened(cx);
    let theme = Theme::default();
    assert_ne!(
        theme.surface, theme.background,
        "distinct grounds expose a plate"
    );
    cx.update(|_, cx| cx.set_global(theme));
    land(
        &shell,
        cx,
        "/files",
        vec![
            file("Inter.ttf", "font-x-generic"),
            file("Inter.otf", "font-x-generic"),
        ],
        1,
    );
    for index in 0..2 {
        set_cursor(&shell, cx, index);
        cx.simulate_keystrokes("space");
        draw(cx);
        let asks = sent(&mut requests, "fontlook");
        assert_eq!(asks.len(), 1);
        let ground = cx.update(|window, _| Look::of_text(&window.text_style(), theme).surface);
        let ground = hex(ground);
        assert_eq!(
            asks[0].get("bg").and_then(Json::as_str),
            Some(ground.as_str()),
            "rasterized samples must carry the same ground as their card"
        );
        let Some(Body::Doc(doc)) = body(&shell, cx) else {
            panic!("font document mounted")
        };
        let View::Font(font) = doc.view else {
            panic!("font view mounted")
        };
        assert_eq!(cx.read(|cx| font.read(cx).theme.surface.clone()), ground);
        cx.simulate_keystrokes("escape");
        draw(cx);
    }
    close(&shell, cx);
}
