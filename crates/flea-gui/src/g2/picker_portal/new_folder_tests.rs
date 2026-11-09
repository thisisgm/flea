//! #417 and PR #304: Save's New Folder… names the folder in a card before anything is made, then enters it.
use super::process::Line;
use super::tests::fixture::{fixture, Fixture};
use gpui::{Modifiers, TestAppContext};

fn seeded(cx: &mut TestAppContext) -> Fixture<'_> {
    let f = fixture(cx, r#"{"mode":"save","folder":"/d","name":"a.txt"}"#, None);
    f.listing
        .say(Line::Reply(r#"{"t":"listed","n":2,"path":"/d"}"#.into()));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"a.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-x-generic"},{"n":"sub","d":true,"s":0,"m":0,"p":16877,"i":"folder"}]}"#.into()));
    f.cx.run_until_parked();
    f
}

fn open_card(f: &mut Fixture) {
    let at = f.cx.debug_bounds("picker-new-folder").unwrap().center();
    f.cx.simulate_click(at, Modifiers::none());
    f.cx.run_until_parked();
}

fn mkdirs(f: &Fixture) -> Vec<String> {
    f.checks
        .sent()
        .into_iter()
        .filter(|line| line.contains(r#""c":"mkdir""#))
        .collect()
}

// The card's field text and whether its whole text is selected, or None with no card.
fn card_field(f: &mut Fixture) -> Option<(String, bool)> {
    let picker = f.picker.clone();
    f.cx.update(|_, cx| {
        let field = picker.read(cx).chrome.folder.clone()?;
        let field = field.read(cx);
        let text = field.text().to_owned();
        Some((text.clone(), field.entry.selected == (0..text.len())))
    })
}

fn card_focused(f: &mut Fixture) -> bool {
    let picker = f.picker.clone();
    f.cx.update(|window, cx| {
        let field = picker.read(cx).chrome.folder.clone();
        field.is_some_and(|field| field.read(cx).focus.is_focused(window))
    })
}

fn save_focused(f: &mut Fixture) -> bool {
    let picker = f.picker.clone();
    f.cx.update(|window, cx| {
        let field = picker.read(cx).browse.filename.clone().unwrap();
        field.read(cx).focus.is_focused(window)
    })
}

fn cancel_focused(f: &mut Fixture) -> bool {
    let picker = f.picker.clone();
    f.cx.update(|window, cx| picker.read(cx).chrome.cancel.is_focused(window))
}

fn card_error(f: &mut Fixture) -> String {
    f.read(|picker| picker.chrome.folder_error.clone())
}

fn save_name(f: &mut Fixture) -> String {
    let picker = f.picker.clone();
    f.cx.update(|_, cx| {
        let field = picker.read(cx).browse.filename.clone().unwrap();
        field.read(cx).text().to_owned()
    })
}

#[gpui::test]
fn picker_new_folder_names_the_folder_before_it_is_made(cx: &mut TestAppContext) {
    let mut f = seeded(cx);
    // The start folder's check answers, as the backend does, so the new folder's own check can go out.
    f.checks.say(Line::Reply(
        r#"{"t":"picker","id":1,"op":"save","ok":true,"review":1,"path":"/d/a.txt","collision":false}"#.into(),
    ));
    f.cx.run_until_parked();
    open_card(&mut f);
    assert!(
        mkdirs(&f).is_empty(),
        "New Folder… makes nothing until a name is confirmed"
    );
    assert!(
        f.cx.debug_bounds("picker-folder-card").is_some(),
        "the naming card is drawn"
    );
    assert_eq!(
        card_field(&mut f),
        Some(("New Folder".into(), true)),
        "the field opens on the window's default name, selected whole"
    );
    f.cx.simulate_input("Invoices");
    f.cx.run_until_parked();
    assert_eq!(
        card_field(&mut f).unwrap().0,
        "Invoices",
        "typing replaces the selection"
    );
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    assert_eq!(
        mkdirs(&f),
        [r#"{"c":"mkdir","path":"/d","name":"Invoices"}"#]
    );
    assert!(
        f.read(|picker| !picker.answered()),
        "Enter in the card never answers the request"
    );
    f.checks.say(Line::Reply(
        r#"{"t":"made","ok":true,"path":"/d/Invoices"}"#.into(),
    ));
    f.cx.run_until_parked();
    assert!(
        card_field(&mut f).is_none(),
        "the card closes once the folder is made"
    );
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/d/Invoices"
    );
    assert_eq!(save_name(&mut f), "a.txt", "the save name is kept");
    assert!(
        save_focused(&mut f),
        "the Save As field takes the keyboard again"
    );
    assert_eq!(
        f.read(|picker| picker.chrome.history.clone()),
        ["/d"],
        "Back returns to the folder the card named into"
    );
    f.listing.say(Line::Reply(
        r#"{"t":"listed","n":0,"path":"/d/Invoices"}"#.into(),
    ));
    f.cx.run_until_parked();
    assert!(
        f.cx.debug_bounds("picker-empty-hero").is_some(),
        "the empty new folder draws the empty hero (0401-picker-new-folder-made)"
    );
    assert!(
        f.checks
            .sent()
            .iter()
            .any(|line| line.contains(r#""op":"save","folder":"/d/Invoices","name":"a.txt""#)),
        "the kept name is checked in the new folder, so Save can answer there"
    );
}

#[gpui::test]
fn picker_new_folder_refusal_keeps_the_card_with_the_window_sentence(cx: &mut TestAppContext) {
    let mut f = seeded(cx);
    open_card(&mut f);
    f.cx.simulate_input("sub");
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    f.checks.say(Line::Reply(
        r#"{"t":"error","where":"mkdir","path":"/d/sub","msg":"a folder or file with that name already exists"}"#.into(),
    ));
    f.cx.run_until_parked();
    assert_eq!(
        card_field(&mut f).unwrap().0,
        "sub",
        "the typed name stays for correction"
    );
    assert_eq!(
        card_error(&mut f),
        "An item with that name is already here."
    );
    assert_eq!(f.read(|picker| picker.listing.location.clone()), "/d");
    f.cx.simulate_input("2");
    f.cx.run_until_parked();
    assert_eq!(card_error(&mut f), "", "an edit clears the refusal");
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    assert_eq!(
        mkdirs(&f).last().map(String::as_str),
        Some(r#"{"c":"mkdir","path":"/d","name":"sub2"}"#),
        "the corrected name is sent to the same folder"
    );
}

#[gpui::test]
fn picker_new_folder_escape_and_cancel_close_only_the_card(cx: &mut TestAppContext) {
    let mut f = seeded(cx);
    assert_eq!(
        f.read(|picker| picker.hints()).as_ref(),
        "enter save · esc cancel"
    );
    open_card(&mut f);
    assert_eq!(
        f.read(|picker| picker.hints()).as_ref(),
        "enter create · esc close",
        "the status hint names what enter and esc do on the card"
    );
    f.cx.simulate_keystrokes("escape");
    f.cx.run_until_parked();
    assert!(card_field(&mut f).is_none(), "esc closes the card");
    assert_eq!(
        f.read(|picker| picker.hints()).as_ref(),
        "enter save · esc cancel",
        "the chooser's own hint returns with the card closed"
    );
    assert!(
        f.read(|picker| !picker.answered()),
        "esc in the card never cancels the request"
    );
    open_card(&mut f);
    f.cx.simulate_keystrokes("tab enter");
    f.cx.run_until_parked();
    assert!(
        card_field(&mut f).is_none(),
        "Cancel, reached by Tab, closes the card"
    );
    assert!(f.read(|picker| !picker.answered()));
    assert!(mkdirs(&f).is_empty(), "nothing was made");
    assert_eq!(f.read(|picker| picker.listing.location.clone()), "/d");
}

#[gpui::test]
fn picker_new_folder_hint_outranks_a_standing_filter(cx: &mut TestAppContext) {
    let mut f = seeded(cx);
    // Save keeps the keyboard on Save As, so the query is typed into the filter directly.
    let picker = f.picker.clone();
    f.cx.update(|_, cx| {
        picker.update(cx, |picker, cx| {
            picker.find_update(
                |filter, pane| {
                    filter.start();
                    filter.typed("s", pane)
                },
                cx,
            )
        })
    });
    f.cx.run_until_parked();
    assert!(
        f.read(|picker| picker.find.holds_query()),
        "a filter query stands before the card opens"
    );
    open_card(&mut f);
    assert!(
        card_field(&mut f).is_some(),
        "the card opens over the filter"
    );
    assert_eq!(
        f.read(|picker| picker.hints()).as_ref(),
        "enter create · esc close",
        "enter and esc act on the card, not on the filter"
    );
}

#[gpui::test]
fn picker_new_folder_create_waits_for_a_name(cx: &mut TestAppContext) {
    let mut f = seeded(cx);
    open_card(&mut f);
    f.cx.simulate_keystrokes("backspace enter");
    f.cx.run_until_parked();
    assert_eq!(card_field(&mut f).unwrap().0, "");
    assert!(mkdirs(&f).is_empty(), "an empty name creates nothing");
    f.cx.simulate_keystrokes("tab tab");
    f.cx.run_until_parked();
    assert!(
        card_focused(&mut f),
        "Create is no Tab stop while the name is empty, so Tab wraps from Cancel to the field"
    );
    // The op1 card shell tags its buttons by place: op1-button-0 is Cancel, op1-button-1 is Create.
    let at = f.cx.debug_bounds("op1-button-1").unwrap().center();
    f.cx.simulate_click(at, Modifiers::none());
    f.cx.run_until_parked();
    assert!(
        mkdirs(&f).is_empty(),
        "a click on Create makes nothing while the name is empty"
    );
}

#[gpui::test]
fn picker_new_folder_esc_frees_the_chooser_while_a_create_is_out(cx: &mut TestAppContext) {
    let mut f = seeded(cx);
    open_card(&mut f);
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    assert_eq!(
        mkdirs(&f).len(),
        1,
        "the create is out and no answer arrives"
    );
    f.cx.simulate_keystrokes("escape");
    f.cx.run_until_parked();
    assert!(
        card_field(&mut f).is_none(),
        "esc closes the card even while the create waits"
    );
    assert!(f.read(|picker| !picker.answered()));
    assert!(
        cancel_focused(&mut f),
        "the chooser's Cancel holds the keyboard until the create answers, as before the card"
    );
    f.checks.say(Line::Reply(
        r#"{"t":"made","ok":true,"path":"/d/New Folder"}"#.into(),
    ));
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/d/New Folder"
    );
    assert!(
        save_focused(&mut f),
        "Save As takes the keyboard back once the create answers"
    );
}

#[gpui::test]
fn picker_new_folder_refusal_after_the_card_closed_is_said(cx: &mut TestAppContext) {
    let mut f = seeded(cx);
    open_card(&mut f);
    f.cx.simulate_input("sub");
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    let at = f.cx.debug_bounds("op1-button-0").unwrap().center();
    f.cx.simulate_click(at, Modifiers::none());
    f.cx.run_until_parked();
    assert!(
        card_field(&mut f).is_none(),
        "a click on the card's Cancel closes it while the create waits"
    );
    open_card(&mut f);
    assert!(
        card_field(&mut f).is_none(),
        "New Folder… stays shut until the create out answers"
    );
    assert!(
        cancel_focused(&mut f),
        "the chooser's Cancel holds the keyboard until the create answers"
    );
    f.checks.say(Line::Reply(
        r#"{"t":"error","where":"mkdir","path":"/d/sub","msg":"a folder or file with that name already exists"}"#.into(),
    ));
    f.cx.run_until_parked();
    let said = f.read(|picker| picker.message.clone());
    assert_eq!(
        said.map(|message| (message.text.to_string(), message.error)),
        Some(("An item with that name is already here.".into(), true)),
        "with no card to hold it, the refusal is said in the chooser's status line"
    );
    assert!(
        save_focused(&mut f),
        "Save As takes the keyboard back once the create answers"
    );
    open_card(&mut f);
    assert!(
        card_field(&mut f).is_some(),
        "New Folder… opens again once the create answered"
    );
    assert_eq!(
        card_error(&mut f),
        "",
        "a card opened afterwards starts with no refusal"
    );
}

#[gpui::test]
fn picker_new_folder_card_holds_the_keyboard(cx: &mut TestAppContext) {
    let mut f = seeded(cx);
    open_card(&mut f);
    let cursor = f.read(|picker| picker.listing.cursor);
    f.cx.simulate_keystrokes("down ctrl-l");
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        cursor,
        "the list does not move under the card"
    );
    let path = f.read(|picker| picker.browse.path.clone());
    let editing = f.cx.update(|_, cx| path.read(cx).path().read(cx).editing());
    assert!(!editing, "the path field stays closed under the card");
}
