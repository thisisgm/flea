//! EY-485: the Save collision review says why Replace stands, above Cancel and Replace, as PickerSave.qml's status line does.
use super::ey313_tests::draw;
use super::ipc::PickerGeometry;
use super::process::Line;
use super::tests::fixture::{fixture, Fixture};
use super::Picker;
use flea::ipc::wire::Request;
use gpui::TestAppContext;

// The catalog sentence with PickerSave.qml's zero-width wrap joint after the dot.
const SENTENCE: &str = "notes.\u{200b}txt already exists here.";
const REFUSAL: &str = "A folder has this name; choose another name.";

struct Drawn;

impl PickerGeometry for Drawn {
    fn controls(&self) -> String {
        "[]".into()
    }
    fn row_centre(&self, _: usize) -> String {
        String::new()
    }
    fn cohesion_controls(&self) -> String {
        "[]".into()
    }
    fn save_field_centre(&self) -> String {
        String::new()
    }
    fn path_bar_open(&self) -> bool {
        false
    }
    fn snapshot_members(&self) -> String {
        String::new()
    }
}

fn snapshot_field(f: &mut Fixture, key: &str) -> String {
    let request = Request::Call {
        target: "fleapicker".into(),
        function: "snapshot".into(),
        arguments: Vec::new(),
    };
    let reply = f.read(|picker| picker.ipc(request, Some(&Drawn)).reply);
    let snapshot = flea::jsondoc::parse(reply.stdout.trim_end()).expect("one JSON document");
    snapshot
        .get(key)
        .and_then(flea::jsondoc::Json::as_str)
        .unwrap_or_else(|| panic!("{key}"))
        .to_owned()
}

// Save File on /d naming notes.txt, which the probe answers as taken.
fn taken(cx: &mut TestAppContext) -> Fixture<'_> {
    let mut f = fixture(
        cx,
        r#"{"title":"Save File","mode":"save","folder":"/d"}"#,
        None,
    );
    f.update(|picker, cx| {
        picker.listing.location = "/d".into();
        picker.listing.save_name = "notes.txt".into();
        picker.invalidate_save(cx);
    });
    f.checks.say(Line::Reply(
        r#"{"t":"picker","id":1,"op":"save","ok":true,"review":1,"path":"/d/notes.txt","collision":true}"#
            .into(),
    ));
    f.settle(|f| f.read(|picker| picker.accept_state().save_ready()));
    f
}

#[gpui::test]
fn fixlogic_ey485_the_collision_review_draws_its_sentence_above_cancel_and_replace(
    cx: &mut TestAppContext,
) {
    let mut f = taken(cx);
    draw(&mut f);
    assert!(
        f.cx.debug_bounds("picker-save-line").is_none(),
        "no line before Return asks"
    );
    assert_eq!(snapshot_field(&mut f, "saveError"), "");
    f.update(|picker, cx| picker.accept(cx));
    draw(&mut f);
    assert!(f.read(|picker| picker.accept_state().collision_review()));
    assert_eq!(f.read(Picker::answer_label).as_ref(), "Replace");
    let line =
        f.cx.debug_bounds("picker-save-line")
            .expect("the review draws its line");
    let band =
        f.cx.debug_bounds("picker-answers-band")
            .expect("the answers band");
    assert!(
        line.bottom() <= band.top() && line.size.height > gpui::px(0.),
        "the line sits above Cancel and Replace: {line:?} over {band:?}"
    );
    assert_eq!(
        f.read(Picker::save_status).as_deref(),
        Some(SENTENCE),
        "the drawn line is the catalog's sentence"
    );
    assert_eq!(
        snapshot_field(&mut f, "saveError"),
        "",
        "saveError stays the backend's refusal alone, as PickerWindow.qml reports it"
    );
    f.update(|picker, _| picker.save_error = REFUSAL.into());
    assert_eq!(
        f.read(Picker::save_status).as_deref(),
        Some(REFUSAL),
        "a refusal outranks the review's sentence"
    );
}

#[gpui::test]
fn fixlogic_ey485_a_new_name_drops_the_sentence(cx: &mut TestAppContext) {
    let mut f = taken(cx);
    f.update(|picker, cx| picker.accept(cx));
    f.update(|picker, cx| {
        picker.listing.save_name = "other.txt".into();
        picker.invalidate_save(cx);
    });
    draw(&mut f);
    assert!(f.cx.debug_bounds("picker-save-line").is_none());
    assert_eq!(f.read(Picker::save_status), None);
}

#[gpui::test]
fn fixlogic_ey485_a_refused_name_draws_the_refusal_on_the_line(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"title":"Save File","mode":"save","folder":"/d"}"#,
        None,
    );
    f.update(|picker, cx| {
        picker.listing.location = "/d".into();
        picker.listing.save_name = "Docs".into();
        picker.invalidate_save(cx);
    });
    f.checks.say(Line::Reply(format!(
        r#"{{"t":"picker","id":1,"op":"save","ok":false,"error":"{REFUSAL}"}}"#
    )));
    f.settle(|f| f.read(|picker| !picker.save_error.is_empty()));
    draw(&mut f);
    assert!(f.cx.debug_bounds("picker-save-line").is_some());
    assert_eq!(f.read(Picker::save_status).as_deref(), Some(REFUSAL));
    assert_eq!(snapshot_field(&mut f, "saveError"), REFUSAL);
}
