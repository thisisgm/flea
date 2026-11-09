//! Location completion, wheel ownership and settings receipts through the mounted chooser.
use super::pointer_tests::{grid, notch, path_click, seed, thumb, wheel};
use super::{
    process::Line,
    tests::fixture::{fixture, Fixture},
};
use crate::window::settings::store::{Update, Worker};
use flea::jsondoc::{self, Json};
use gpui::{point, px, Modifiers, ScrollDelta, TestAppContext, TouchPhase};

// Sample sent line: {"c":"jump","id":3,"ranking":0,"favourites":[],"recent":[]}
fn answer_jumps(f: &mut Fixture, from: usize) {
    let mut at = from;
    while let Some(line) = f.listing.sent().get(at).cloned() {
        at += 1;
        let Some(id) = line
            .strip_prefix(r#"{"c":"jump","id":"#)
            .and_then(|line| line.split(',').next())
        else {
            continue;
        };
        f.listing.say(Line::Reply(format!(
            r#"{{"t":"jumped","id":{id},"favourites":[],"zoxide":[],"recent":[],"frecency":{{}}}}"#
        )));
        f.cx.run_until_parked();
    }
}

#[gpui::test]
fn bfx_picker_clicked_location_completes_and_commits_without_renaming(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","folder":"/d/sub","name":"report.pdf"}"#,
        None,
    );
    seed(&mut f);
    let from = f.listing.sent().len();
    path_click(&mut f, true);
    answer_jumps(&mut f, from);
    let from = f.listing.sent().len();
    f.cx.simulate_input("/d/Do");
    f.cx.run_until_parked();
    answer_jumps(&mut f, from);
    f.cx.simulate_keystrokes("tab");
    f.cx.run_until_parked();
    assert_eq!(
        f.listing.sent().last().unwrap(),
        r#"{"c":"peek","path":"/d","first":128,"hidden":false,"hiddenLast":false}"#
    );
    f.listing.say(Line::Reply(r#"{"t":"peeked","path":"/d","first":128,"hidden":false,"mode":16877,"rows":[{"n":"Docs","d":true}]}"#.into()));
    f.cx.run_until_parked();
    let path = f.update(|picker, cx| picker.browse.path_view(cx));
    assert_eq!(
        f.cx.update(|_, cx| path.read(cx).entry.text.clone()),
        "/d/Docs/"
    );
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    assert!(f.listing.sent().iter().any(|line| line
        == r#"{"c":"peek","path":"/d/Docs","first":0,"hidden":false,"hiddenLast":false}"#));
    f.listing.say(Line::Reply(
        r#"{"t":"peeked","path":"/d/Docs","first":0,"mode":16877,"rows":[]}"#.into(),
    ));
    f.cx.run_until_parked();
    assert_eq!(f.read(|picker| picker.listing.location.clone()), "/d/Docs");
    assert_eq!(
        f.read(|picker| picker.listing.save_name.clone()),
        "report.pdf"
    );
    assert!(!f.read(|picker| picker.answered()));
}

#[gpui::test]
fn bfx_picker_pointer_controls_refuse_during_submission(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    grid(&mut f);
    path_click(&mut f, false);
    f.cx.simulate_keystrokes("escape");
    f.cx.run_until_parked();
    notch(&mut f, 3., true);
    assert_eq!(thumb(&mut f), "large");
    f.update(|picker, cx| picker.accept(cx));
    assert!(f.read(|picker| picker.accept_state().submitting()));
    f.cx.run_until_parked();
    let at = f.cx.debug_bounds("picker-path-slot").unwrap().center();
    f.cx.simulate_click(at, Modifiers::none());
    f.cx.run_until_parked();
    let path = f.update(|picker, cx| picker.browse.path_view(cx));
    assert!(!f.cx.update(|_, cx| path.read(cx).editing()));
    notch(&mut f, 3., true);
    assert_eq!(thumb(&mut f), "large");
}

fn loaded(send: &async_channel::Sender<Update>, document: &str, through: u64) {
    send.try_send(Update::Loaded {
        document: jsondoc::parse(document).unwrap(),
        through,
        saved: true,
    })
    .unwrap();
}

fn written_size(patch: &Json) -> Option<&str> {
    patch.get("preview")?.get("thumbSize")?.as_str()
}

#[gpui::test]
fn bfx_picker_zoom_writes_only_size_and_keeps_newer_pending_size(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","folder":"/d/sub","name":"report.pdf","hidden":false}"#,
        None,
    );
    grid(&mut f);
    let (send, updates) = async_channel::unbounded();
    let (writes, queue) = std::sync::mpsc::channel();
    f.update(|picker, cx| picker.watch_settings(Worker { writes, updates }, cx));
    notch(&mut f, 3., true);
    assert_eq!(thumb(&mut f), "large");
    let (first, patch, operation) = queue.try_recv().unwrap();
    assert_eq!(
        patch,
        jsondoc::parse(r#"{"preview":{"thumbSize":"large"}}"#).unwrap()
    );
    assert!(operation.is_none());
    notch(&mut f, 3., true);
    let (second, patch, _) = queue.try_recv().unwrap();
    assert_eq!(written_size(&patch), Some("xlarge"));
    f.update(|picker, cx| picker.persist_picker_view("grid", cx));
    let (third, patch, _) = queue.try_recv().unwrap();
    assert!(first < second && second < third);
    assert_eq!(patch.get("pickerView").and_then(Json::as_str), Some("grid"));
    loaded(&send, r#"{"preview":{"thumbSize":"large"}}"#, first);
    f.cx.run_until_parked();
    assert_eq!(
        thumb(&mut f),
        "xlarge",
        "an older size receipt cannot undo later wheel input"
    );
    loaded(&send, r#"{"preview":{"thumbSize":"xlarge"}}"#, second);
    f.cx.run_until_parked();
    notch(&mut f, 1.5, true);
    loaded(
        &send,
        r#"{"pickerView":"grid","preview":{"thumbSize":"xlarge"}}"#,
        third,
    );
    f.cx.run_until_parked();
    notch(&mut f, 1.5, true);
    assert_eq!(
        thumb(&mut f),
        "huge",
        "an unrelated settings receipt keeps partial travel"
    );
    assert_eq!(f.read(|picker| picker.req.hidden), Some(false));
    assert_eq!(
        f.read(|picker| picker.listing.save_name.clone()),
        "report.pdf"
    );
}

#[gpui::test]
fn bfx_picker_zoom_adopts_saved_size_and_reports_write_failure(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    grid(&mut f);
    let (send, updates) = async_channel::unbounded();
    let (writes, queue) = std::sync::mpsc::channel();
    f.update(|picker, cx| picker.watch_settings(Worker { writes, updates }, cx));
    loaded(
        &send,
        r#"{"preview":{"thumbSize":"huge","ctrlZoom":true}}"#,
        0,
    );
    f.cx.run_until_parked();
    assert_eq!(thumb(&mut f), "huge");
    notch(&mut f, 3., true);
    assert_eq!(thumb(&mut f), "largest");
    let (revision, _, _) = queue.try_recv().unwrap();
    notch(&mut f, 3., true);
    assert!(
        queue.try_recv().is_err(),
        "clamping does not write an unchanged size"
    );
    send.try_send(Update::Failed {
        through: revision,
        keep: true,
    })
    .unwrap();
    f.cx.run_until_parked();
    assert!(f.read(|picker| picker.message.as_ref().is_some_and(|message| message.error)));
}

#[gpui::test]
fn bfx_picker_zoom_uses_filtered_cursor_and_ignores_cancelled_pixels(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    grid(&mut f);
    // Every other row stays, so arena row 600 draws at view index 300, many rows below the top.
    let kept: std::rc::Rc<[usize]> = (0..1000).step_by(2).collect();
    let (arena, view_index) = (600, 300);
    let covered = |f: &mut Fixture, kept: &[usize]| {
        f.update(|picker, cx| {
            let view = picker.browse.listing.read(cx);
            let layout = view.grid_view().layout(crate::grid::Shown::Only(kept));
            layout.covers(view.scroll_y(), view_index)
        })
    };
    f.update(|picker, cx| {
        picker.browse.listing.update(cx, |view, cx| {
            view.listing_mut().move_cursor(arena);
            view.set_filtered(Some(kept.clone()), "a", None);
            cx.notify();
        });
        picker.move_cursor(arena, cx);
        // The cursor rides the last visible row, so a zoom that reads it at the wrong index scrolls it away.
        picker.browse.listing.update(cx, |view, _| {
            let layout = view.grid_view().layout(crate::grid::Shown::Only(&kept));
            let top = layout.row_top(view_index / layout.columns);
            view.set_scroll_y(layout.clamp(top + layout.cell_height - layout.height));
        });
    });
    f.cx.run_until_parked();
    assert!(
        covered(&mut f, &kept),
        "the filtered cursor is in view before the zoom"
    );
    let before = f.listing.sent().len();
    notch(&mut f, 9., true);
    assert_eq!(thumb(&mut f), "huge");
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        arena,
        "zoom does not change the owner's facts"
    );
    assert_eq!(
        f.update(|picker, cx| picker.browse.listing.read(cx).listing().cursor()),
        arena
    );
    assert!(
        covered(&mut f, &kept),
        "the filtered cursor's view index stays visible"
    );
    assert_eq!(
        f.listing.sent().len(),
        before,
        "a narrowing does not request unrelated arena rows"
    );
    wheel(
        &mut f,
        ScrollDelta::Pixels(point(px(0.), px(-288.))),
        true,
        TouchPhase::Cancelled,
    );
    assert_eq!(thumb(&mut f), "huge");
}

#[gpui::test]
fn bfx_picker_zoom_reports_a_closed_settings_writer(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    grid(&mut f);
    let (_send, updates) = async_channel::unbounded();
    let (writes, queue) = std::sync::mpsc::channel();
    drop(queue);
    f.update(|picker, cx| picker.watch_settings(Worker { writes, updates }, cx));
    notch(&mut f, 3., true);
    assert_eq!(thumb(&mut f), "large");
    assert!(f.read(|picker| picker.message.as_ref().is_some_and(|message| message.error)));
}

fn filter_menu(cx: &mut TestAppContext) -> Fixture<'_> {
    let mut f = fixture(
        cx,
        r#"{"folder":"/d/sub","filters":[{"label":"Text","globs":["*.txt"]}]}"#,
        None,
    );
    seed(&mut f);
    let trigger = f.cx.debug_bounds("picker-filter").unwrap().center();
    f.cx.simulate_click(trigger, Modifiers::none());
    f.cx.run_until_parked();
    assert!(
        f.read(|picker| picker.chrome.menu_view.is_some()),
        "a click on the filter trigger opens filters"
    );
    f
}

#[gpui::test]
fn bfx_picker_filter_trigger_press_closes_an_open_menu(cx: &mut TestAppContext) {
    let mut f = filter_menu(cx);
    // The card covers the trigger, so this press is its outside click and the trigger's own click never fires.
    let trigger = f.cx.debug_bounds("picker-filter").unwrap().center();
    f.cx.simulate_click(trigger, Modifiers::none());
    f.cx.run_until_parked();
    assert!(
        f.read(|picker| picker.chrome.menu_view.is_none() && picker.chrome.menu.is_none()),
        "a second press on the trigger closes filters without reopening them"
    );
}

#[gpui::test]
fn bfx_picker_path_press_leaves_an_open_filter_menu_to_its_card(cx: &mut TestAppContext) {
    let mut f = filter_menu(cx);
    // A parent run of the strip, which navigates when nothing sits above it.
    let path = f.update(|picker, cx| picker.browse.path_view(cx));
    let strip =
        f.cx.update(|_, cx| path.read(cx).paint.borrow().strips[0].unwrap());
    let from = f.listing.sent().len();
    f.cx.simulate_click(
        point(strip.left() + px(5.), strip.center().y),
        Modifiers::none(),
    );
    f.cx.run_until_parked();
    assert!(
        f.read(|picker| picker.chrome.menu_view.is_none() && picker.chrome.menu.is_none()),
        "a press on the location bar outside the card closes the filter menu"
    );
    let after = f.listing.sent()[from..].to_vec();
    assert!(
        after.is_empty(),
        "the press that closes the menu navigates nowhere: {after:?}"
    );
    assert_eq!(f.read(|picker| picker.listing.location.clone()), "/d/sub");
    assert!(
        !f.cx.update(|_, cx| path.read(cx).editing()),
        "the press that closes the menu opens nothing"
    );
    path_click(&mut f, false);
}

#[gpui::test]
fn bfx_picker_grid_refill_is_not_held_by_a_pending_folder(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    grid(&mut f);
    f.update(|picker, _| picker.chrome.folder_request = Some("/d/sub".into()));
    let from = f.listing.sent().len();
    let small = jsondoc::parse(r#"{"preview":{"thumbSize":"small"}}"#).unwrap();
    f.update(|picker, cx| picker.apply_settings(&small, cx));
    assert_eq!(thumb(&mut f), "small");
    assert!(
        f.listing.sent()[from..]
            .iter()
            .any(|line| line.contains(r#""c":"window""#)),
        "a smaller saved size asks for its newly visible rows while a folder request is out"
    );
}

#[gpui::test]
fn bfx_picker_failed_size_write_keeps_the_chosen_size(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    grid(&mut f);
    let (send, updates) = async_channel::unbounded();
    let (writes, queue) = std::sync::mpsc::channel();
    f.update(|picker, cx| picker.watch_settings(Worker { writes, updates }, cx));
    notch(&mut f, 3., true);
    assert_eq!(thumb(&mut f), "large");
    let (revision, _, _) = queue.try_recv().unwrap();
    send.try_send(Update::Failed {
        through: revision,
        keep: true,
    })
    .unwrap();
    f.cx.run_until_parked();
    loaded(&send, r#"{"preview":{"thumbSize":"medium"}}"#, revision);
    f.cx.run_until_parked();
    assert_eq!(
        thumb(&mut f),
        "large",
        "a receipt after a failed write keeps the size the wheel chose"
    );
    assert!(f.read(|picker| picker.message.as_ref().is_some_and(|message| message.error)));
}
