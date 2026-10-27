//! Quick Look's card sits on the theme surface; its chips, fences and failed figures on the window ground.
use super::preview_tests::{draw, file, opened};
use super::test_shell::{land, set_cursor};
use crate::{
    g2::pv1::{doc::View, Body},
    palette::Theme,
    state::Shell,
};
use gpui::{Entity, TestAppContext, VisualTestContext};

/// The card open on a Markdown row under the bench theme: surface `#0c1512`, window ground `#111c18`.
fn quick_look_markdown(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext, Theme) {
    // The Markdown parse thread is real, so its wake comes from outside the deterministic scheduler.
    cx.executor().allow_parking();
    let (shell, cx, _lines) = opened(cx);
    let theme = Theme::from_text("background='#111c18'\ndark_background='#0c1512'");
    assert_ne!(
        theme.background, theme.surface,
        "distinct grounds expose a swap"
    );
    cx.update(|_, cx| cx.set_global(theme));
    land(
        &shell,
        cx,
        "/files",
        vec![file("notes.md", "text-markdown")],
        1,
    );
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    draw(cx);
    (shell, cx, theme)
}

#[gpui::test]
fn quick_look_markdown_card_ground_is_the_surface(cx: &mut TestAppContext) {
    let (shell, cx, theme) = quick_look_markdown(cx);
    let style = cx.update(|window, _| window.text_style());
    let card = shell.read_with(cx, |shell, cx| {
        let host = shell.quick_look.as_ref().expect("the card is open");
        host.view.read(cx).look(&style, theme)
    });
    assert_eq!(
        card.surface, theme.surface,
        "the card body takes the surface ground"
    );
    assert_eq!(
        card.background, theme.background,
        "the bare ground stays the window's"
    );
}

#[gpui::test]
fn quick_look_markdown_code_ground_is_the_window(cx: &mut TestAppContext) {
    let (shell, cx, theme) = quick_look_markdown(cx);
    let (code, background, surface) = shell.read_with(cx, |shell, cx| {
        let host = shell.quick_look.as_ref().expect("the card is open");
        let Some(Body::Doc(doc)) = host.view.read(cx).subject().map(|s| s.body.clone()) else {
            panic!("the Markdown document is mounted")
        };
        let View::Markdown(view) = doc.view else {
            panic!("the Markdown view is mounted")
        };
        let view = view.read(cx);
        (
            view.palette.code,
            view.palette.background,
            view.palette.surface,
        )
    });
    assert_eq!(
        code, theme.background,
        "chips, fences and failed figures take the window ground"
    );
    assert_eq!(
        background, theme.background,
        "a figure's ground stays the window's"
    );
    assert_eq!(
        surface, theme.surface,
        "a figure's surface stays the theme surface"
    );
}
