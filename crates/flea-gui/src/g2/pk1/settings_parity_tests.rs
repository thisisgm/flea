use super::{
    process::Line,
    tests::fixture::{fixture, Fixture},
};
use flea::jsondoc::{self, Json};
use gpui::TestAppContext;
use std::time::Duration;

fn apply(f: &mut Fixture, text: &str) {
    let data = flea::uistate::from_file(text);
    f.update(|picker, cx| picker.apply_settings(&data, cx));
    f.cx.run_until_parked();
}

fn last_list(f: &Fixture) -> Json {
    f.listing
        .sent()
        .iter()
        .rev()
        .find_map(|line| {
            let data = jsondoc::parse(line).unwrap();
            (data.get("c").and_then(Json::as_str) == Some("list")).then_some(data)
        })
        .unwrap()
}

#[gpui::test]
fn picker_parity_hidden_request_overrides_saved_default(cx: &mut TestAppContext) {
    for (request, expected) in [
        (r#"{"folder":"/d"}"#, true),
        (r#"{"folder":"/d","hidden":false}"#, false),
        (r#"{"folder":"/d","hidden":true}"#, true),
    ] {
        let mut f = fixture(cx, request, None);
        apply(&mut f, r#"{"hidden":true}"#);
        assert_eq!(
            last_list(&f).get("hidden").and_then(Json::as_bool),
            Some(expected)
        );
        f.update(|picker, cx| picker.open_folder("/next", cx));
        assert_eq!(
            last_list(&f).get("hidden").and_then(Json::as_bool),
            Some(expected)
        );
    }
}

#[gpui::test]
fn picker_parity_restores_view_without_overwriting_pending_user_choice(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    apply(&mut f, r#"{"pickerView":"grid"}"#);
    let listing = f.read(|picker| picker.browse.listing.clone());
    f.cx.update(|_, cx| assert_eq!(listing.read(cx).view_mode(), "grid"));
    let rows = (0..64)
        .map(|index| {
            format!(
                r#"{{"n":"file-{index}.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-plain"}}"#
            )
        })
        .collect::<Vec<_>>();
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/d","n":64,"v":1,"w":true}"#.into(),
    ));
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"rows","start":0,"listing":1,"rows":[{}]}}"#,
        rows.join(",")
    )));
    f.cx.run_until_parked();
    f.cx.update(|_, cx| listing.update(cx, |view, _| view.set_scroll_y(134.)));
    apply(&mut f, r#"{"pickerView":"grid","langMarks":false}"#);
    f.cx.update(|_, cx| {
        assert_eq!(
            listing.read(cx).scroll_y(),
            134.,
            "an unrelated settings update keeps the scrolled viewport"
        )
    });
    let (writes, queue) = std::sync::mpsc::channel();
    let (_send, updates) = async_channel::unbounded();
    f.update(|picker, cx| {
        picker.watch_settings(
            crate::window::settings::store::Worker { writes, updates },
            cx,
        )
    });
    f.update(|picker, cx| picker.user_set_view("list", cx));
    let (_, patch, _) = queue.try_recv().unwrap();
    assert_eq!(patch.get("pickerView").and_then(Json::as_str), Some("list"));
    apply(&mut f, r#"{"pickerView":"grid","langMarks":false}"#);
    f.cx.update(|_, cx| assert_eq!(listing.read(cx).view_mode(), "list"));
}

#[gpui::test]
fn picker_parity_applies_folder_and_global_sort_on_each_open(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    apply(
        &mut f,
        r#"{"sort":{"key":"name","reverse":true},"folderSorts":{"/d":{"key":"date","reverse":false}},"foldersFirst":false,"groupByKind":true,"hiddenLast":true}"#,
    );
    let line = last_list(&f);
    assert_eq!(line.get("by").and_then(Json::as_str), Some("mtime"));
    assert_eq!(line.get("desc").and_then(Json::as_bool), Some(false));
    assert_eq!(
        line.get("foldersFirst").and_then(Json::as_bool),
        Some(false)
    );
    assert_eq!(line.get("groupByKind").and_then(Json::as_bool), Some(true));
    assert_eq!(line.get("hiddenLast").and_then(Json::as_bool), Some(true));
    f.update(|picker, cx| picker.open_folder("/next", cx));
    let line = last_list(&f);
    assert_eq!(line.get("by").and_then(Json::as_str), Some("name"));
    assert_eq!(line.get("desc").and_then(Json::as_bool), Some(true));
    apply(
        &mut f,
        r#"{"rememberSort":false,"sort":{"key":"size","reverse":true},"folderSorts":{"/next":{"key":"date","reverse":false}}}"#,
    );
    assert_eq!(last_list(&f).get("by").and_then(Json::as_str), Some("size"));
}

fn listed(f: &mut Fixture) {
    f.listing
        .say(Line::Reply(r#"{"t":"listed","n":1,"path":"/d"}"#.into()));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"a.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-x-generic"}]}"#.into()));
    f.cx.run_until_parked();
}

fn submitted(f: &mut Fixture) -> u64 {
    listed(f);
    f.update(|picker, cx| picker.accept(cx));
    assert!(f.read(|picker| picker.accept_state().submitting()));
    let check = jsondoc::parse(f.checks.sent().last().unwrap()).unwrap();
    check.get("id").and_then(Json::as_f64).unwrap() as u64
}

#[gpui::test]
fn picker_parity_settings_change_relists_a_browsing_chooser(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    listed(&mut f);
    let sent = f.listing.sent().len();
    apply(&mut f, r#"{"sort":{"key":"size","reverse":true}}"#);
    assert_eq!(
        f.listing.sent().len(),
        sent + 1,
        "a sort change re-lists at once"
    );
    assert_eq!(last_list(&f).get("by").and_then(Json::as_str), Some("size"));
}

#[gpui::test]
fn picker_parity_settings_change_keeps_a_submission_listing(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    let id = submitted(&mut f);
    let sent = f.listing.sent().len();
    apply(&mut f, r#"{"sort":{"key":"size","reverse":true}}"#);
    assert_eq!(
        f.listing.sent().len(),
        sent,
        "a settings change never re-lists under a submission"
    );
    assert!(f.read(|picker| picker.accept_state().submitting()));
    f.checks.say(Line::Reply(format!(
        r#"{{"t":"picker","id":{id},"op":"mark","ok":false,"error":"gone"}}"#
    )));
    f.settle(|f| f.listing.sent().len() > sent);
    assert!(!f.read(|picker| picker.accept_state().submitting() || picker.answered()));
    assert_eq!(
        f.listing.sent().len(),
        sent + 1,
        "a refused submission runs the held re-list once"
    );
    assert_eq!(last_list(&f).get("by").and_then(Json::as_str), Some("size"));
}

#[gpui::test]
fn picker_parity_settings_change_after_an_answer_never_relists(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    let id = submitted(&mut f);
    let sent = f.listing.sent().len();
    apply(&mut f, r#"{"sort":{"key":"size","reverse":true}}"#);
    f.checks.say(Line::Reply(format!(
        r#"{{"t":"picker","id":{id},"op":"mark","ok":true,"marks":[{{"path":"/d/a.txt","bytes":3}}],"removed":0}}"#
    )));
    f.settle(|f| f.written().is_some());
    assert!(f.read(|picker| picker.answered() && !picker.accept_state().submitting()));
    assert_eq!(
        f.listing.sent().len(),
        sent,
        "a re-list held by the submission never runs once it answers"
    );
    apply(&mut f, r#"{"sort":{"key":"name","reverse":false}}"#);
    assert_eq!(
        f.listing.sent().len(),
        sent,
        "an answered chooser keeps its listing"
    );
}

pub(super) fn draw(f: &mut Fixture) {
    f.cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    f.cx.run_until_parked();
}

#[gpui::test]
fn picker_parity_grid_uses_shared_viewport_thumbnail_demand(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.update(|picker, cx| picker.user_set_view("grid", cx));
    let rows: Vec<String> = (0..128)
        .map(|at| {
            format!(
                r#"{{"n":"photo-{at}.png","d":false,"s":3,"m":0,"p":33188,"i":"image-x-generic"}}"#
            )
        })
        .collect();
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/d","n":128,"v":1,"w":true}"#.into(),
    ));
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"rows","start":0,"listing":1,"rows":[{}]}}"#,
        rows.join(",")
    )));
    f.cx.run_until_parked();
    draw(&mut f);
    draw(&mut f);
    f.cx.executor().advance_clock(Duration::from_millis(70));
    f.cx.run_until_parked();
    f.listing.say(Line::Reply(
        r#"{"t":"fsinfo","path":"/d","class":""}"#.into(),
    ));
    f.cx.run_until_parked();
    draw(&mut f);
    f.cx.executor().advance_clock(Duration::from_millis(70));
    f.cx.run_until_parked();
    let asks: Vec<Json> = f
        .listing
        .sent()
        .iter()
        .filter_map(|line| {
            let data = jsondoc::parse(line).unwrap();
            (data.get("c").and_then(Json::as_str) == Some("thumb")).then_some(data)
        })
        .collect();
    assert!(
        !asks.is_empty(),
        "the mounted picker Grid asks for thumbnails"
    );
    let asked = asks[0].get("rows").and_then(Json::as_array).unwrap();
    assert!(
        !asked.is_empty() && asked.len() < 128,
        "only the viewport asks"
    );
    let picture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/images/transfer.png"
    );
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"thumbed","row":0,"file":"{picture}"}}"#
    )));
    f.cx.run_until_parked();
    draw(&mut f);
    let listing = f.read(|picker| picker.browse.listing.clone());
    f.cx.update(|_, cx| {
        let view = listing.read(cx);
        let state = view.grid_state();
        assert!(
            state
                .borrow()
                .drawn_row(0)
                .and_then(|tile| tile.decoded)
                .is_some(),
            "the shared background decoder supplies the painted tile"
        );
    });
    f.update(|picker, cx| picker.user_set_view("list", cx));
    draw(&mut f);
    assert!(
        f.listing
            .sent()
            .iter()
            .any(|line| line.contains("\"c\":\"thumbcancel\"")),
        "leaving Grid cancels pending work"
    );
    for command in ["thumb", "thumbcancel", "fsinfo"] {
        assert!(super::process::LISTING.contains(&command));
    }
}

#[gpui::test]
fn picker_parity_watched_change_refreshes_only_the_current_folder(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/d","n":1,"v":1,"w":true}"#.into(),
    ));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"a.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-plain"}]}"#.into()));
    f.cx.run_until_parked();
    assert_eq!(f.read(|picker| picker.listing.total), 1);
    let before = f.listing.sent().len();
    f.listing
        .say(Line::Reply(r#"{"t":"changed","path":"/other"}"#.into()));
    f.cx.run_until_parked();
    assert_eq!(
        f.listing.sent().len(),
        before,
        "an old folder's watcher cannot refresh this folder"
    );
    f.listing
        .say(Line::Reply(r#"{"t":"changed","path":"/d"}"#.into()));
    f.cx.run_until_parked();
    assert_eq!(
        f.listing.sent().len(),
        before + 1,
        "the existing backend watcher re-lists the current picker folder"
    );
    assert_eq!(f.read(|picker| picker.listing.state), "loading");
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/d","n":2,"v":2,"w":true}"#.into(),
    ));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":2,"rows":[{"n":"a.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-plain"},{"n":"watched.txt","d":false,"s":5,"m":0,"p":33188,"i":"text-plain"}]}"#.into()));
    f.cx.run_until_parked();
    assert_eq!(f.read(|picker| picker.listing.total), 2);
}
