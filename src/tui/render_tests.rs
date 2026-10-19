    use super::*;
    use crate::tui::layout::{footer, pane_padding};
    use crate::tui::model::Row;
    use crate::tui::cells::{match_range, Wrapped};
    #[test]
    fn chrome_and_pane_insets_preserve_body_and_minimum_pdf_space() {
        assert_eq!(body_height(67), 63);
        assert_eq!(BODY_ROW + body_height(67), 66);
        assert_eq!(preview_area(277, false), (171, 105));
        assert_eq!(preview_area(277, true), (1, 275));
        for columns in 12..=277 {
            for quicklook in [false, true] {
                let (start, width) = preview_area(columns, quicklook);
                assert!(width >= 5 && start + width <= columns);
                let outer = if quicklook { columns } else { panes(columns, true).2 };
                assert_eq!(start + width + pane_padding(outer), columns);
            }
        }
    }
    #[test]
    fn counted_lines_read_singular_for_one_and_plural_for_many() {
        let mut model = Model::new(std::path::PathBuf::from("/"), &crate::jsondoc::Json::Null);
        let theme = Theme::from_text("");
        model.total = 1;
        let one = footer(&model, &theme, 120, std::time::Duration::ZERO);
        assert!(one.contains("1 item") && !one.contains("1 items"), "one row in the footer: {one}");
        model.total = 1204;
        let many = footer(&model, &theme, 120, std::time::Duration::ZERO);
        assert!(many.contains("1,204 items"), "many rows in the footer: {many}");
        let row = || Row::parse(&crate::jsondoc::Json::Obj(vec![("n".into(), crate::tui::wire::word("a"))]), &[]);
        model.selected = [0].into_iter().collect();
        model.selected_rows.insert(0, row());
        assert_eq!(selection_line(&model, 0, 80), "1 item selected");
        model.selected = [0, 1].into_iter().collect();
        model.selected_rows.insert(1, row());
        assert_eq!(selection_line(&model, 0, 80), "2 items selected");
        model.selected = [0, 1].into_iter().collect();
        model.selected_rows = [(0, row())].into_iter().collect();
        assert_eq!(selection_line(&model, 4, 80), "1 marked item outside loaded rows");
        model.selected = [0, 1, 2].into_iter().collect();
        assert_eq!(selection_line(&model, 4, 80), "2 marked items outside loaded rows");
        model.selected.clear();
        model.selected_rows.clear();
        model.total = 1;
        model.filter = "a".into();
        model.rows = [(0, row())].into_iter().collect();
        let filtered = footer(&model, &theme, 120, std::time::Duration::ZERO);
        assert!(crate::tui::render::frame_tests::plain(&filtered).contains("Filter a  1 of 1 shown"), "one match of one row: {filtered}");
        assert!(!filtered.contains("matches") && !filtered.contains("rows"), "no plural may survive it: {filtered}");
        model.total = 2;
        model.rows.insert(1, row());
        let filtered = footer(&model, &theme, 120, std::time::Duration::ZERO);
        assert!(crate::tui::render::frame_tests::plain(&filtered).contains("Filter a  2 of 2 shown"), "two matches of two rows: {filtered}");
    }
    #[test]
    fn active_tab_remains_in_the_same_visible_hit_map() {
        let mut model = Model::new(std::path::PathBuf::from("/tab-one"), &crate::jsondoc::Json::Null);
        for index in 2..=12 {
            model.tabs.push(super::super::model::Tab { path: format!("/tab-{index}").into(), cursor: 0, back: Vec::new(), forward: Vec::new() });
        }
        model.tab = 11;
        for columns in [12, 40, 80, 200] {
            let tabs = tab_layout(&model, columns);
            assert!(tabs.iter().any(|(index, _)| *index == model.tab));
            assert!(tabs.iter().map(|(_, label)| text_width(label)).sum::<usize>() <= columns);
        }
    }
    #[test]
    fn footer_separates_error_color_and_preserves_secondary_progress() {
        let mut model = Model::new(std::path::PathBuf::from("/"), &crate::jsondoc::Json::Null);
        model.hints = crate::tui::keymap::Hints::new(&Map::load(), "default");
        model.fail("Denied".into());
        model.fail("Missing".into());
        model.transfer = "Copying 2 of 5".into();
        model.search = "Searching 12 matches".into();
        let theme = Theme::from_text("");
        let output = footer(&model, &theme, 120, std::time::Duration::ZERO);
        assert!(output.contains(&format!("{}Denied\x1b[0m", theme.error)));
        assert!(output.find("Denied").unwrap() < output.find("Copying").unwrap());
        // The find status takes the count's place at the head of the row; the transfer still follows the error.
        assert!(output.find("Searching").unwrap() < output.find("Denied").unwrap());
        assert!(crate::tui::render::frame_tests::plain(&output).ends_with("? keys"));
        assert_eq!(output.matches(&theme.error).count(), 1);
    }
    #[test]
    fn footer_hints_follow_the_keymap_and_vanish_with_their_key() {
        let rebound = Map::parse(r##"
[[code]]
key = "Backspace"
action = "escape"
[[text]]
char = "h"
action = "keymapSheet"
"##);
        let mut model = Model::new(std::path::PathBuf::from("/"), &crate::jsondoc::Json::Null);
        model.fail("Denied".into());
        let theme = Theme::from_text("");
        model.hints = crate::tui::keymap::Hints::new(&rebound, "default");
        let output = footer(&model, &theme, 120, std::time::Duration::ZERO);
        assert!(output.contains("Denied") && !output.contains("dismisses"), "{output:?}");
        assert!(crate::tui::render::frame_tests::plain(&output).ends_with("h keys"), "{output:?}");
        assert!(!output.contains("Esc"), "{output:?}");
        model.hints = crate::tui::keymap::Hints::default();
        let output = footer(&model, &theme, 120, std::time::Duration::ZERO);
        assert!(output.contains("Denied") && !output.contains("dismisses") && !output.contains("keys"), "{output:?}");
    }
    #[test]
    fn unsafe_names_cannot_emit_terminal_commands() {
        assert_eq!(clean("a\x1b]52;c;evil\x07\u{202e}b"), "a]52;c;evilb");
        assert_eq!(fit("abcdef", 3), "abc");
        assert_eq!(fit("a", 3), "a  ");
        assert_eq!(
            Wrapped::new("abcdef", 2).collect::<Vec<_>>(),
            vec!["ab", "cd", "ef"]
        );
        assert_eq!(Wrapped::new("", 2).collect::<Vec<_>>(), vec![""]);
        assert!(Wrapped::new("abc", 0).next().is_none());
        assert_eq!(panes(100, true), (21, 39, 38));
        assert_eq!(panes(100, false), (21, 78, 0));
        assert_eq!(bytes(999), "999 B");
        assert_eq!(bytes(1000), "1.0 kB");
        assert_eq!(bytes(1_200_000_000), "1.2 GB");
        assert_eq!(grouped(653), "653");
        assert_eq!(grouped(1204), "1,204");
        assert_eq!(grouped(1234567), "1,234,567");
        let text = "□ İ.txt";
        let (start, end) = match_range(text, "i").unwrap();
        assert_eq!(&text[start..end], "İ");
    }
