use super::*;
use crate::tui::render::{cards::tests::{at, role, screen, PALETTE}, frame_tests::{caps_language, model}};

const PRESETS: [&str; 5] = ["default", "vim", "mac", "windows", "yazi"];
// The board's sheet from column 9 of rows 5 to 26, trailing blanks trimmed.
const BOARD: [&str; 22] = [
    "MOVE                                 LOOK                              CHANGE",
    "      j / down  Cursor down                   ?  Keyboard Shortcuts               y / ctrl-c  Copy",
    "        k / up  Cursor up                 space  Quick Look                       x / ctrl-x  Cut",
    "      g / home  First item                    .  Hidden files                     p / ctrl-v  Paste",
    "       G / end  Last item                     I  Detail column                        r / f2  Rename",
    "        ctrl-d  Page down            FIND                                        dd / delete  Move to Trash",
    "        ctrl-u  Page up                       /  Filter                         shift-delete  Delete permanently",
    "         enter  Open                 f / ctrl-f  Find                             z / ctrl-z  Undo",
    "      h / left  Parent folder            ctrl-g  Search contents       ctrl-shift-z / ctrl-y  Redo",
    "             H  Back                          o  Reveal                     ctrl-shift-n / N  New folder",
    "             L  Forward                       s  Sort                                      v  Select",
    "tab / ctrl-tab  Focus the preview             S  Reverse sort                       m / menu  Menu",
    "             t  New tab              : / ctrl-l  Go to folder                         ctrl-t  Open terminal",
    "ctrl-enter / O  Open in new tab                                                            A  Open agent here",
    "    w / ctrl-w  Close tab                                                             ctrl-p  Print…",
    " ctrl-pagedown  Next tab",
    "   ctrl-pageup  Previous tab",
    "             {  Move tab left",
    "             }  Move tab right",
    "        escape  Close or go back",
    "           1–9  Select tab",
    "             q  Quit",
];
fn row(action: &str) -> &'static Row { ROWS.iter().find(|row| row.1.contains(&action)).unwrap() }

#[test]
fn keys_sheet_matches_its_board_cells_and_inks() {
    let theme = Theme::from_text(PALETTE);
    let mut m = model();
    m.sheet = true;
    let (rows, ink, _) = screen(&m, &theme);
    assert_eq!((at(&rows, 2, 6, 1), at(&rows, 2, 123, 1), at(&rows, 28, 6, 1), at(&rows, 28, 123, 1)), ("┌".into(), "┐".into(), "└".into(), "┘".into()));
    assert_eq!((at(&rows, 3, 9, 18), at(&rows, 3, 118, 3)), ("Keyboard Shortcuts".into(), "esc".into()));
    assert!(ink[3][9].2, "the title is bold");
    assert_eq!(at(&rows, 4, 9, 112), "─".repeat(112));
    for (i, line) in BOARD.iter().enumerate() {
        assert_eq!(at(&rows, 5 + i, 9, 113).trim_end(), *line, "row {}", 5 + i);
    }
    assert_eq!(at(&rows, 27, 7, 116).trim(), "", "the last row inside the frame stays blank");
    assert_eq!((ink[5][9].0.clone(), ink[10][46].0.clone()), (role(&theme.meta), role(&theme.meta)), "headings in meta");
    assert_eq!((ink[6][22].0.clone(), ink[6][25].0.clone()), (role(&theme.meta), role(&theme.foreground)), "keys in meta, words in foreground");
    assert_eq!(ink[2][6].0, role(&theme.frame));
}

#[test]
fn every_printed_key_reaches_its_action_in_every_preset() {
    let map = Map::load();
    for preset in PRESETS {
        for row in ROWS.iter().filter(|row| listed(row, preset) && row.1 != ["tab1"]) {
            let printed = spelling(&map, preset, row);
            if printed.is_empty() {
                assert!(row.1.iter().all(|action| map.chord(action, preset, context(row.1)).is_none()), "{preset} {:?} binds a key the sheet leaves out", row.1);
            }
            for spelled in printed.split(" / ").filter(|spelled| !spelled.is_empty()) {
                if spelled.contains(' ') {
                    let mut parts = spelled.split(' ');
                    let first = keys(parts.next().unwrap()).remove(0);
                    let tail = keys(parts.next().unwrap()).remove(0);
                    let action = map.action(&first, preset);
                    let prefix = action.strip_prefix("prefix:").unwrap();
                    assert!(row.1.contains(&map.sequence(prefix, &tail, preset, "listing").as_str()));
                    assert!(parts.next().is_none());
                    continue;
                }
                let first = spelled.chars().next().unwrap();
                let doubled = spelled.chars().count() == 2 && spelled.chars().all(|c| c == first);
                let probe = if doubled { first.to_string() } else { spelled.to_owned() };
                let found: Vec<String> = keys(&probe).iter().map(|key| map.in_context(key, preset, context(row.1))).collect();
                let reaches = found.iter().any(|action| row.1.iter().any(|want| if doubled { *action == format!("{want}Arm") } else { action == want || *action == format!("{want}Arm") }));
                assert!(reaches, "{preset}: {spelled} printed for {:?} resolves to {found:?}", row.1);
            }
        }
        assert_eq!(spelling(&map, preset, row("tab1")), "1–9");
    }
}

#[test]
fn a_key_that_needs_the_kitty_protocol_prints_beside_one_that_does_not() {
    let map = Map::load();
    let protocol = |spelled: &str| spelled.starts_with("ctrl-shift-") || matches!(spelled, "ctrl-enter" | "ctrl-tab");
    for preset in PRESETS {
        for (action, pair) in [("openTab", "ctrl-enter / O"), ("redo", "ctrl-shift-z / ctrl-y"), ("newFolder", "ctrl-shift-n / N"), ("focusPreview", "tab / ctrl-tab")] {
            assert_eq!(spelling(&map, preset, row(action)), pair, "{preset}");
        }
        for row in ROWS.iter().filter(|row| listed(row, preset)) {
            let printed = spelling(&map, preset, row);
            if printed.split(" / ").any(protocol) { assert!(printed.split(" / ").any(|spelled| !protocol(spelled)), "{preset} {printed}"); }
        }
        assert_eq!(spelling(&map, preset, row("deletePermanently")), "shift-delete", "{preset}: shift-delete needs no protocol");
    }
}

#[test]
fn arm_letters_print_doubled_and_the_sheet_keeps_the_view_switches_off() {
    let map = Map::load();
    assert_eq!(spelling(&map, "vim", row("copy")), "yy / ctrl-c");
    assert_eq!(spelling(&map, "vim", row("cursorFirst")), "gg / home");
    assert_eq!(spelling(&map, "default", row("trash")), "dd / delete");
    assert_eq!(spelling(&map, "yazi", row("trash")), "d / delete", "one press trashes in yazi");
    let mut m = model();
    for preset in PRESETS {
        m.preset = preset.into();
        let words: Vec<String> = groups(&m, &map).into_iter().flatten().map(|line| format!("{line:?}").to_lowercase()).collect();
        assert!(!words.iter().any(|line| line.contains("grid") || line.contains("column view")), "{preset}");
    }
}

#[test]
fn each_group_stays_whole_in_at_most_three_columns_at_every_width() {
    let map = Map::load();
    for (columns, height) in [(250, 27), (250, 12), (130, 27), (110, 27), (80, 27), (60, 27), (130, 12), (40, 20), (20, 10), (12, 1)] {
        let mut m = model();
        (m.columns, m.height) = (columns, height);
        let sheet = sheet(&m, &map);
        let case = format!("{columns}x{height}");
        assert!(sheet.columns.len() <= COLUMNS, "{case}");
        assert!(total(&sheet.columns) <= cards::room(columns), "{case}: {} cells", total(&sheet.columns));
        for group in groups(&m, &map) {
            let Line::Heading(heading) = &group[0] else { panic!() };
            let holder: Vec<usize> = (0..sheet.columns.len()).filter(|&c| sheet.columns[c].iter().any(|line| matches!(line, Line::Heading(text) if text == heading))).collect();
            assert_eq!(holder.len(), 1, "{case}: {heading} sits in one column");
            let column = &sheet.columns[holder[0]];
            let start = column.iter().position(|line| matches!(line, Line::Heading(text) if text == heading)).unwrap();
            let stacked = !sheet.columns.iter().flatten().any(|line| matches!(line, Line::Binding(..)));
            if !stacked { assert_eq!(&column[start..start + group.len()], &group[..], "{case}: {heading} whole"); }
        }
        let card = card(&m, &map);
        assert!(card.x + card.width + 2 <= columns && card.y + card.height + 2 <= (height + 2).max(5), "{case}: the card fits");
    }
    let mut m = model();
    m.columns = 30;
    let stacked = sheet(&m, &map);
    assert!(stacked.columns[0].iter().any(|line| matches!(line, Line::Key(keys) if keys == "ctrl-shift-z / ctrl-y")), "a narrow sheet gives a key its own line");
}

#[test]
fn hints_name_the_first_key_without_ctrl_or_alt() {
    let map = Map::load();
    for (preset, action, hint) in [("default", "copy", "y"), ("vim", "copy", "yy"), ("default", "trash", "dd"), ("default", "deletePermanently", "shift-delete"),
        ("default", "print", ""), ("default", "open", "enter"), ("default", "openTerminal", ""), ("default", "unknown", "")] {
        assert_eq!(super::hint(&map, preset, action), hint, "{preset} {action}");
    }
}

#[test]
fn turkish_headings_keep_the_dotted_capital_i() {
    let mut m = model();
    m.i18n = caps_language("tr");
    assert_eq!(m.i18n.language(), "tr");
    assert_eq!(m.i18n.tr("keys.group.change"), "değiştir");
    let headings: Vec<String> = groups(&m, &Map::load()).into_iter().flatten()
        .filter_map(|line| if let Line::Heading(text) = line { Some(text) } else { None }).collect();
    // Plain to_uppercase draws DEĞIŞTIR.
    assert_eq!(headings, ["HAREKET", "GÖRÜNÜM", "BUL", "DEĞİŞTİR"]);
}

#[test]
fn tui_words_review_binding_columns_are_never_clipped_to_key_width() {
    let map = Map::load();
    let mut m = model();
    for preset in PRESETS {
        for columns in 12..=300 {
            for height in [1, 12, 27] {
                m.preset = preset.into();
                (m.columns, m.height) = (columns, height);
                let sheet = sheet(&m, &map);
                let card = card(&m, &map);
                let mut offset = 0;
                for column in &sheet.columns {
                    let (key, label) = widths(column);
                    let width = key + 2 + label;
                    let room = width.min(card.content().saturating_sub(offset));
                    if column.iter().any(|line| matches!(line, Line::Binding(..))) {
                        assert_eq!(room, width, "{preset} {columns}x{height}: a binding column is complete");
                        assert!(room >= key + 2, "the two separator cells are included in the packer's budget");
                    }
                    offset += width + GAP;
                }
            }
        }
    }
}

#[test]
fn yazi_prefix_routes_are_shown_as_sequences_without_changing_other_presets() {
    let map = Map::load();
    assert_eq!(spelling(&map, "yazi", row("tabNew")), "t t");
    assert_eq!(spelling(&map, "yazi", row("cursorFirst")), "g g / home");
    assert_eq!(spelling(&map, "yazi", row("sortNext")), ", a");
    assert!(spelling(&map, "yazi", row("sortReverse")).is_empty(), "Name descending does not reverse an arbitrary active sort");
    for preset in ["default", "vim", "mac", "windows"] {
        assert_eq!(spelling(&map, preset, row("tabNew")), "t");
    }
}

#[test]
fn explicit_layout_snapshots_keep_sequence_parts_and_protocol_pairs_complete() {
    let map = Map::load();
    for preset in PRESETS {
        let mut m = model();
        m.preset = preset.into();
        let mut usable = 0;
        for (layout, variant) in [("us", None), ("ru", None), ("fr", None), ("us", Some("dvorak"))] {
            let snapshot = flea::keys::display::KeycapSnapshot::new(layout, variant, Some(0), 7).unwrap();
            let before = (m.keycaps, m.keycap_labels.clone());
            match m.set_keycaps(snapshot, &map) {
                Ok(()) => {
                    usable += 1;
                    for row in ROWS.iter().filter(|row| listed(row, preset)) {
                        let canonical = spelling(&map, preset, row);
                        if canonical.is_empty() { continue; }
                        let expected = if canonical == "1–9" { format!("{}–{}", snapshot.label("1").unwrap(), snapshot.label("9").unwrap()) }
                            else { label(&canonical, &snapshot).unwrap() };
                        assert_eq!(m.keycap_labels[&canonical], expected, "{preset} {layout} {canonical}");
                    }
                }
                Err(_) => assert_eq!((m.keycaps, m.keycap_labels.clone()), before, "failed snapshot must retain its complete epoch"),
            }
        }
        assert!(usable > 0, "{preset}: no complete layout snapshot");
    }
}

#[test]
fn yazi_sequence_keycaps_relabel_each_part_of_the_sequence() {
    let map = Map::load();
    // Letters keep their caps in both layouts, so only a comma after the prefix shows the second part relabelled.
    let comma = Map::parse("[[tui]]\nname = \"yazi\"\nmods = \"text\"\nkey = \",\"\naction = \"prefix:,\"\ncontext = \"listing\"\n\n[[tui]]\nname = \"yazi\"\nmods = \"text\"\nkey = \",\"\naction = \"tabNew\"\ncontext = \"listing\"\nprefix = \",\"\n");
    for (layout, variant, sort, tail) in [("fr", None, "; a", "; ;"), ("us", Some("dvorak"), "w a", "w w")] {
        let snapshot = flea::keys::display::KeycapSnapshot::new(layout, variant, Some(0), 7).unwrap();
        let mut m = model();
        m.preset = "yazi".into();
        m.set_keycaps(snapshot, &map).unwrap();
        assert_eq!(m.keycap_labels[", a"], sort, "{layout}");
        assert_eq!(m.keycap_labels["t t"], "t t", "{layout}");
        let mut m = model();
        m.preset = "yazi".into();
        m.set_keycaps(snapshot, &comma).unwrap();
        assert_eq!(m.keycap_labels[", ,"], tail, "{layout}");
    }
}

#[test]
fn yazi_editor_labels_compose_the_snapshots_actual_letter_and_named_key_atoms() {
    for layout in ["us","ru"] {
        let snapshot=flea::keys::display::KeycapSnapshot::new(layout,None,Some(0),9).unwrap();
        assert_eq!(label("alt-b / ctrl-left",&snapshot).unwrap(),format!("alt-{} / ctrl-{}",snapshot.label("b").unwrap(),snapshot.label("left").unwrap()));
        assert_eq!(label("alt-d / ctrl-delete",&snapshot).unwrap(),format!("alt-{} / {}",snapshot.label("d").unwrap(),snapshot.label("ctrl-delete").unwrap()));
    }
}
