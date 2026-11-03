use super::*;
use crate::{
    list::Metrics,
    palette::Theme,
    presentation::{Fonts, Snapshot},
};

#[gpui::test]
fn gcomp_atomic_font_theme_reaches_both_headers_footer_and_modal(cx: &mut TestAppContext) {
    let handle = open(cx);
    edit(cx, handle, |shell, window, cx| {
        shell.open_overlay_copy_to(false, "/files", window, cx)
    });
    for (base, small, caption, colors) in [
        (12, 11, 10, "background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'\ndark_background='#150d0a'"),
        (16, 15, 14, "background='#eff1f5'\nforeground='#4c4f69'\naccent='#1e66f5'\ndark_background='#e6e9ef'"),
    ] {
        let (theme, parsed) = Theme::from_text_observed(colors);
        edit(cx, handle, |shell, window, cx| {
            let family = shell.active_typeface().font.family.to_string();
            let fonts = Fonts::from_sources(&format!("[font]\nbase-size={base}\nbody=20\nbody-small={small}\ncaption={caption}"), "", family);
            shell.apply_presentation(Snapshot { theme, parsed, fonts }, window, cx);
        });
        frame(cx, handle);
        edit(cx, handle, |shell, window, cx| {
            for host in shell.panes.iter().flatten() {
                let list = host.list.borrow();
                assert_eq!(f32::from(list.typeface.body), 20.);
                let metrics = Metrics::for_typeface(&list.typeface);
                assert_eq!(list.drawn.metrics.row, metrics.row);
                assert_eq!(f32::from(list.header.drawn.bounds.size.height), metrics.header);
                assert_eq!(list.drawn.palette.unwrap().accent_frame, theme.accent_frame);
            }
            let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
            let disk = footer.painted.get("disk").expect("the mounted footer painted");
            let counts = footer.painted.get("counts").unwrap();
            assert_eq!(disk.font_size, caption as f32);
            assert_eq!(counts.font_size, caption as f32);
            assert_eq!(counts.color, theme.foreground);
            assert_eq!(f32::from(counts.bounds.size.height), shell.geometry(window).chrome.height);
            let card = shell.overlay.as_ref().unwrap().read(cx);
            assert_eq!(card.palette.surface, theme.surface);
            assert_eq!(card.palette.accent_frame, theme.accent_frame);
            assert!(!shell.focus.is_focused(window));
        });
    }
    edit(cx, handle, |shell, window, cx| {
        shell.close_overlay(window, cx)
    });
    frame(cx, handle);
    assert_eq!(call(cx, handle, "bodyPx"), Reply::value("20"));
    assert_eq!(
        json(cx, handle, "dualState")
            .get("active")
            .and_then(Json::as_bool),
        Some(true)
    );
}
