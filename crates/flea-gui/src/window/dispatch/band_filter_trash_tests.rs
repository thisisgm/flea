//! S2-13: a band over a filtered List marks only the rows the filter shows, so Move to Trash never takes a hidden file.
use super::{land, open, row};
use crate::{backend::Backend, state::Shell, window::u07_tests::read};
use gpui::{
    point, px, Entity, Modifiers, MouseButton, MouseUpEvent, TestAppContext, VisualTestContext,
};

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn centre(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    index: usize,
) -> gpui::Point<gpui::Pixels> {
    // rowCentre answers "x y" in window pixels, for example "412.5 118".
    let text = read(shell, cx, "rowCentre", &[&index.to_string()]);
    let mut values = text.split_whitespace().map(|v| {
        v.parse::<f32>()
            .unwrap_or_else(|_| panic!("rowCentre {index} answered {text:?}"))
    });
    let (x, y) = (
        values.next().expect("drawn x"),
        values.next().expect("drawn y"),
    );
    point(px(x), px(y))
}

fn selected(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<usize> {
    shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().selected.clone()
    })
}

#[gpui::test]
fn a_band_over_a_filtered_list_marks_and_trashes_only_the_shown_rows(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    let rows = (0..8)
        .map(|i| {
            row(
                &format!("file-{i}-{}", if i % 2 == 0 { "keep" } else { "skip" }),
                false,
            )
        })
        .collect();
    land(&shell, cx, "/files", rows, 1);
    let (backend, mut read_line) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    let mut sent = move || std::iter::from_fn(&mut read_line).collect::<Vec<String>>();
    cx.simulate_keystrokes("/ k e e p");
    // The backend answers the last filter request, for example {"c":"filter","query":"keep","id":4}.
    let id = sent()
        .iter()
        .filter(|line| flea::json::field_str(line, "c").as_deref() == Some("filter"))
        .filter_map(|line| flea::json::field_usize(line, "id"))
        .next_back()
        .expect("typing in the filter asked the backend");
    let line = format!(
        r#"{{"t":"filtered","id":{id},"query":"keep","n":8,"rows":[0,2,4,6],"ms":0.01,"listing":1}}"#
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, Backend::test_reply_line(&line), window, cx)
        })
    });
    draw(cx);
    let shown = shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .filter
            .shown
            .as_deref()
            .map(<[usize]>::to_vec)
    });
    assert_eq!(
        shown,
        Some(vec![0, 2, 4, 6]),
        "the filter shows the keep rows"
    );
    // From the blank ground two rows under the last shown row, up to the first shown row.
    let last = centre(&shell, cx, 6);
    let first = centre(&shell, cx, 0);
    let row_height = (last.y - first.y) / 3.;
    let start = point(last.x, last.y + row_height * 2.);
    cx.simulate_mouse_move(start, None, Modifiers::none());
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    draw(cx);
    let (band, filtered) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.input.band.is_some(), host.filter.shown.is_some())
    });
    assert!(
        band && filtered,
        "the press below the rows starts a band over the filter: band {band}, filter {filtered}"
    );
    for step in 1..=4 {
        let y = start.y + (first.y - start.y) * (step as f32 / 4.);
        // A band needs width as well as height, so the pointer drifts left as it climbs.
        cx.simulate_mouse_move(
            point(start.x - px(10. * step as f32), y),
            Some(MouseButton::Left),
            Modifiers::none(),
        );
        draw(cx);
    }
    assert_eq!(
        selected(&shell, cx),
        [0, 2, 4, 6],
        "the band marked a hidden row"
    );
    cx.simulate_event(MouseUpEvent {
        position: point(start.x - px(40.), first.y),
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 1,
    });
    draw(cx);
    assert_eq!(
        selected(&shell, cx),
        [0, 2, 4, 6],
        "release kept a hidden mark"
    );
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .filter
            .shown
            .is_none()),
        "Escape closed the filter"
    );
    assert_eq!(
        selected(&shell, cx),
        [0, 2, 4, 6],
        "closing the filter kept a hidden mark"
    );
    sent();
    cx.simulate_keystrokes("delete");
    let trashed: Vec<String> = sent()
        .into_iter()
        .filter(|line| flea::json::field_str(line, "c").as_deref() == Some("trash"))
        .collect();
    assert_eq!(
        trashed,
        [r#"{"c":"trash","rows":[0,2,4,6],"menuId":0,"listing":1}"#],
        "Move to Trash took a row the filter hid"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
