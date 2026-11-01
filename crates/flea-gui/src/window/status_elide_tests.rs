//! EY-425: a left text wider than its zone is cut with an ellipsis, as StatusBar.qml's ElideRight does.
use super::*;

fn drawn_counts(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> String {
    handle
        .update(cx, |shell, _, cx| {
            let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
            footer.counts_text.text()
        })
        .unwrap()
}

#[gpui::test]
fn ey425_a_narrow_footer_elides_the_selection_size(cx: &mut TestAppContext) {
    let handle = open(cx);
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(1100.), gpui::px(700.)));
    frame(cx, handle);
    handle
        .update(cx, |shell, _, cx| {
            shell.panes[0].as_mut().unwrap().selected = vec![1];
            cx.notify();
        })
        .unwrap();
    // The shell's frame hands the footer its new head; the footer lays it out on the next.
    frame(cx, handle);
    frame(cx, handle);
    let full = drawn_counts(cx, handle);
    assert_eq!(
        full, "1 of 1,000 selected · 0 B",
        "a wide bar draws the whole head"
    );
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(300.), gpui::px(240.)));
    frame(cx, handle);
    frame(cx, handle);
    let cut = drawn_counts(cx, handle);
    assert!(
        cut.ends_with('\u{2026}') && cut.len() < full.len(),
        "v0.3.12 elides the head inside its zone: {cut:?}"
    );
}
