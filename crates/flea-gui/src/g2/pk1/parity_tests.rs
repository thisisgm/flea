//! Mounted chooser regressions driven through production input and child replies.
use super::process::Line;
use super::tests::fixture::{fixture, Fixture};
use gpui::{Modifiers, TestAppContext};

fn seed(f: &mut Fixture, path: &str) {
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"listed","n":3,"path":"{path}"}}"#
    )));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"a.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-x-generic"},{"n":"b.txt","d":false,"s":7,"m":0,"p":33188,"i":"text-x-generic"},{"n":"sub","d":true,"s":0,"m":0,"p":16877,"i":"folder"}]}"#.into()));
    f.cx.run_until_parked();
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
}

#[gpui::test]
fn pb_picker1_grid_switch_is_mounted(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seed(&mut f, "/d");
    f.cx.simulate_keystrokes("ctrl-3");
    f.cx.run_until_parked();
    assert!(
        f.cx.debug_bounds("picker-grid").is_some(),
        "Ctrl+3 mounts the actual grid"
    );
    f.cx.simulate_keystrokes("ctrl-1");
    f.cx.run_until_parked();
    assert!(f.cx.debug_bounds("picker-grid").is_none());
}

#[gpui::test]
fn pb_picker1_space_marks_through_checks(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d","multiple":true}"#, None);
    seed(&mut f, "/d");
    f.cx.simulate_keystrokes("space");
    f.cx.run_until_parked();
    assert!(
        f.checks
            .sent()
            .iter()
            .any(|line| line.contains("\"op\":\"mark\"") && line.contains("/d/a.txt")),
        "Space reaches the mark backend"
    );
    f.checks.say(Line::Reply(r#"{"t":"picker","id":1,"op":"mark","ok":true,"marks":[{"path":"/d/a.txt","bytes":3}],"removed":0}"#.into()));
    f.cx.run_until_parked();
    assert_eq!(f.read(|picker| picker.marks.len()), 1);
    assert!(f.cx.debug_bounds("picker-selection-status").is_some());
}

#[gpui::test]
fn pb_picker1_answer_tab_space_cancels(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/"}"#, None);
    seed(&mut f, "/");
    f.cx.simulate_keystrokes("tab space");
    f.cx.run_until_parked();
    assert!(
        f.read(|picker| picker.answered()),
        "Tab from list reaches Cancel; Space activates it"
    );
}

#[gpui::test]
fn pb_picker1_parent_and_back_dispatch(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seed(&mut f, "/d");
    f.cx.simulate_keystrokes("backspace");
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/",
        "Backspace goes to the parent"
    );
    seed(&mut f, "/");
    let at =
        f.cx.debug_bounds("picker-back")
            .expect("Back is drawn")
            .center();
    f.cx.simulate_click(at, Modifiers::none());
    f.cx.run_until_parked();
    assert_eq!(f.read(|picker| picker.listing.location.clone()), "/d");
}

#[gpui::test]
fn pb_picker1_filter_keyboard_choice_relists(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"folder":"/","filters":[{"label":"Text","globs":["*.txt"]},{"label":"Images","globs":["*.png"]}]}"#,
        None,
    );
    seed(&mut f, "/");
    f.cx.simulate_keystrokes("tab down");
    f.cx.run_until_parked();
    assert!(
        f.cx.debug_bounds("picker-filter-menu").is_some(),
        "Down on focused trigger opens filters"
    );
    f.cx.simulate_keystrokes("down enter");
    f.cx.run_until_parked();
    assert_eq!(f.read(|picker| picker.listing.chip), 1);
    assert!(f.listing.sent().last().unwrap().contains("*.png"));
}

#[gpui::test]
fn pb_picker1_save_has_new_folder(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"mode":"save","folder":"/d","name":"a.txt"}"#, None);
    seed(&mut f, "/d");
    let at =
        f.cx.debug_bounds("picker-new-folder")
            .expect("New Folder is mounted before Save As")
            .center();
    f.cx.simulate_click(at, Modifiers::none());
    f.cx.run_until_parked();
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    assert!(
        f.checks
            .sent()
            .iter()
            .any(|line| line.contains("\"c\":\"mkdir\"")),
        "New Folder uses the existing backend mkdir action"
    );
}

#[gpui::test]
fn pb_picker1_grid_visual_neighbours(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seed(&mut f, "/d");
    f.cx.simulate_keystrokes("ctrl-3 right");
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        1,
        "Right steps to the visual neighbour"
    );
}

#[gpui::test]
fn pb_picker1_jump_uses_existing_child(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seed(&mut f, "/d");
    f.cx.simulate_keystrokes("ctrl-l");
    f.cx.run_until_parked();
    assert!(
        f.listing
            .sent()
            .iter()
            .any(|line| line.contains("\"c\":\"jump\"")),
        "The picker asks ranked suggestions through its existing child"
    );
}

#[gpui::test]
fn pb_picker1_check_header_and_compact_date_share_geometry(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d","multiple":true}"#, None);
    seed(&mut f, "/d");
    let listing = f.read(|picker| picker.browse.listing.clone());
    let (leading, date_width, date) = f.cx.update(|_, cx| {
        let view = listing.read(cx);
        let drawn = view.drawn();
        let layout = drawn.header.drawn.layout;
        (
            layout.name.x - drawn.drawn.metrics.label_x(),
            layout
                .cell(crate::list::columns::Column::Date)
                .map(|cell| cell.width),
            drawn.visible()[0]
                .prepared
                .as_ref()
                .unwrap()
                .date_text
                .clone(),
        )
    });
    assert_eq!(leading.round(), 27., "18px check followed by the 9px gap");
    assert_eq!(date_width, Some(80.));
    assert_eq!(date.len(), 10, "Picker dates show the day without time");
}

#[gpui::test]
fn pb_picker1_refused_path_returns_to_entry(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seed(&mut f, "/d");
    f.cx.simulate_keystrokes("ctrl-l");
    f.cx.simulate_input("/gone");
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    f.listing.say(Line::Reply(
        r#"{"t":"peeked","path":"/gone","first":0,"n":0,"failed":true,"rows":[]}"#.into(),
    ));
    f.cx.run_until_parked();
    let path = f.update(|picker, cx| picker.browse.path_view(cx));
    let (editing, text, focused) = f.cx.update(|window, cx| {
        let view = path.read(cx);
        (
            view.editing(),
            view.entry.text.clone(),
            view.focus.is_focused(window),
        )
    });
    assert!(
        editing && focused,
        "A refusal returns the caret to the initiating path field"
    );
    assert_eq!(text, "/gone");
}

#[gpui::test]
fn pb_picker1_select_all_resolves_unheld_names(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d","multiple":true}"#, None);
    seed(&mut f, "/d");
    f.cx.simulate_keystrokes("ctrl-a");
    f.cx.run_until_parked();
    assert!(f
        .listing
        .sent()
        .last()
        .unwrap()
        .contains(r#""rows":[0,1,2]"#));
    f.listing.say(Line::Reply(
        r#"{"t":"paths","paths":["/d/a.txt","/d/b.txt","/d/sub"]}"#.into(),
    ));
    f.cx.run_until_parked();
    assert!(f.checks.sent().last().unwrap().contains(r#""op":"select""#));
    f.checks.say(Line::Reply(r#"{"t":"picker","id":1,"op":"select","ok":true,"marks":[{"path":"/d/a.txt","bytes":3},{"path":"/d/b.txt","bytes":7}]}"#.into()));
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.selection_status()),
        "2 selected · 10 B"
    );
}

#[gpui::test]
fn pb_picker1_range_input_survives_a_pending_check(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d","multiple":true}"#, None);
    seed(&mut f, "/d");
    f.cx.simulate_keystrokes("ctrl-a shift-down");
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        1,
        "Range cursor moves even while its prior selection is resolving"
    );
    f.listing.say(Line::Reply(
        r#"{"t":"paths","paths":["/d/a.txt","/d/b.txt","/d/sub"]}"#.into(),
    ));
    f.cx.run_until_parked();
    f.checks.say(Line::Reply(r#"{"t":"picker","id":1,"op":"select","ok":true,"marks":[{"path":"/d/a.txt","bytes":3},{"path":"/d/b.txt","bytes":7}]}"#.into()));
    f.cx.run_until_parked();
    assert_eq!(
        f.listing
            .sent()
            .iter()
            .filter(|line| line.contains(r#""c":"paths""#))
            .count(),
        2,
        "The next range is replayed after the held check"
    );
}

#[gpui::test]
fn pb_picker1_folder_symlink_uses_the_wire_icon(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"folder":"/d","multiple":true,"directory":true}"#,
        None,
    );
    f.listing
        .say(Line::Reply(r#"{"t":"listed","n":1,"path":"/d"}"#.into()));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"linked","d":false,"s":1,"m":0,"p":41471,"i":"folder","k":0}],"kinds":["Folder link"]}"#.into()));
    f.cx.run_until_parked();
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f.cx.simulate_keystrokes("space");
    f.cx.run_until_parked();
    assert!(
        f.checks
            .sent()
            .iter()
            .any(|line| line.contains("/d/linked") && line.contains(r#""directory":true"#)),
        "The wire icon identifies a folder link independently of its localized kind description"
    );
}

#[gpui::test]
fn pb_picker1_filter_choice_rides_the_portal_reply(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"folder":"/","filters":[{"label":"Text","globs":["*.txt"]},{"label":"Images","globs":["*.png"]}]}"#,
        None,
    );
    seed(&mut f, "/");
    f.cx.simulate_keystrokes("tab down down enter");
    f.cx.run_until_parked();
    seed(&mut f, "/");
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    f.checks.say(Line::Reply(
        r#"{"t":"picker","id":1,"op":"mark","ok":true,"marks":[{"path":"/a.txt","bytes":3}]}"#
            .into(),
    ));
    f.settle(|f| f.written().is_some());
    assert_eq!(
        f.written().as_deref(),
        Some(r#"{"response":0,"uris":["file:///a.txt"],"filter":1}"#)
    );
}

#[gpui::test]
fn pb_picker1_new_folder_opens_only_the_confirmed_child(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"mode":"save","folder":"/d","name":"a.txt"}"#, None);
    seed(&mut f, "/d");
    let at = f.cx.debug_bounds("picker-new-folder").unwrap().center();
    f.cx.simulate_click(at, Modifiers::none());
    f.cx.run_until_parked();
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    f.checks.say(Line::Reply(
        r#"{"t":"made","ok":true,"path":"/d/New Folder"}"#.into(),
    ));
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/d/New Folder"
    );
    assert!(f.listing.sent().last().unwrap().contains("/d/New Folder"));
}

#[gpui::test]
fn pb_picker1_filter_line_select_all_uses_selection_owner(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d","multiple":true}"#, None);
    seed(&mut f, "/d");
    f.cx.simulate_keystrokes("/");
    f.cx.simulate_input("a");
    f.cx.run_until_parked();
    f.listing.say(Line::Reply(
        r#"{"t":"filtered","id":1,"query":"a","n":3,"rows":[0],"listing":1}"#.into(),
    ));
    f.cx.run_until_parked();
    f.cx.simulate_keystrokes("ctrl-a");
    f.cx.run_until_parked();
    assert!(
        f.listing.sent().last().unwrap().contains(r#""rows":[0]"#),
        "Ctrl+A on the filter line resolves exactly the matching file identities"
    );
    f.listing
        .say(Line::Reply(r#"{"t":"paths","paths":["/d/a.txt"]}"#.into()));
    f.cx.run_until_parked();
    assert!(f.checks.sent().last().unwrap().contains(r#""op":"select""#));
}

#[gpui::test]
fn pb_picker1_ctrl_backspace_edits_the_save_name(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"mode":"save","folder":"/d","name":"a.txt"}"#, None);
    seed(&mut f, "/d");
    let name = f
        .read(|picker| picker.browse.filename.clone())
        .expect("Save has its name field");
    let focus = f.cx.update(|_, cx| name.read(cx).focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f.cx.run_until_parked();
    let sent = f.listing.sent().len();
    f.cx.simulate_keystrokes("ctrl-backspace");
    f.cx.run_until_parked();
    assert!(
        !f.listing.sent()[sent..]
            .iter()
            .any(|line| line.contains(r#""c":"list""#)),
        "Ctrl+Backspace in the name field edits the name, never the folder"
    );
}

#[gpui::test]
fn pb_picker1_shift_arrow_moves_the_cursor_without_a_range(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seed(&mut f, "/d");
    let before = f.read(|picker| picker.listing.cursor);
    f.cx.simulate_keystrokes("shift-down");
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        before + 1,
        "Shift+Down moves the cursor where no range is allowed"
    );
    f.cx.simulate_keystrokes("shift-up");
    f.cx.run_until_parked();
    assert_eq!(f.read(|picker| picker.listing.cursor), before);
}

// v0.3.12 binds parent to the listing context only (ui/js/Keymap.js:19 ctrl-up, :78 alt-up).
fn name_field_keeps_the_parent_chord(cx: &mut TestAppContext, preset: &str, keystroke: &str) {
    let mut f = fixture(cx, r#"{"mode":"save","folder":"/d","name":"a.txt"}"#, None);
    seed(&mut f, "/d");
    let settings = flea::jsondoc::parse(&format!(r#"{{"keys":"{preset}"}}"#)).unwrap();
    f.update(|picker, cx| picker.apply_settings(&settings, cx));
    let name = f
        .read(|picker| picker.browse.filename.clone())
        .expect("Save has its name field");
    let focus = f.cx.update(|_, cx| name.read(cx).focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f.cx.run_until_parked();
    let sent = f.listing.sent().len();
    f.cx.simulate_keystrokes(keystroke);
    f.cx.run_until_parked();
    assert!(
        !f.listing.sent()[sent..]
            .iter()
            .any(|line| line.contains(r#""c":"list""#)),
        "{keystroke} with the name field focused never changes the folder"
    );
}

#[gpui::test]
fn pb_picker1_ctrl_up_leaves_the_save_name_alone(cx: &mut TestAppContext) {
    name_field_keeps_the_parent_chord(cx, "mac", "ctrl-up");
}

#[gpui::test]
fn pb_picker1_alt_up_leaves_the_save_name_alone(cx: &mut TestAppContext) {
    name_field_keeps_the_parent_chord(cx, "windows", "alt-up");
}
