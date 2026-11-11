// Ported from tests/js/tags.js and tests/js/tagskeys.js; the QML source pins became these typed checks.
use super::tags::*;
use super::testing::english;
use crate::keys::{self, Context, Frontend, Input, Match, PRESETS, SHIFT};

fn files(sets: &[&[&'static str]]) -> Vec<Vec<&'static str>> {
    sets.iter().map(|s| s.to_vec()).collect()
}

#[test]
fn states_read_the_selection() {
    assert_eq!(
        NAMES.join(","),
        "Red,Orange,Yellow,Green,Blue,Purple,Gray",
        "seven names in menu order"
    );
    assert_eq!(
        state_for(&files(&[&["Red"], &["Red", "Blue"]]), "Red"),
        State::Applied,
        "a tag on every file is applied"
    );
    assert_eq!(
        state_for(&files(&[&["Red"], &["Blue"]]), "Red"),
        State::Mixed,
        "a tag on some files is mixed"
    );
    assert_eq!(
        state_for(&files(&[&["Red"], &["Blue"]]), "Green"),
        State::Absent,
        "a tag on no file is absent"
    );
    assert_eq!(
        state_for::<&str>(&[], "Red"),
        State::Absent,
        "an empty selection is absent, not applied"
    );
    let all = states_for(&files(&[&["Red"], &["Blue"]]));
    assert_eq!(
        (all[0], all[3]),
        (State::Mixed, State::Absent),
        "statesFor reports every name"
    );
    let read = states_from(&["Red"], &["Blue"]);
    assert_eq!(
        (read[0], read[4], read[3]),
        (State::Applied, State::Mixed, State::Absent),
        "a backend read gives each name its state"
    );
    assert_eq!(
        (key_for("Gray"), key_for("Pink")),
        ("7", ""),
        "a digit per name, none outside the seven"
    );
}

#[test]
fn status_lines_count_and_hint_undo() {
    let c = english();
    let p = "default";
    assert_eq!(
        done_line(&c, p, Op::Add, 1, "Red", 0),
        "Tagged 1 item Red \u{b7} z undoes",
        "one item is singular"
    );
    assert_eq!(
        done_line(&c, p, Op::Add, 3, "Red", 0),
        "Tagged 3 items Red \u{b7} z undoes",
        "three items are plural"
    );
    assert_eq!(
        done_line(&c, p, Op::Clear, 1, "", 0),
        "Removed all tags from 1 item \u{b7} z undoes",
        "clearing one item"
    );
    assert_eq!(
        done_line(&c, p, Op::Clear, 3, "", 0),
        "Removed all tags from 3 items \u{b7} z undoes",
        "clearing three items"
    );
    assert_eq!(
        done_line(&c, p, Op::Remove, 1, "Red", 0),
        "Removed Red from 1 item \u{b7} z undoes",
        "removing from one item"
    );
    assert_eq!(
        done_line(&c, p, Op::Remove, 3, "Red", 0),
        "Removed Red from 3 items \u{b7} z undoes",
        "removing from three items"
    );
    assert_eq!(
        done_line(&c, p, Op::from_wire("add"), 2, "Blue", 0),
        "Tagged 2 items Blue \u{b7} z undoes",
        "an add answer says Tagged"
    );
    assert_eq!(
        done_line(&c, p, Op::from_wire("remove"), 2, "Blue", 0),
        "Removed Blue from 2 items \u{b7} z undoes",
        "a remove answer says Removed"
    );
    assert_eq!(
        skipped_line(&c, 1),
        "1 item skipped",
        "one skipped row counts one item"
    );
    assert_eq!(
        skipped_line(&c, 2),
        "2 items skipped",
        "many skipped rows count items"
    );
    assert_eq!(
        done_line(&c, p, Op::Add, 2, "Blue", 1),
        "Tagged 2 items Blue \u{b7} 1 item skipped \u{b7} z undoes",
        "the skip sits ahead of the undo hint"
    );
    assert_eq!(
        done_line(&c, p, Op::Add, 3, "Red", 2),
        "Tagged 3 items Red \u{b7} 2 items skipped \u{b7} z undoes",
        "two skips count items"
    );
    assert_eq!(
        done_line(&c, p, Op::Add, 0, "Blue", 1),
        "1 item skipped",
        "a skip with no change says only the skip"
    );
    assert_eq!(
        done_line(&c, p, Op::Remove, 2, "Blue", 0),
        "Removed Blue from 2 items \u{b7} z undoes",
        "a change with no skip says only the change"
    );
    assert_eq!(
        done_line(&c, p, Op::Add, 0, "Red", 0),
        "",
        "no change and no skip says nothing"
    );
    assert_eq!(
        done_line(&c, p, Op::Clear, 0, "", 0),
        "",
        "a no-op clear prints no z line"
    );
    assert_eq!(
        done_line(&c, p, Op::Clear, 2, "", 0),
        "Removed all tags from 2 items \u{b7} z undoes",
        "a clear that changed rows prints the z line"
    );
}

#[test]
fn the_flyout_lists_seven_swatches_and_clears_only_what_is_set() {
    let absent = [State::Absent; 7];
    assert_eq!(
        flyout(&absent, false).len(),
        7,
        "Remove All Tags hides while the target carries no tag (tags-menu board)"
    );
    let mut red = absent;
    red[0] = State::Mixed;
    let rows = flyout(&red, false);
    let shape: Vec<String> = rows
        .iter()
        .map(|row| match row {
            FlyoutRow::Tag(tag) => tag.name.to_owned(),
            FlyoutRow::Separator => "sep".into(),
            FlyoutRow::Clear => "__clear__".into(),
        })
        .collect();
    assert_eq!(
        shape.join(","),
        "Red,Orange,Yellow,Green,Blue,Purple,Gray,sep,__clear__",
        "seven tags, a separator and remove-all"
    );
    let tags: Vec<TagRow> = rows
        .iter()
        .filter_map(|r| {
            if let FlyoutRow::Tag(t) = r {
                Some(*t)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        tags.iter().map(|t| t.digit).collect::<String>(),
        "1234567",
        "each tag carries its digit hint"
    );
    assert_eq!(
        tags[0].state,
        State::Mixed,
        "the flyout carries the mixed state"
    );
    assert_eq!(
        (tags[0].name, tags[0].label_key),
        ("Red", "places.tag.red"),
        "swatches keyed by name, label by catalog"
    );
    assert!(
        flyout(&red, true)
            .iter()
            .all(|r| !matches!(r, FlyoutRow::Tag(t) if !t.fallback)),
        "the fallback flag reaches every row"
    );
    let states = states_for(&files(&[&["Red", "Blue"], &["Red"]]));
    assert_eq!(
        (states[0], states[4]),
        (State::Applied, State::Mixed),
        "the flyout carries each state"
    );
}

#[test]
fn stamp_latches_the_tag_slot_per_folder() {
    let mut used = [false; 7];
    let window = vec![Some(vec!["Red", "Blue"]), None];
    let seen = stamp(&window, false, &mut used, true);
    assert!(
        seen.latched,
        "a tagged row latches the slot for every row of the reply"
    );
    assert_eq!(
        rail_tags(&used).collect::<Vec<_>>(),
        ["Red", "Blue"],
        "the tags seen are recorded in menu order"
    );
    assert!(seen.grew, "a first sighting reports growth");
    let later: Vec<Option<Vec<&str>>> = vec![None];
    assert!(
        stamp(&later, true, &mut used, false).latched,
        "the slot stays for a later reply of the same folder"
    );
    assert!(
        !stamp(&later, true, &mut used, true).latched,
        "a fresh listing without a tagged row drops the slot"
    );
    assert!(
        !stamp(&later, false, &mut [false; 7], false).latched,
        "a folder with no tagged row never draws the slot"
    );
    assert!(
        !stamp(&[Some(vec!["Red"])], false, &mut used, false).grew,
        "a tag already seen reports no growth"
    );
    let mut gray = [false; 7];
    gray[6] = true;
    assert_eq!(
        rail_tags(&gray).map(key_for).collect::<Vec<_>>(),
        ["7"],
        "rail rows carry their digit"
    );
}

#[test]
fn swatch_geometry_is_the_boards() {
    assert_eq!(
        (
            SWATCH_SIZE,
            SWATCH_GAP,
            FALLBACK_BORDER,
            PAIR_SWATCH_SIZE,
            DIGIT_TEXT,
            ROW_SLOT
        ),
        (14.0, 3.0, 2.0, 10.0, 10.0, 97.0)
    );
    assert_eq!(
        swatches_that_fit(7, ROW_SLOT),
        5,
        "the 97 px row slot holds five whole swatches, no counter"
    );
    assert_eq!(
        swatches_that_fit(3, ROW_SLOT),
        3,
        "notes.txt's three tags all draw"
    );
    assert_eq!(swatches_that_fit(2, 88.0), 2);
    assert_eq!(swatches_that_fit(3, 0.0), 0);
}

#[test]
fn keys_open_the_flyout_and_toggle_tags() {
    let input = |key: &'static str, modifiers| Input {
        key,
        text: key,
        modifiers,
    };
    for preset in PRESETS {
        assert_eq!(
            keys::lookup(preset, Context::Listing, Frontend::Gui, input("T", SHIFT)),
            Match::Action("tags"),
            "{preset} T opens Tags"
        );
        for (at, digit) in ["1", "2", "3", "4", "5", "6", "7"].into_iter().enumerate() {
            let want = format!("tag{}", at + 1);
            assert_eq!(
                keys::lookup(preset, Context::Menu, Frontend::Gui, input(digit, 0)).action(),
                want,
                "{preset} menu {digit}"
            );
        }
        assert_eq!(
            keys::lookup(preset, Context::Menu, Frontend::Gui, input("8", 0)).action(),
            "",
            "{preset} 8 answers nothing"
        );
    }
}

#[test]
fn the_skip_fragment_rides_only_on_a_z_undo_hint_as_v0312_done_line() {
    // v0.3.12 Tags.js doneLine inserts the skip only by rewriting a trailing " · z undoes".
    let c = english();
    assert_eq!(
        done_line(&c, "default", Op::Add, 2, "Blue", 1),
        "Tagged 2 items Blue \u{b7} 1 item skipped \u{b7} z undoes",
        "a z undo key carries the skip ahead of it"
    );
    assert_eq!(
        done_line(&c, "vim", Op::Add, 2, "Blue", 1),
        "Tagged 2 items Blue \u{b7} u undoes",
        "another undo key shows no skip"
    );
    assert_eq!(
        done_line_hinted(&c, "", Op::Add, 2, "Blue", 1),
        "Tagged 2 items Blue",
        "no undo key shows no skip"
    );
    assert_eq!(
        done_line_hinted(&c, "", Op::Add, 0, "Blue", 1),
        "1 item skipped",
        "a skip with no change still says only the skip"
    );
}

// In right-to-left text the undo hint carries an isolated z, and the skip still rides on it.
#[test]
fn i50_keycap_rtl_skip_still_rides_on_the_isolated_z_undo_hint() {
    let fa = crate::keys::presentation::keycap_tests::language("fa");
    let hint = undo_hint(&fa, "default");
    assert_eq!(hint, " · \u{2066}z\u{2069} واگرد می‌کند");
    let line = done_line(&fa, "default", Op::Add, 2, "Blue", 1);
    assert!(line.contains(&skipped_line(&fa, 1)), "{line:?}");
    assert!(line.ends_with(&hint), "{line:?}");
}
