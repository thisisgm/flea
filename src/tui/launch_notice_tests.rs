// Each state-file outcome at launch names its own footer key, and a settle refusal outranks the rest.
use super::launch_notice;
use super::words::Text;

#[test]
fn each_launch_state_names_its_own_footer_sentence() {
    let refused = launch_notice(Err("ui.json.lock stayed locked".into()), true, true);
    assert!(
        matches!(&refused, Some(Text::Args("tui.actions.save_settings", args)) if matches!(args.as_slice(), [Text::Data(why)] if why.contains("stayed locked"))),
        "{refused:?}"
    );
    let unreadable = launch_notice(Ok(()), true, true);
    assert!(
        matches!(unreadable, Some(Text::Key("settings.error.unreadable"))),
        "{unreadable:?}"
    );
    let held = launch_notice(Ok(()), false, true);
    assert!(
        matches!(held, Some(Text::Key("settings.error.save_session"))),
        "{held:?}"
    );
    assert!(launch_notice(Ok(()), false, false).is_none());
}
