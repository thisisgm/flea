//! tests/js/crumbs.js, case for case: the chrome's crumbs, the aliased network roots and the fit to the strip.
use super::HOME;
use flea_gui::path::crumbs::{
    alias_at, crumbs, failed_name, fit_crumbs, labels_by_key, relabel_aliases, remember_alias,
    Alias, Crumb, Place,
};

#[track_caller]
fn check<T: PartialEq + std::fmt::Debug>(line: u32, got: T, want: T) {
    super::js("crumbs.js", line, got, want);
}

fn drawn(list: &[Crumb]) -> String {
    list.iter().map(|crumb| crumb.text.as_str()).collect()
}

fn targets(list: &[Crumb]) -> String {
    list.iter()
        .map(|crumb| crumb.path.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn shown(path: &str, label: &str) -> Alias {
    Alias {
        path: path.into(),
        label: label.into(),
        key: String::new(),
    }
}

const DAV: &str =
    "/run/user/1000/gvfs/dav:host=cloud.example.com,ssl=true,prefix=%2Fdav%2Fspaces%2FSPACE-ID";
const PERSONAL: &str = "Cloud \u{2014} Personal";
const TEAM: &str = "Cloud \u{2014} Team";

#[test]
fn the_crumbs_concatenate_to_the_line_they_replace_and_name_their_directories() {
    let under = crumbs("/home/gm/Work/claude", HOME, None);
    check(21, drawn(&under), "~/Work/claude".into());
    check(
        22,
        targets(&under),
        "/home/gm /home/gm/Work /home/gm/Work/claude".into(),
    );
    let lasts: Vec<String> = under.iter().map(|crumb| crumb.last.to_string()).collect();
    check(24, lasts.join(","), "false,false,true".into());
    let at_home = crumbs(HOME, HOME, None);
    check(
        27,
        format!("{}|{}", drawn(&at_home), targets(&at_home)),
        "~|/home/gm".into(),
    );
    let outside = crumbs("/usr/share", HOME, None);
    check(
        30,
        format!("{}|{}", drawn(&outside), targets(&outside)),
        "/usr/share|/ /usr /usr/share".into(),
    );
    let root = crumbs("/", HOME, None);
    check(
        33,
        format!("{}|{}|{}", drawn(&root), targets(&root), root.len()),
        "/|/|1".into(),
    );
    let no_home = crumbs("/home/gm/Work", "", None);
    check(
        36,
        format!("{}|{}", drawn(&no_home), targets(&no_home)),
        "/home/gm/Work|/ /home /home/gm /home/gm/Work".into(),
    );
    let sibling = crumbs("/home/gmx/deep", HOME, None);
    check(
        41,
        format!("{}|{}", drawn(&sibling), targets(&sibling)),
        "/home/gmx/deep|/ /home /home/gmx /home/gmx/deep".into(),
    );
}

#[test]
fn a_saved_label_replaces_only_the_whole_mounted_root() {
    let dav = crumbs(
        &format!("{DAV}/Documents/Projects"),
        HOME,
        Some(&shown(DAV, PERSONAL)),
    );
    check(48, drawn(&dav), format!("{PERSONAL}/Documents/Projects"));
    check(
        50,
        targets(&dav),
        format!("{DAV} {DAV}/Documents {DAV}/Documents/Projects"),
    );
    let at_root = crumbs(DAV, HOME, Some(&shown(DAV, PERSONAL)));
    check(
        53,
        format!("{}|{}", drawn(&at_root), targets(&at_root)),
        format!("{PERSONAL}|{DAV}"),
    );
    let trailing = crumbs(&format!("{DAV}/"), HOME, Some(&shown(DAV, PERSONAL)));
    check(
        56,
        format!(
            "{}|{}|{}",
            drawn(&trailing),
            targets(&trailing),
            trailing[0].last
        ),
        format!("{PERSONAL}|{DAV}|true"),
    );
    let slash = crumbs(
        &format!("{DAV}/Documents"),
        HOME,
        Some(&shown(DAV, "Cloud / Personal")),
    );
    check(
        61,
        format!("{}|{}|{}", slash.len(), slash[0].text, targets(&slash)),
        format!("2|Cloud / Personal/|{DAV} {DAV}/Documents"),
    );
    let sibling = crumbs(
        &format!("{DAV}-old/Documents"),
        HOME,
        Some(&shown(DAV, PERSONAL)),
    );
    check(66, drawn(&sibling), format!("{DAV}-old/Documents"));
}

#[test]
fn the_deepest_remembered_root_labels_and_a_saved_label_outranks_a_live_row() {
    let team_root = format!("{DAV}/Team");
    let label = |path: &str, aliases: &[Alias]| alias_at(path, aliases).map(|alias| alias.label);
    let aliases = remember_alias(&[], DAV, PERSONAL, Some("davs://cloud/personal"));
    let aliases = remember_alias(
        &aliases,
        &format!("{team_root}/"),
        TEAM,
        Some("davs://cloud/team"),
    );
    check(
        72,
        label(&format!("{team_root}/Roadmap"), &aliases),
        Some(TEAM.into()),
    );
    let team_first = remember_alias(&[], &team_root, TEAM, Some("davs://cloud/team"));
    let deep_first = remember_alias(&team_first, DAV, PERSONAL, Some("davs://cloud/personal"));
    check(
        76,
        label(&format!("{team_root}/Roadmap"), &deep_first),
        Some(TEAM.into()),
    );
    check(
        78,
        label(&format!("{DAV}/Documents"), &aliases),
        Some(PERSONAL.into()),
    );
    check(80, label(&format!("{DAV}-old/Documents"), &aliases), None);
    let moved = remember_alias(
        &aliases,
        &format!("{DAV}/Personal"),
        "Personal files",
        Some("davs://cloud/personal"),
    );
    check(83, label(&format!("{DAV}/Documents"), &moved), None);
    check(
        85,
        label(&format!("{DAV}/Personal/Docs"), &moved),
        Some("Personal files".into()),
    );
    let places = [
        Place {
            uri: "",
            path: "davs://cloud/personal/",
            label: "Cloud \u{2014} Private",
            saved: true,
        },
        Place {
            uri: "davs://cloud/personal/",
            path: "",
            label: "Personal on cloud.example.com",
            saved: false,
        },
    ];
    let labels = labels_by_key(&places, |uri| {
        uri.strip_suffix('/').unwrap_or(uri).to_owned()
    });
    let renamed = relabel_aliases(&moved, &labels);
    check(
        93,
        label(&format!("{DAV}/Personal/Docs"), &renamed),
        Some("Cloud \u{2014} Private".into()),
    );
    check(
        95,
        label(&format!("{team_root}/Roadmap"), &renamed),
        Some(TEAM.into()),
    );
}

#[test]
fn a_strip_too_narrow_collapses_whole_crumbs_and_elides_only_the_leaf() {
    let deep = crumbs("/home/gm/one/two/three/four/five", HOME, None);
    check(
        100,
        drawn(&fit_crumbs(deep.clone(), 80)),
        "~/one/two/three/four/five".into(),
    );
    check(
        101,
        drawn(&fit_crumbs(deep.clone(), 16)),
        "~/\u{2026}/four/five".into(),
    );
    let marker = &fit_crumbs(deep.clone(), 16)[1];
    check(
        104,
        format!("{}{}", marker.path, marker.elided),
        "true".into(),
    );
    check(
        106,
        drawn(&fit_crumbs(deep, 24)),
        "~/one/two/\u{2026}/four/five".into(),
    );
    check(
        109,
        drawn(&fit_crumbs(crumbs("/home/gm/one/two", HOME, None), 4)),
        "~/one/two".into(),
    );
    let long = crumbs(
        "/home/gm/a-directory-with-a-deliberately-long-name",
        HOME,
        None,
    );
    check(
        114,
        drawn(&fit_crumbs(long.clone(), 20)),
        "~/a-directo\u{2026}ong-name".into(),
    );
    check(
        116,
        fit_crumbs(long, 20)[1].path.clone(),
        "/home/gm/a-directory-with-a-deliberately-long-name".into(),
    );
    let collapsed = crumbs("/home/gm/one/two/three/a-very-long-leaf-name", HOME, None);
    check(
        118,
        drawn(&fit_crumbs(collapsed, 18)),
        "~/\u{2026}/three/a-ve\u{2026}ame".into(),
    );
    let four = crumbs("/home/gm/one/two/three/four", HOME, None);
    check(
        121,
        drawn(&fit_crumbs(four.clone(), 14)),
        "~/\u{2026}/three/four".into(),
    );
    let wide = crumbs(
        "/home/gm/one/two/a-parent-with-a-very-long-name-indeed/leaf-name",
        HOME,
        None,
    );
    check(
        126,
        drawn(&fit_crumbs(wide, 16)),
        "~/\u{2026}/leaf-name".into(),
    );
    check(129, drawn(&fit_crumbs(four, 6)), "~/\u{2026}/four".into());
}

// Crumb.qml cursorShape keys on parent; this pins only Crumb::parent(), the predicate paint_element maps to the pointing hand.
#[test]
fn noscene_parent_is_true_only_for_a_real_parent_crumb() {
    let whole = crumbs("/home/gm/one/two/three/four/five", HOME, None);
    let leaf = whole.last().unwrap();
    assert!(leaf.last && !leaf.parent(), "the leaf is no parent");
    assert!(
        whole[..whole.len() - 1].iter().all(Crumb::parent),
        "every other crumb of a whole path is a parent"
    );
    let fitted = fit_crumbs(whole, 16);
    let marker = fitted
        .iter()
        .find(|crumb| crumb.elided)
        .expect("a narrow strip collapses into a marker");
    assert!(!marker.parent(), "the marker is no parent");
    assert!(fitted
        .iter()
        .filter(|crumb| !crumb.last && !crumb.elided)
        .all(Crumb::parent));
}

/// S8-11: a share root that cannot be read is named by its alias, the path bar's and rail's label, never its gvfs directory.
#[test]
fn s8_11_a_failing_share_root_is_named_by_its_label() {
    let root = "/run/user/1000/gvfs/dav:host=127.0.0.1,port=18767,ssl=false";
    let aliases = remember_alias(&[], root, "127.0.0.1:18767", Some("dav://127.0.0.1:18767/"));
    assert_eq!(failed_name(root, &aliases), "127.0.0.1:18767");
    assert_eq!(
        failed_name(&format!("{root}/"), &aliases),
        "127.0.0.1:18767"
    );
    assert_eq!(failed_name(&format!("{root}/photos"), &aliases), "photos");
    assert_eq!(
        failed_name(root, &[]),
        "dav:host=127.0.0.1,port=18767,ssl=false"
    );
}
