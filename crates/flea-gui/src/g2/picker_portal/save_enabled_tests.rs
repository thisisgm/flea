//! A save chooser probes the supplied name once its folder lands, so Save draws enabled without a press first.
use super::ipc::PickerGeometry as _;
use super::process::Line;
use super::tests::fixture::fixture;
use gpui::TestAppContext;

fn landed<'a>(name: &str, cx: &'a mut TestAppContext) -> super::tests::fixture::Fixture<'a> {
    let f = fixture(
        cx,
        &format!(r#"{{"mode":"save","folder":"/d","name":"{name}"}}"#),
        None,
    );
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/d","n":1,"v":1,"w":true}"#.into(),
    ));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"notes.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-plain"}]}"#.into()));
    f.cx.run_until_parked();
    f
}

#[gpui::test]
fn picker_gm_supplied_save_name_is_probed_and_save_enabled(cx: &mut TestAppContext) {
    let mut f = landed("sa-d3-new.txt", cx);
    assert_eq!(
        f.checks.sent(),
        [r#"{"op":"save","folder":"/d","name":"sa-d3-new.txt","c":"picker","id":1}"#],
        "the supplied name is probed once the folder lands, as v0.3.12 does"
    );
    assert!(
        !f.read(|picker| picker.can_accept()),
        "Save waits on the probe"
    );
    f.checks.say(Line::Reply(
        r#"{"t":"picker","id":1,"op":"save","ok":true,"review":1,"path":"/d/sa-d3-new.txt","collision":false}"#.into(),
    ));
    f.settle(|f| f.read(|picker| picker.accept_state().save_ready()));
    assert!(
        f.read(|picker| picker.can_accept()),
        "a free valid name enables Save"
    );
    assert!(save_enabled(&mut f), "the drawn Save control is enabled");
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"notes.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-plain"}]}"#.into()));
    f.cx.run_until_parked();
    assert_eq!(
        f.checks.sent().len(),
        1,
        "a later rows line in the same folder probes nothing again"
    );
    assert!(f.read(|picker| picker.can_accept()), "Save stays enabled");
}

#[gpui::test]
fn picker_gm_empty_save_name_keeps_save_disabled(cx: &mut TestAppContext) {
    let mut f = landed("", cx);
    assert!(f.checks.sent().is_empty(), "an empty name is never probed");
    assert!(!f.read(|picker| picker.can_accept()));
    assert!(
        !save_enabled(&mut f),
        "the drawn Save control stays disabled"
    );
}

// #428: a start folder that lists no rows, empty or emptied by the caller's filter, still probes the supplied name.
#[gpui::test]
fn picker_supplied_save_name_is_probed_in_an_empty_start_folder(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","folder":"/d","name":"test.srt"}"#,
        None,
    );
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/d","n":0,"v":1,"w":true}"#.into(),
    ));
    f.cx.run_until_parked();
    assert_eq!(f.read(|picker| picker.listing.state), "empty");
    assert_eq!(
        f.checks.sent(),
        [r#"{"op":"save","folder":"/d","name":"test.srt","c":"picker","id":1}"#],
        "the supplied name is probed once the empty folder lands"
    );
    f.checks.say(Line::Reply(
        r#"{"t":"picker","id":1,"op":"save","ok":true,"review":1,"path":"/d/test.srt","collision":false}"#.into(),
    ));
    f.settle(|f| f.read(|picker| picker.accept_state().save_ready()));
    assert!(f.read(|picker| picker.can_accept()));
    assert!(save_enabled(&mut f), "the drawn Save control is enabled");
}

// The Save control as the painted frame reports it to the harness.
fn save_enabled(f: &mut super::tests::fixture::Fixture) -> bool {
    f.read(|picker| picker.drawn.arm());
    f.cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let controls: String = f
        .read(|picker| picker.drawn.controls())
        .split_whitespace()
        .collect();
    // Sample input: [{"name":"Save","visible":true,"enabled":true,"focused":false,"hovered":false,"pressed":false,"centre":"1 2","bounds":[0,0,60,30]}].
    let at = controls
        .find(r#""name":"Save""#)
        .unwrap_or_else(|| panic!("a drawn Save control: {controls}"));
    let control = &controls[at..at + controls[at..].find('}').unwrap()];
    control.contains(r#""enabled":true"#)
}
