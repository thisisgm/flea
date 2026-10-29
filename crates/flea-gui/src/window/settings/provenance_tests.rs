use super::*;
use flea::ui_logic_st1::{
    about,
    rows::{Kind, Row},
    update, Text,
};

// The About rows for a build whose owner query answered owner, with the updater's answer for an unowned build.
fn drawn(owner: &str) -> Vec<Row> {
    let mut facts = about::Facts {
        version: "0.4.0".into(),
        handler: "kitty-open.desktop".into(),
        update: update::answered(&update::idle(), "unchecked unowned - -\n", 0),
        ..about::Facts::default()
    };
    apply(&mut facts, owner, &about::PackageFacts::default());
    about::rows(&facts, true, "English", Vec::new())
}

fn row<'a>(rows: &'a [Row], label: &'static str) -> &'a Row {
    rows.iter()
        .find(|row| row.label == Text::Key(label))
        .expect(label)
}

#[test]
fn settings_about_unowned_build_names_its_source_and_package() {
    let rows = drawn("");
    assert_eq!(
        row(&rows, "settings.about.installed_from").value,
        Text::Key("settings.about.source_unowned")
    );
    assert_eq!(
        row(&rows, "settings.about.package").value,
        Text::Key("settings.about.not_owned")
    );
}

#[test]
fn settings_about_owned_build_with_failed_query_names_source_and_unknown_package() {
    let rows = drawn("flea-bin");
    assert_eq!(
        row(&rows, "settings.about.installed_from").value,
        Text::Key("settings.about.source_local")
    );
    assert_eq!(
        row(&rows, "settings.about.package").value,
        Text::Key("settings.about.not_reported")
    );
}

#[test]
fn settings_about_unowned_update_row_reads_built_from_source() {
    let rows = drawn("");
    let flea = row(&rows, "settings.about.update_flea");
    assert_eq!(flea.value, Text::Key("settings.update.source"));
    assert_eq!(flea.kind, Kind::Fact);
    assert!(flea.inert);
    assert_eq!(row(&rows, "settings.update.note_source").kind, Kind::Hint);
}

#[test]
fn settings_about_check_automatically_is_ticked_when_unset() {
    let rows = about::rows(
        &about::Facts::default(),
        about::auto_check(None),
        "English",
        Vec::new(),
    );
    assert!(row(&rows, "settings.about.check_automatically").on);
}

// The About panel and updater before and after claim.rs's owner step, seeded in one update so the window's own pacman probe cannot decide.
type Drawn = (Vec<Row>, update::Status);

fn probed(owner: &str, answer: &str, cx: &mut gpui::TestAppContext) -> (Drawn, Drawn) {
    let (backend, _requests) = crate::backend::Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let mut shell = crate::state::tests::shell_with(backend, cx);
        shell.launch.settings.data = flea::uischema::defaults();
        shell.open_settings(window, cx);
        shell
            .settings
            .panel
            .as_ref()
            .unwrap()
            .update(cx, |view, _| view.panel.show_section("about"));
        shell
    });
    entity.update(cx, |shell, cx| {
        shell.settings.update = update::Status {
            kind: "opr".to_owned(),
            ..update::idle()
        };
        shell.sync_settings_update(cx);
        let panel = shell.settings.panel.clone().unwrap();
        panel.update(cx, |view, _| {
            let blank = about::Facts::default();
            view.panel.facts.about.source = blank.source;
            view.panel.facts.about.package = blank.package;
            view.panel.rebuild();
        });
        let read = |shell: &crate::state::Shell, cx: &mut gpui::Context<crate::state::Shell>| {
            (
                panel.read(cx).panel.rows.clone(),
                shell.settings.update.clone(),
            )
        };
        let before = read(shell, cx);
        shell.settings_owner(owner, &about::package_facts(answer), cx);
        (before, read(shell, cx))
    })
}

#[gpui::test]
fn settings_about_an_unowned_probe_redraws_the_open_panel(cx: &mut gpui::TestAppContext) {
    let (_, (rows, status)) = probed("", "", cx);
    assert_eq!(status.state, update::State::Unchecked);
    assert_eq!(status.kind, "unowned");
    assert_eq!(
        row(&rows, "settings.about.installed_from").value,
        Text::Key("settings.about.source_unowned")
    );
    assert_eq!(
        row(&rows, "settings.about.package").value,
        Text::Key("settings.about.not_owned")
    );
    assert_eq!(
        row(&rows, "settings.about.update_flea").value,
        Text::Key("settings.update.source")
    );
}

#[gpui::test]
fn settings_about_an_owned_probe_redraws_the_open_panel(cx: &mut gpui::TestAppContext) {
    let (before, after) = probed("flea", super::package_tests::OPR_INFO, cx);
    assert_eq!(
        after.1, before.1,
        "an owned build keeps the updater's own answer"
    );
    assert_eq!(
        row(&after.0, "settings.about.installed_from").value,
        Text::Key("settings.about.source_opr")
    );
    assert_eq!(
        row(&after.0, "settings.about.package").value,
        Text::data("flea 0.3.2-1")
    );
}

#[gpui::test]
fn settings_about_a_local_probe_redraws_the_open_panel(cx: &mut gpui::TestAppContext) {
    let (before, after) = probed("flea", super::package_tests::LOCAL_INFO, cx);
    assert_eq!(
        after.1, before.1,
        "a local package keeps the updater's own answer"
    );
    assert_eq!(
        row(&after.0, "settings.about.installed_from").value,
        Text::Key("settings.about.source_local")
    );
    assert_eq!(
        row(&after.0, "settings.about.package").value,
        Text::data("flea 0.3.3-1")
    );
}

#[gpui::test]
fn settings_about_a_failed_package_probe_keeps_unknown_rows(cx: &mut gpui::TestAppContext) {
    let (before, after) = probed("flea", "", cx);
    assert_eq!(after.1, before.1);
    assert_eq!(
        row(&after.0, "settings.about.installed_from").value,
        Text::Key("settings.about.source_local")
    );
    assert_eq!(
        row(&after.0, "settings.about.package").value,
        Text::Key("settings.about.not_reported")
    );
    assert_eq!(
        row(&after.0, "settings.about.built").value,
        Text::Key("settings.about.not_recorded")
    );
}
