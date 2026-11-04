//! Open With and Permissions board states as plain data: the strings, the focused control and the requests every press sends.
use super::keys::Key;
use super::openwith::{OpenWith, Stop as OwStop};
use super::permissions::Permissions;
use super::shell::{AppRow, Block, Card, Cursor, Ink, Tri};
use super::Effect;
use flea::i18n::{I18n, Paths};
use flea::jsondoc::parse;

pub(super) fn language(file: &str) -> I18n {
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/ui_logic_op3/fixtures");
    I18n::load(Paths {
        omarchy: fixtures.clone(),
        language_file: fixtures.join(file),
        catalogs: Vec::new(),
    })
}

pub(crate) fn english() -> I18n {
    language("/nonexistent/language")
}

pub(super) fn buttons(card: &Card) -> String {
    card.buttons
        .iter()
        .map(|b| {
            format!(
                "{}{}{}{}",
                b.label,
                if b.primary { "*" } else { "" },
                if b.focused { "@" } else { "" },
                if b.enabled { "" } else { "~" }
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

pub(super) fn notes(card: &Card) -> Vec<(String, Ink)> {
    card.body
        .iter()
        .filter_map(|b| match b {
            Block::Note { text, ink, .. } => Some((text.clone(), *ink)),
            _ => None,
        })
        .collect()
}

pub(super) fn sent(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Send(line) => Some(line.clone()),
            _ => None,
        })
        .collect()
}

pub(super) const APPLICATIONS: &str = r#"{"t":"menuaction","id":4,"op":"applications","ok":true,"path":"/fx/v/work/notes.txt","kind":"Plain text document","mime":"text/plain","applications":[{"id":"nvim.desktop","label":"Neovim","icon":"nvim","default":true}],"installed":[{"id":"xfce4-about.desktop","label":"About Xfce","icon":"org.xfce.about","default":false},{"id":"bssh.desktop","label":"Avahi SSH Server Browser","icon":"network-wired","default":false},{"id":"bvnc.desktop","label":"Avahi VNC Server Browser","icon":"network-wired","default":false},{"id":"avahi-discover.desktop","label":"Avahi Zeroconf Browser","icon":"network-wired","default":false},{"id":"basecamp.desktop","label":"Basecamp","icon":"","default":false},{"id":"btop.desktop","label":"btop++","icon":"btop","default":false},{"id":"chromium.desktop","label":"Chromium","icon":"chromium","default":false}]}"#;

pub(crate) fn open_with() -> OpenWith {
    let mut card = OpenWith::open(4, "notes.txt");
    let held =
        parse(r#"{"t":"menuaction","id":4,"op":"snapshot","ok":true,"kinds":[33188]}"#).unwrap();
    // The catalogue is asked once the snapshot is held, and a repeated answer asks nothing more.
    assert_eq!(
        sent(&card.reply(&english(), &held)),
        vec![r#"{"c":"menuaction","op":"applications","id":4,"installed":true}"#]
    );
    assert!(card.reply(&english(), &held).is_empty());
    card.reply(&english(), &parse(APPLICATIONS).unwrap());
    card
}

fn apps(card: &Card) -> Vec<AppRow> {
    card.body
        .iter()
        .find_map(|b| match b {
            Block::Apps(rows, _) => Some(rows.clone()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn open_with_opens_on_the_search_with_the_default_waiting_grey() {
    let i18n = english();
    let card = open_with().card(&i18n);
    assert_eq!(card.title, "Open notes.txt With");
    assert_eq!(card.esc, "esc");
    assert_eq!(card.width, 630.);
    let Block::Field(search) = &card.body[0] else {
        panic!("the search leads")
    };
    assert!(
        search.focused && search.value.is_empty() && search.placeholder == "Search applications"
    );
    let rows = apps(&card);
    assert_eq!(
        rows[0],
        AppRow::Eyebrow {
            text: "Registered for Plain text document".into(),
            first: true
        }
    );
    assert_eq!(
        rows[1],
        AppRow::App {
            at: 0,
            icon: "nvim".into(),
            label: "Neovim".into(),
            tail: "default".into(),
            cursor: Cursor::Waiting
        }
    );
    assert_eq!(
        rows[2],
        AppRow::Eyebrow {
            text: "All applications".into(),
            first: false
        }
    );
    assert_eq!(
        rows.iter()
            .filter(|r| matches!(r, AppRow::App { .. }))
            .count(),
        7,
        "seven applications fill the viewport"
    );
    assert_eq!(
        rows[rows.len() - 2],
        AppRow::App {
            at: 5,
            icon: "".into(),
            label: "Basecamp".into(),
            tail: String::new(),
            cursor: Cursor::None
        },
        "an entry with no themed icon draws the board's window mark"
    );
    assert_eq!(
        rows.last(),
        Some(&AppRow::App {
            at: 6,
            icon: "btop".into(),
            label: "btop++".into(),
            tail: String::new(),
            cursor: Cursor::None
        })
    );
    assert!(card.body.contains(&Block::CheckLine {
        check: super::shell::Check {
            value: Tri::Off,
            focused: false
        },
        label: "Always use this application for plain text document files".into()
    }));
    assert_eq!(
        notes(&card),
        vec![(
            "Undo does not reverse this; change it here again.".into(),
            Ink::Muted
        )]
    );
    assert_eq!(buttons(&card), "Cancel|Open*");
}

#[test]
fn open_with_tab_walks_list_check_cancel_open_as_the_boards_do() {
    let i18n = english();
    let mut card = open_with();
    card.key(Key::Tab);
    assert_eq!(card.focus, OwStop::List);
    let drawn = card.card(&i18n);
    let Block::Field(search) = &drawn.body[0] else {
        panic!()
    };
    assert!(
        !search.focused,
        "openwith-list: the search lets go of its frame"
    );
    assert_eq!(
        apps(&drawn)[1],
        AppRow::App {
            at: 0,
            icon: "nvim".into(),
            label: "Neovim".into(),
            tail: "default".into(),
            cursor: Cursor::Focused
        }
    );
    card.key(Key::Tab);
    assert!(
        card.card(&i18n)
            .body
            .iter()
            .any(|b| matches!(b, Block::CheckLine { check, .. } if check.focused)),
        "openwith-check"
    );
    card.key(Key::Tab);
    assert_eq!(
        buttons(&card.card(&i18n)),
        "Cancel@|Open*",
        "openwith-cancel"
    );
    card.key(Key::Tab);
    assert_eq!(buttons(&card.card(&i18n)), "Cancel|Open*@", "openwith-open");
    assert!(
        matches!(
            apps(&card.card(&i18n))[1],
            AppRow::App {
                cursor: Cursor::Waiting,
                ..
            }
        ),
        "openwith-open: Neovim keeps the grey highlight while the list does not have focus"
    );
    card.key(Key::Tab);
    assert_eq!(card.focus, OwStop::Search, "the walk comes round");
}

#[test]
fn open_with_opens_the_cursor_app_and_closes_only_on_the_backends_yes() {
    let i18n = english();
    let mut card = open_with();
    card.key(Key::Down);
    let effects = card.key(Key::Enter);
    assert_eq!(
        sent(&effects),
        vec![
            r#"{"c":"menuaction","op":"openWith","id":4,"application":"xfce4-about.desktop","always":false}"#
        ]
    );
    assert!(
        card.key(Key::Enter).is_empty(),
        "a second Enter while opening sends nothing"
    );
    assert_eq!(buttons(&card.card(&i18n)), "Cancel|Open*~");
    assert!(card
        .reply(
            &i18n,
            &parse(r#"{"t":"menuaction","id":4,"op":"openWith","ok":false,"error":"x"}"#).unwrap()
        )
        .is_empty());
    assert!(notes(&card.card(&i18n))
        .contains(&("The application could not be opened.".into(), Ink::Error)));
    card.key(Key::Enter);
    assert_eq!(
        card.reply(
            &i18n,
            &parse(r#"{"t":"menuaction","id":4,"op":"openWith","ok":true}"#).unwrap()
        ),
        vec![Effect::Close]
    );
    let effects = card.key(Key::Escape);
    assert_eq!(
        effects,
        vec![
            Effect::Send(r#"{"c":"menuaction","op":"close","id":4}"#.into()),
            Effect::Close
        ]
    );
}

#[test]
fn open_with_search_filters_and_the_always_box_rides_the_open() {
    let i18n = english();
    let mut card = open_with();
    for c in "zz".chars() {
        card.key(Key::Text(c.to_string()));
    }
    assert!(apps(&card.card(&i18n))
        .iter()
        .any(|row| matches!(row, AppRow::Empty { text, .. } if text.starts_with("No application matches \u{201c}zz\u{201d}"))));
    card.key(Key::Backspace);
    card.key(Key::Backspace);
    card.key(Key::Text("b".into()));
    card.key(Key::Text("t".into()));
    let rows = apps(&card.card(&i18n));
    assert_eq!(
        rows.len(),
        2,
        "only the All applications eyebrow and btop++ match"
    );
    card.key(Key::Tab);
    card.key(Key::Tab);
    card.key(Key::Space);
    assert!(card.always);
    card.key(Key::Tab);
    card.key(Key::Tab);
    assert_eq!(
        sent(&card.key(Key::Enter)),
        vec![
            r#"{"c":"menuaction","op":"openWith","id":4,"application":"btop.desktop","always":true}"#
        ]
    );
}

pub(super) const INSPECT: &str = r#"{"t":"permissions","id":7,"op":"inspect","ok":true,"path":"/fx/v/work/notes.txt","directory":false,"mode":"0644","uid":1000,"gid":1000,"owner":"gm","group":"gm","reason":""}"#;

pub(crate) fn permissions() -> Permissions {
    let (mut card, effects) = Permissions::open(
        7,
        vec!["/fx/v/work/notes.txt".into()],
        "Plain text document",
    );
    assert_eq!(
        sent(&effects),
        vec![r#"{"c":"permissions","op":"inspect","id":7,"path":"/fx/v/work/notes.txt"}"#]
    );
    card.reply(&english(), &parse(INSPECT).unwrap());
    card
}
