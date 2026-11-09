//! The Open With flyout's grouping and the file menu's Tags flyout, as FINDER-BEHAVIOR.md section 8 and the boards draw them.
use super::build::listing_entries;
use super::entry::Text;
use super::flyouts::open_with_entries;
use super::rows::OPEN_WITH_OTHER;
use super::testkit::*;

#[test]
fn open_with_separates_the_default_and_sorts_the_rest() {
    use super::build::App;
    let apps = vec![
        App {
            id: "omawrite.desktop".into(),
            label: "Omawrite".into(),
            icon: "omawrite".into(),
            default: false,
        },
        App {
            id: "nvim.desktop".into(),
            label: "Neovim".into(),
            icon: "nvim".into(),
            default: true,
        },
        App {
            id: "writer.desktop".into(),
            label: "LibreOffice Writer".into(),
            icon: "writer".into(),
            default: false,
        },
    ];
    let rows = open_with_entries(&apps, true);
    let shape: Vec<&str> = rows
        .iter()
        .map(|row| {
            if row.separator {
                "---"
            } else {
                row.id.as_ref()
            }
        })
        .collect();
    assert_eq!(
        shape,
        vec![
            "nvim.desktop",
            "---",
            "writer.desktop",
            "omawrite.desktop",
            "---",
            OPEN_WITH_OTHER
        ],
        "default alone, then the rest alphabetical, then Another Application"
    );
    assert_eq!(rows[0].hint, Some(Text::Key("menu.open_with.default")));
}

#[test]
fn the_file_menu_offers_tags_with_the_seven_tag_flyout() {
    let mut p = state();
    p.hidden.clear();
    let rows = listing_entries(&p);
    let tags = entry(&rows, "tags").expect("the file menu offers Tags");
    assert_eq!(label(&english(), Some(tags)), "Tags");
    assert_eq!(tags.glyph, Some("tag"));
    let leaves: Vec<&str> = tags
        .submenu
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .filter(|leaf| !leaf.separator)
        .map(|leaf| leaf.id.as_ref())
        .collect();
    assert_eq!(
        leaves,
        vec!["tag1", "tag2", "tag3", "tag4", "tag5", "tag6", "tag7"]
    );
}

#[test]
fn open_with_captions_only_the_lifted_default() {
    use super::build::App;
    let apps = vec![
        App {
            id: "b.desktop".into(),
            label: "Beta".into(),
            icon: "b".into(),
            default: true,
        },
        App {
            id: "a.desktop".into(),
            label: "Alpha".into(),
            icon: "a".into(),
            default: true,
        },
    ];
    let rows = open_with_entries(&apps, true);
    let captioned: Vec<&str> = rows
        .iter()
        .filter(|row| row.hint == Some(Text::Key("menu.open_with.default")))
        .map(|row| row.id.as_ref())
        .collect();
    assert_eq!(
        captioned,
        vec!["b.desktop"],
        "one default caption, on the row above the separator"
    );
}
