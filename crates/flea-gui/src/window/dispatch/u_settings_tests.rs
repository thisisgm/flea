//! U-SETTINGS: text size chords, the standing view a tab leaves, and the Settings panel keys, on the production Shell.
use super::{
    effect_tests::close,
    test_shell::{land, open_with, row},
};
use crate::{state::Shell, window::u07_tests::read};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn home() -> Vec<flea::protocol::RowFacts> {
    vec![
        row("alpha", true),
        row("beta", true),
        row("notes.txt", false),
    ]
}

// The production Shell on a landed /files, with `seed` written through the settings writer the panel uses.
fn landed<'a>(
    cx: &'a mut TestAppContext,
    seed: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    cx.update(|cx| cx.set_global(crate::state::constructor_fixture::NoSettingsWorker));
    let (shell, cx) = open_with(cx, r#""keys":"default","view":"list""#, None);
    land(&shell, cx, "/files", home(), 1);
    if !seed.is_empty() {
        let patch = flea::jsondoc::parse(seed).unwrap();
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| shell.change_settings(patch, window, cx))
        });
        relanded(&shell, cx);
    }
    cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    cx.run_until_parked();
    (shell, cx)
}

// A listing the last step asked for is answered, as the backend answers it on a real window.
fn relanded(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let (busy, listing) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.pane.busy(), host.pane.list_requests)
    });
    if busy {
        let place = shell.read_with(cx, |shell, _| {
            shell.panes[0].as_ref().unwrap().pane.place().to_owned()
        });
        land(shell, cx, &place, home(), listing);
    }
}

fn keys(shell: &Entity<Shell>, cx: &mut VisualTestContext, strokes: &[&str]) {
    for stroke in strokes {
        cx.simulate_keystrokes(stroke);
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        relanded(shell, cx);
    }
}

fn stored(shell: &Entity<Shell>, cx: &mut VisualTestContext, path: &[&str]) -> Option<String> {
    shell.read_with(cx, |shell, _| {
        flea::ui_logic_st1::pages::at(&shell.launch.settings.data, path).map(|value| match value {
            flea::jsondoc::Json::Str(word) => word.clone(),
            other => format!("{other:?}"),
        })
    })
}

// The Settings panel's section, the side that holds the keys, and the id of the cursor row.
fn panel(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (String, String, String) {
    shell.read_with(cx, |shell, cx| {
        let Some(view) = shell.settings.panel.as_ref() else {
            return ("closed".into(), String::new(), String::new());
        };
        let panel = &view.read(cx).panel;
        (
            panel.section.to_string(),
            format!("{:?}", panel.side),
            panel
                .rows
                .get(panel.cursor)
                .map(|row| row.id.clone())
                .unwrap_or_default(),
        )
    })
}

// EY-21, TextSize.js announce: a text size chord says the size it left and its way back in the footer.
#[gpui::test]
fn ey21_text_size_chords_say_the_size_in_the_footer(cx: &mut TestAppContext) {
    let (shell, cx) = landed(cx, "");
    let mut said = Vec::new();
    for stroke in ["ctrl-shift-+", "ctrl-shift-0", "ctrl-shift--"] {
        keys(&shell, cx, &[stroke]);
        said.push((
            stroke,
            stored(&shell, cx, &["display", "textSize", "mode"]),
            read(&shell, cx, "lastMessage", &[]),
        ));
    }
    eprintln!("EY21 {said:?}");
    assert!(
        said[0].2.starts_with("Text size ") && said[0].2.contains("px. "),
        "{:?}",
        said[0]
    );
    assert!(
        said[1].2.starts_with("Text size follows Omarchy, "),
        "{:?}",
        said[1]
    );
    assert!(
        said[2].2.starts_with("Text size ") && said[2].2.contains("px"),
        "{:?}",
        said[2]
    );
    close(&shell, cx);
}

// EY-181: returning to the first tab's Grid after switching the second to List must store Grid for the next launch.
#[gpui::test]
fn ey181_a_tab_switch_makes_its_view_the_standing_one(cx: &mut TestAppContext) {
    let (shell, cx) = landed(cx, "");
    keys(&shell, cx, &["ctrl-3"]);
    assert_eq!(read(&shell, cx, "viewMode", &[]), "grid");
    assert_eq!(stored(&shell, cx, &["view"]).as_deref(), Some("grid"));
    keys(&shell, cx, &["t"]);
    keys(&shell, cx, &["ctrl-1"]);
    assert_eq!(read(&shell, cx, "tabCount", &[]), "2");
    assert_eq!(
        stored(&shell, cx, &["view"]).as_deref(),
        Some("list"),
        "the second tab's own switch is stored"
    );
    keys(&shell, cx, &["ctrl-pagedown"]);
    assert_eq!(read(&shell, cx, "tabIndex", &[]), "0");
    assert_eq!(
        read(&shell, cx, "viewMode", &[]),
        "grid",
        "the first tab brings back its Grid"
    );
    assert_eq!(
        stored(&shell, cx, &["view"]).as_deref(),
        Some("grid"),
        "the Grid the window is left on is the standing view"
    );
    close(&shell, cx);
}

// EY-220, pf.settings.display-size: from the listing, Ctrl+comma, Tab, Down to Display, Tab, Down, Right steps 14 to 16.
#[gpui::test]
fn ey220_display_answers_the_keys_from_the_listing(cx: &mut TestAppContext) {
    let (shell, cx) = landed(cx, r#"{"display":{"textSize":{"mode":14}}}"#);
    let mut seen = Vec::new();
    for stroke in [
        "ctrl-,", "tab", "down", "down", "down", "down", "down", "tab", "down", "right",
    ] {
        keys(&shell, cx, &[stroke]);
        seen.push((stroke, panel(&shell, cx)));
    }
    eprintln!("EY220 {seen:?}");
    assert_eq!(
        seen[8].1,
        ("display".into(), "Pane".into(), "textStop".into()),
        "Down lights the Text size ruler"
    );
    assert_eq!(
        stored(&shell, cx, &["display", "textSize", "mode"]).as_deref(),
        Some("Num(\"16\")")
    );
    close(&shell, cx);
}

// EY-197, pf.places.favorite-reorder: comma, Tab, Down to Places, Tab, Return adds the folder after Docs, Down, Shift+J moves Docs down.
#[gpui::test]
fn ey197_shift_j_moves_the_favourite_under_the_cursor(cx: &mut TestAppContext) {
    let (shell, cx) = landed(
        cx,
        r#"{"display":{"textSize":{"mode":14}},"places":{"favourites":[{"label":"Docs","path":"/docs"}]}}"#,
    );
    let mut seen = Vec::new();
    for stroke in [",", "tab", "down", "tab", "enter", "down", "shift-j"] {
        keys(&shell, cx, &[stroke]);
        seen.push((
            stroke,
            panel(&shell, cx),
            stored(&shell, cx, &["places", "favourites"]),
        ));
    }
    eprintln!("EY197 {seen:#?}");
    let order = stored(&shell, cx, &["places", "favourites"]).unwrap_or_default();
    let docs = order.find("/docs");
    let files = order.find("/files");
    assert!(
        files.is_some() && docs.is_some() && files < docs,
        "Shift+J moves Docs below the added folder: {order}"
    );
    close(&shell, cx);
}
