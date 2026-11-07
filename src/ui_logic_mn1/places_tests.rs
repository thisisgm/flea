//! tests/js/placemenu.js and tests/js/background.js's ground test.
use super::build::{listing_entries, Context, RailEntry};
use super::places::*;
use super::testkit::*;

#[test]
fn place_rows() {
    let i18n = english();
    check(
        "with the switch off the rail's own menu is the one it has today",
        place_menu(Some(&strings(&["placeMenu"])), false).len(),
        0,
    );
    check(
        "and a list it could not read switches nothing on",
        place_menu(None, false).len(),
        0,
    );
    check(
        "a Places row offers the path rows and nothing that cuts, sends or destroys",
        labels(&i18n, &place_menu(Some(&[]), false)),
        "Open|Open in New Tab|Open in Terminal|Copy Path|Add to Favorites".into(),
    );
    check("and a Favorites row ends on Remove rather than Add, so issue 138's duplicate is impossible", labels(&i18n, &place_menu(Some(&[]), true)),
        "Open|Open in New Tab|Open in Terminal|Copy Path|Remove from Favorites".into());
    let rows = place_menu(Some(&[]), false);
    let copy = entry(&rows, "copypath").unwrap();
    check(
        "a place's Copy path is one flat row that copies at once, as in 0.3.7",
        format!(
            "{}|{}|{}|{}",
            copy.id,
            label(&i18n, Some(copy)),
            copy.glyph.unwrap(),
            copy.submenu.is_none()
        ),
        "copypath|Copy Path|file-text|true".into(),
    );
    check(
        "and no place row opens a flyout",
        rows.iter().filter(|r| r.submenu.is_some()).count(),
        0,
    );
    check(
        "the one Copy as switch hides a place's Copy path",
        labels(&i18n, &place_menu(Some(&strings(&["copyAs"])), false)),
        "Open|Open in New Tab|Open in Terminal|Add to Favorites".into(),
    );
    check(
        "and the file menu keeps the Copy as flyout beside it",
        labels(
            &i18n,
            &listing_entries(&Context {
                hidden: Vec::new(),
                row_mode: 0o100644,
                ..Context::default()
            }),
        )
        .contains("Copy As"),
        true,
    );
    check(
        "the key carries the path, because the rail rebuilds under an open menu",
        place_key("/home/gm/Work", Some(2)),
        "place:2:/home/gm/Work".into(),
    );
    check(
        "and a Places row carries no favourite index",
        place_key("/home/gm/Downloads", None),
        "place:-1:/home/gm/Downloads".into(),
    );
    let key = "place:-1:/home/gm/Downloads";
    check(
        "Open opens the path the row named",
        place_choice("open", key),
        PlaceChoice::Open("/home/gm/Downloads".into()),
    );
    check(
        "and so does Open in terminal",
        place_choice("openTerminal", key),
        PlaceChoice::Terminal("/home/gm/Downloads".into()),
    );
    check(
        "Copy path still acts on the row's own path",
        place_choice("copypath", key),
        PlaceChoice::CopyPath("/home/gm/Downloads".into()),
    );
    check(
        "Copy path copies that path",
        place_choice("copypath", key),
        PlaceChoice::CopyPath("/home/gm/Downloads".into()),
    );
    check(
        "and a Copy as leaf is no place action, since a place draws no flyout",
        place_choice("copyAs:copyQuoted", key),
        PlaceChoice::Ignore,
    );
    check(
        "Add to Favorites saves the path under its own leaf",
        place_choice("addFavourite", key),
        PlaceChoice::AddFavourite {
            path: "/home/gm/Downloads".into(),
            label: "Downloads".into(),
        },
    );
    check(
        "New tab opens the path the row named",
        place_choice("openTab", key),
        PlaceChoice::OpenTab("/home/gm/Downloads".into()),
    );
    let records = vec![Favourite {
        label: "Work".into(),
        path: "/home/gm/Work".into(),
    }];
    check(
        "Remove takes the row whose path the menu was opened on",
        removal_holds(
            &records,
            &place_choice("removeFavourite", "place:0:/home/gm/Work"),
        ),
        Some(0),
    );
    check(
        "and a row that moved under the menu is said out loud rather than removed",
        (
            removal_holds(
                &records,
                &place_choice("removeFavourite", "place:0:/home/gm/Elsewhere"),
            ),
            i18n.tr(FAVOURITES_CHANGED),
        ),
        (
            None,
            "Favorites changed; reopen the menu before removing this row.".into(),
        ),
    );
    check(
        "the rail's own Remove still removes by index under its identity check",
        removal_holds(
            &records,
            &place_choice(
                "removeFavourite",
                "favourite:0:{\"label\":\"Work\",\"path\":\"/home/gm/Work\"}",
            ),
        ),
        Some(0),
    );
    check(
        "and refuses when the record it was opened on is not there any more",
        removal_holds(
            &records,
            &place_choice(
                "removeFavourite",
                "favourite:0:{\"label\":\"Moved\",\"path\":\"/home/gm/Work\"}",
            ),
        ),
        None,
    );
}

#[test]
fn place_open_and_middle_click() {
    let i18n = english();
    let idle = PaneBusy::default();
    check(
        "Open starts the listing the row named",
        route_background(&idle, "/home/gm", "/home/gm/Downloads"),
        Background::Navigate("/home/gm/Downloads".into()),
    );
    check(
        "Open on the folder already shown starts no listing",
        route_background(&idle, "/home/gm/Downloads/", "/home/gm/Downloads"),
        Background::Opened,
    );
    let busy = PaneBusy {
        list_in_flight: true,
        ..PaneBusy::default()
    };
    check(
        "Open while a listing is out starts nothing new",
        route_background(&busy, "/home/gm", "/home/gm/Downloads"),
        Background::Refused("loading"),
    );
    check(
        "it says the listing is still loading",
        i18n.tr("pane.busy"),
        "A directory is already loading.".into(),
    );
    check(
        "a deferred menu opens only on a ready or empty listing of its target",
        (
            should_open("/d", "/d/", "ready"),
            should_open("/d", "/d", "empty"),
            should_open("/d", "/d", "error"),
            should_open("/e", "/d", "ready"),
        ),
        (true, true, false, false),
    );
    check(
        "a middle click on the Trash opens nothing",
        place_tab("trash", "Trash", ""),
        PlaceTab::Nothing,
    );
    check(
        "and a remote favourite says it opens in this tab only",
        place_tab("favourite", "NAS", "smb://nas/data"),
        PlaceTab::Remote("NAS".into()),
    );
    check(
        "its sentence names the favourite",
        i18n.tr_args(OPENS_HERE, &["NAS"]),
        "NAS opens in this tab only".into(),
    );
    check(
        "a middle click on a home row opens its folder in a new tab",
        place_tab("home", "Downloads", "/home/gm/Downloads"),
        PlaceTab::Open("/home/gm/Downloads".into()),
    );
    check(
        "a file:// favourite opens the decoded path in a new tab",
        place_tab("favourite", "My Docs", "file:///home/gm/My%20Docs"),
        PlaceTab::Open("/home/gm/My Docs".into()),
    );
    let mount = RailEntry {
        group: "device".into(),
        kind: "volume".into(),
        mounted: true,
        ..RailEntry::default()
    };
    check(
        "the Trash row opens the Trash menu",
        matches!(
            rail_menu_for("trash", "", None, "", 3, Some(&[]), &mount),
            RailMenu::Trash(_)
        ),
        true,
    );
    check(
        "a favourite with the Places menu off keeps its legacy key",
        matches!(
            rail_menu_for(
                "favourite",
                "/w",
                Some(1),
                "{}",
                0,
                Some(&strings(&["placeMenu"])),
                &mount
            ),
            RailMenu::LegacyFavourite { .. }
        ),
        true,
    );
    check(
        "a mount row opens its own rows",
        matches!(
            rail_menu_for("volume", "", None, "", 0, None, &mount),
            RailMenu::Mount(_)
        ),
        true,
    );
}

#[test]
fn right_click_ground() {
    // 30 rows of 20 under a 100 viewport scrolled to its end, with one row of bare ground past the last.
    check(
        "unscrolled, a right click on the first row is on a row",
        on_background(10., 0., 20., 30),
        false,
    );
    check(
        "scrolled, a right click on the last row is on a row",
        on_background(70., 520., 20., 30),
        false,
    );
    check(
        "scrolled, a right click on the top row in view is on a row",
        on_background(10., 520., 20., 30),
        false,
    );
    check(
        "scrolled, the bare ground under the last row is still the background",
        on_background(90., 520., 20., 30),
        true,
    );
    check(
        "Trash: scrolled, the bare ground under the last row is still the background",
        on_background(90., 520., 20., 30),
        true,
    );
}
