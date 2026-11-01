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

// EY-425: the ellipsis follows the longest head that fits, the space after the dot included.
#[gpui::test]
fn fix_views_2_ey425_the_ellipsis_follows_the_longest_head_that_fits(cx: &mut TestAppContext) {
    use gpui::AppContext;
    let handle = open(cx);
    handle
        .update(cx, |shell, _, cx| {
            shell.panes[0].as_mut().unwrap().selected = vec![1];
            cx.notify();
        })
        .unwrap();
    let mut cuts = 0;
    for width in (280..520).step_by(2) {
        cx.simulate_window_resize(
            handle.into(),
            gpui::size(gpui::px(width as f32), gpui::px(240.)),
        );
        frame(cx, handle);
        frame(cx, handle);
        let typeface = cx
            .update_window(handle.into(), |_, window, cx| {
                crate::list::Typeface::for_window(window, cx)
            })
            .unwrap();
        let (full, drawn, drawn_width, longer, zone) = handle
            .update(cx, |shell, window, cx| {
                let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
                let full = footer.counts.to_string();
                let drawn = footer.counts_text.text();
                let head = drawn.strip_suffix('\u{2026}').unwrap_or(&drawn).to_owned();
                let style = gpui::TextStyle {
                    font_family: typeface.font.family.clone(),
                    ..window.text_style()
                };
                let measure = |text: &str| {
                    f32::from(
                        window
                            .text_system()
                            .layout_line(text, typeface.caption, &[style.to_run(text.len())], None)
                            .width,
                    )
                };
                let longer = full[head.len()..]
                    .chars()
                    .next()
                    .map(|next| measure(&format!("{head}{next}\u{2026}")));
                (
                    full,
                    drawn.clone(),
                    measure(&drawn),
                    longer,
                    footer.zone_width,
                )
            })
            .unwrap();
        if drawn == full {
            continue;
        }
        cuts += 1;
        let head = drawn
            .strip_suffix('\u{2026}')
            .expect("a cut head ends with the ellipsis");
        assert!(full.starts_with(head), "{drawn:?} is a head of {full:?}");
        assert!(
            drawn_width <= zone,
            "at {width} px {drawn:?} overflows its zone"
        );
        assert!(
            longer.is_some_and(|width| width > zone),
            "at {width} px one more character still fits beside the ellipsis: {drawn:?}"
        );
    }
    assert!(cuts > 0, "some widths cut the head");
}
