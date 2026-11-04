use super::bench::{fixture, frame, held, open, real_cx};
use super::pane::ROW_HEIGHT;
use crate::palette::Theme;
use crate::visual::Probe;
use flea::jsondoc::Json;
use gpui::AppContext;
use std::rc::Rc;

// The row indices the probe reported, in push order, from its "row" roles.
fn probed_rows(probe: &Probe) -> Vec<usize> {
    let receipt = probe.json(&Json::Null);
    let Some(Json::Arr(roles)) = receipt.get("roles") else {
        panic!("the probe reports no roles");
    };
    roles
        .iter()
        .filter(|role| role.get("role").and_then(Json::as_str) == Some("row"))
        .filter_map(|role| {
            let id = role.get("id").and_then(Json::as_str)?;
            id.rsplit_once("-row-")?.1.parse().ok()
        })
        .collect()
}

#[test]
fn probe_rows_of_a_scrolled_frame_are_that_frames_visible_rows() {
    let mut cx = real_cx();
    let rows = fixture("2k").into_iter().take(40).collect();
    let window = open(&mut cx, held(rows), 900., 4. * ROW_HEIGHT);
    frame(&mut cx, window, 0.);
    let list = window
        .update(&mut cx, |view, _, _| Rc::clone(&view.list))
        .expect("the bench window is open");
    list.borrow_mut().probe = Some(Probe::default());
    frame(&mut cx, window, 10. * ROW_HEIGHT);
    let state = list.borrow();
    let shown: Vec<usize> = state
        .visible()
        .iter()
        .filter(|shown| shown.prepared.is_some())
        .map(|shown| shown.index)
        .collect();
    assert_eq!(shown, vec![10, 11, 12, 13]);
    let probe = state.probe.as_ref().expect("the probe is installed");
    assert_eq!(
        probed_rows(probe),
        shown,
        "the probe drew the rows of the previous frame, not this frame's visible rows"
    );
}

#[test]
fn a_changed_theme_reads_the_palette_again_without_restyle() {
    let mut cx = real_cx();
    let rows = fixture("2k").into_iter().take(3).collect();
    let window = open(&mut cx, held(rows), 900., 3. * ROW_HEIGHT);
    let list = window
        .update(&mut cx, |view, _, _| Rc::clone(&view.list))
        .expect("the bench window is open");
    let original = Theme::default();
    let mut swapped = original;
    std::mem::swap(&mut swapped.background, &mut swapped.foreground);
    let first = cx
        .update_window(window.into(), |_, window, _| {
            list.borrow_mut().palette(window, original)
        })
        .expect("the bench window is open");
    let second = cx
        .update_window(window.into(), |_, window, _| {
            list.borrow_mut().palette(window, swapped)
        })
        .expect("the bench window is open");
    assert_eq!(first.background, original.background);
    assert_eq!(
        second.background, swapped.background,
        "a changed theme kept the palette read for the first theme"
    );
    assert_eq!(list.borrow().palette_reads, 2);
}

impl super::ListState {
    /// The names the last frame drew, row by row.
    pub fn drawn_names(&self) -> Vec<String> {
        self.visible
            .iter()
            .filter_map(|shown| shown.prepared.as_ref().map(|row| row.label.clone()))
            .collect()
    }
}
