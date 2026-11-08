// The AirDrop page (board 0401-airdrop-page) with the window's facts: Receiving and the folder ask the host, never the backend.
use super::page::{Key, PluginsPage};
use super::page_tests::{builtins, go, labels, opened, Recorder};
use super::seam::Effect;
use flea::ui_logic_sd1::airdrop::{Adapter, Readiness};
use flea::ui_logic_st2::airdrop::{Page, FOLDER_ID, RECEIVING_ID};
use flea::ui_logic_st2::builtin::AIRDROP;
use flea::ui_logic_st2::pages::Row;
use flea::ui_logic_st2::schema::Value;

fn ready() -> Page {
    Page {
        receiving: false,
        folder: "/home/u/Downloads".into(),
        shown_as: "minipc".into(),
        adapter: Adapter {
            readiness: Readiness::Ready,
            card: "Intel AX210".into(),
        },
    }
}

// The AirDrop page open, the plugin switched on or off, with the window's facts in.
fn page(wire: &Recorder, on: bool, folder: Option<&str>) -> PluginsPage {
    let mut plugins = builtins("{}");
    let airdrop = plugins
        .iter_mut()
        .find(|p| p.manifest.id == AIRDROP)
        .expect("AirDrop is built in");
    airdrop.enabled = on;
    if let Some(folder) = folder {
        airdrop.settings = vec![("folder".into(), Value::Text(folder.into()))];
    }
    let mut page = opened(wire, plugins);
    go(&mut page, wire, AIRDROP);
    page.key(Key::Enter, wire);
    assert_eq!(page.title(), "AirDrop");
    page
}

#[test]
fn only_the_airdrop_page_sets_its_fact_values_at_the_right_edge() {
    let wire = Recorder::default();
    assert!(page(&wire, true, None).facts_right());
    let mut shelf = opened(&wire, builtins("{}"));
    go(&mut shelf, &wire, "flea.shelf");
    shelf.key(Key::Enter, &wire);
    assert_eq!(shelf.title(), "Shelf");
    assert!(!shelf.facts_right());
}

fn receiving_available(page: &PluginsPage) -> bool {
    page.rows()
        .iter()
        .any(|r| matches!(r, Row::Choice { id, available: true, .. } if id == RECEIVING_ID))
}

#[test]
fn the_page_is_the_board_once_the_window_says_what_it_knows() {
    let wire = Recorder::default();
    let mut page = page(&wire, true, None);
    assert!(
        labels(&page).contains(&"Wi-Fi adapter: Checking…".to_string()),
        "{:?}",
        labels(&page)
    );
    assert!(
        !receiving_available(&page),
        "nothing turns on before doctor answers"
    );
    assert!(page.set_airdrop(ready()));
    assert!(!page.set_airdrop(ready()), "the same facts change nothing");
    assert_eq!(
        labels(&page),
        [
            "~Share files with iPhones, iPads and Macs nearby.",
            "#AirDrop",
            "Receiving",
            "Save received files to",
            "#This Computer",
            "Shown as: minipc",
            "Wi-Fi adapter: Intel AX210, works with AirDrop",
            "#Access",
            "Reads the files you send",
            "Uses Wi-Fi to reach Apple devices nearby",
            "Saves the files you accept to the folder above",
            "#Credits",
            "Built on owly by Tristan (@t4t5): MIT",
            "--",
            "Back to Plugins",
        ]
    );
    assert!(receiving_available(&page));
    let folder = page.rows().iter().find_map(|r| match r {
        Row::Action {
            id,
            value,
            chevron: true,
            ..
        } if id == FOLDER_ID => Some(value.clone()),
        _ => None,
    });
    assert_eq!(folder.as_deref(), Some("Downloads"));
}

#[test]
fn receiving_asks_the_host_and_sends_nothing_to_the_backend() {
    let wire = Recorder::default();
    let mut page = page(&wire, true, None);
    page.set_airdrop(ready());
    go(&mut page, &wire, RECEIVING_ID);
    let sent = wire.lines.borrow().len();
    page.key(Key::Right, &wire);
    assert_eq!(
        page.take_effect(),
        Effect::AirdropReceiving(true, "/home/u/Downloads".into())
    );
    assert_eq!(page.take_effect(), Effect::None, "asked once");
    assert_eq!(
        wire.lines.borrow().len(),
        sent,
        "Receiving is the window's radio, not a saved setting"
    );
    page.set_airdrop(Page {
        receiving: true,
        ..ready()
    });
    page.key(Key::Left, &wire);
    assert_eq!(
        page.take_effect(),
        Effect::AirdropReceiving(false, "/home/u/Downloads".into())
    );
}

#[test]
fn an_adapter_that_cannot_or_a_plugin_that_is_off_greys_receiving() {
    let wire = Recorder::default();
    let mut page = page(&wire, true, None);
    page.set_airdrop(Page {
        adapter: Adapter {
            readiness: Readiness::Broken,
            card: "Intel 8265".into(),
        },
        ..ready()
    });
    assert!(labels(&page).contains(&"Wi-Fi adapter: Intel 8265, cannot do AirDrop".to_string()));
    assert!(!receiving_available(&page));
    press_greyed_receiving(&mut page, &wire);
    let mut off = super::page_tests::opened(&wire, builtins("{}"));
    go(&mut off, &wire, AIRDROP);
    off.key(Key::Enter, &wire);
    off.set_airdrop(ready());
    assert!(!receiving_available(&off), "AirDrop ships off");
    press_greyed_receiving(&mut off, &wire);
}

// A greyed Receiving takes the key and the chip click and asks the host for nothing.
fn press_greyed_receiving(page: &mut PluginsPage, wire: &Recorder) {
    go(page, wire, RECEIVING_ID);
    let index = page.cursor().expect("the cursor is on Receiving");
    page.key(Key::Right, wire);
    assert_eq!(
        page.take_effect(),
        Effect::None,
        "a greyed key asks nothing"
    );
    page.pick(index, "everyone", wire);
    assert_eq!(
        page.take_effect(),
        Effect::None,
        "a greyed chip asks nothing"
    );
}

#[test]
fn the_folder_row_opens_the_chooser_where_the_saved_folder_is_and_a_choice_is_saved() {
    let wire = Recorder::default();
    let mut page = page(&wire, true, Some("/mnt/nas/in"));
    page.set_airdrop(ready());
    go(&mut page, &wire, FOLDER_ID);
    assert_eq!(
        page.key(Key::Enter, &wire),
        Effect::AirdropFolder("/mnt/nas/in".into())
    );
    page.set_airdrop_folder("/home/u/Inbox", &wire);
    let line = wire.last();
    for part in [
        r#""op":"set""#,
        r#""plugin":"flea.airdrop""#,
        r#""key":"folder""#,
        r#""value":"/home/u/Inbox""#,
    ] {
        assert!(line.contains(part), "{line}");
    }
}
