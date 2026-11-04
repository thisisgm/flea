use super::*;
use crate::tui::{render::frame_tests::{model, plain, grid, entry}, wire};
use crate::jsondoc::{self, Json};

#[test]
fn t3_header_keeps_crumbs_leading_and_trailing_tabs_in_the_mouse_hit_map() {
    let mut m = model();
    let theme = Theme::from_text("");
    assert!(tab_layout(&m, 128).is_empty());
    let one = plain(&header(&m, &theme, 130));
    assert!(one.starts_with(" /fx/v/work"));
    assert!(one.ends_with("Name ▴ "));
    assert!(!one.contains("1 work"));
    m.tabs.push(m.tabs[0].clone()); m.tab = 1;
    let tabs = tab_layout(&m, 128);
    assert_eq!(tabs[0], (usize::MAX, " ".repeat(104)));
    assert_eq!(tabs[1], (0, " 1 work ".into()));
    assert_eq!(tabs[2], (1, " 2 work ".into()));
    let two = plain(&header(&m, &theme, 130));
    assert_eq!(two.chars().skip(105).collect::<String>(), " 1 work  2 work   Name ▴ ");
    assert_eq!(one.find("/fx/v/work"), two.find("/fx/v/work"));
    m.path = "/a-very-long-ancestor/another-ancestor/current".into();
    let short = crumbs(&display_path(&m), 18, &theme);
    assert!(plain(&short).ends_with("current"));
    assert!(short.contains(&format!("{}current", theme.foreground)));
    for columns in [12, 20, 40, 130] {
        let spans = tab_layout(&m, columns - 2);
        assert!(spans.iter().any(|(i, _)| *i == m.tab));
        assert_eq!(text_width(&plain(&header(&m, &theme, columns))), columns);
    }
}
#[test]
fn t3_small_notice_matches_both_reference_regions_and_wraps_whole_words() {
    let theme = Theme::from_text("");
    let wide = grid(&small(130, 4, &theme), 130, 4);
    assert_eq!(wide[1].trim(), "Make the window at least 12 columns wide and 5 rows tall.");
    let narrow = grid(&small(11, 6, &theme), 11, 6);
    assert_eq!(narrow.iter().map(|s| s.trim()).collect::<Vec<_>>(), ["Make the", "window at", "least 12", "columns", "wide and 5", "rows tall."]);
}
#[test]
fn t3_footer_uses_plain_selection_filter_disk_and_error_roles_without_collision() {
    let mut m = model();
    let theme = Theme::from_text("");
    m.selected.extend([7, 8]);
    m.selected_rows = m.rows.iter().filter(|(i, _)| m.selected.contains(i)).map(|(i, r)| (*i, r.clone())).collect();
    let selected = footer(&m, &theme, 128, Duration::ZERO);
    assert_eq!(plain(&selected).trim(), format!("2 of 9 selected · 147 B{}tmpfs · 64.1 MB free · ? keys", " ".repeat(76)));
    assert!(!selected.contains("\x1b[7m"));
    m.selected.clear(); m.filter = "o".into();
    assert!(plain(&footer(&m, &theme, 128, Duration::ZERO)).starts_with("Filter o  5 of 9 shown"));
    let frame = plain(&crate::tui::render::frame(&m, &theme, &crate::tui::keymap::Map::load(), 130, 31, Duration::ZERO));
    assert!(frame.contains("Filter o  5 of 9 shown") && !frame.contains("hidden by the filter"), "the count replaces the retired body note: {frame}");
    m.filter.clear(); m.error = "You don’t have permission to open this folder.".into();
    let error = footer(&m, &theme, 128, Duration::ZERO);
    assert!(error.contains(&format!("{}{}", theme.error, m.error)));
    assert!(!plain(&error).contains("dismiss"));
    for width in 0..131 {
        let text = plain(&footer(&m, &theme, width, Duration::ZERO));
        assert_eq!(text_width(&text), width);
        assert!(!text.contains('\n'));
    }
}
#[test]
fn t3_focus_and_scroll_thumb_expire_without_ongoing_redraw() {
    let mut m = model(); let now = Instant::now();
    assert!(focused(&m));
    m.preview_focus = true; assert!(!focused(&m)); m.preview_focus = false;
    for kind in ["filter", "search", "rename", "newFolder"] {
        m.editor = Some(crate::tui::editor::Editor::new(kind, String::new(), m.path.clone()));
        assert_eq!(focused(&m), matches!(kind, "filter" | "search"));
    }
    m.editor = None; m.menu = true; assert!(!focused(&m)); m.menu = false;
    m.sheet = true; assert!(!focused(&m)); m.sheet = false;
    m.total = 80; m.top = 53; m.scrolled_at = Some(now);
    assert_eq!(thumb(&m, 27, now), Some((18, 9)));
    assert_eq!(thumb(&m, 27, now + Duration::from_millis(700)), None);
    m.total = 10; assert_eq!(thumb(&m, 27, now), None);
}
#[test]
fn t3_folder_sizes_wait_for_settle_cancel_and_ignore_stale_replies() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = model(); m.folder_sizes.clear(); m.rows.insert(40, entry("offscreen", true, 0));
    let now = Instant::now();
    m.request_sizes(&mut w, now).unwrap();
    assert_eq!(wire::text(&w.events.recv_timeout(Duration::from_secs(1)).unwrap().unwrap(), "c"), "fsinfo");
    m.request_sizes(&mut w, now + Duration::from_millis(119)).unwrap();
    assert!(w.events.recv_timeout(Duration::from_millis(20)).is_err());
    m.request_sizes(&mut w, now + Duration::from_millis(120)).unwrap();
    let sent = w.events.recv_timeout(Duration::from_secs(1)).unwrap().unwrap();
    assert_eq!(wire::text(&sent, "c"), "dirsize");
    assert_eq!(sent.get("rows").and_then(Json::as_array).unwrap().len(), 7);
    m.receive(jsondoc::parse(r#"{"t":"dirsized","row":0,"bytes":462,"partial":false}"#).unwrap(), &mut w).unwrap();
    m.receive(jsondoc::parse(r#"{"t":"dirsized","row":4,"bytes":60,"partial":true}"#).unwrap(), &mut w).unwrap();
    assert_eq!(m.row_size(0, &m.rows[&0]), "462 B");
    assert_eq!(m.row_size(4, &m.rows[&4]), ">60 B");
    m.top = 40; m.height = 1;
    m.request_sizes(&mut w, now + Duration::from_millis(121)).unwrap();
    let cancel = w.events.recv_timeout(Duration::from_secs(1)).unwrap().unwrap();
    assert_eq!(wire::text(&cancel, "c"), "dirsizecancel");
    m.receive(jsondoc::parse(r#"{"t":"dirsized","row":1,"bytes":999,"partial":false}"#).unwrap(), &mut w).unwrap();
    assert!(!m.folder_sizes.contains_key(&1));
    assert!(m.folder_sizes.is_empty());
    m.disk_class = "network".into();
    m.request_sizes(&mut w, now + Duration::from_secs(1)).unwrap();
    assert!(w.events.recv_timeout(Duration::from_millis(20)).is_err());
    drop(w); reader.join().unwrap();
}
#[test]
fn t3_disk_replies_require_the_current_directory_and_no_pending_navigation() {
    let (mut w, reader) = wire::echo_wire(); let mut m = model();
    for value in [r#"{"t":"fsinfo","path":"/old","fs":"wrong","free":1}"#,
        r#"{"t":"fsinfo","path":"/fx/v/work","fs":"ext4","free":4096}"#] {
        m.receive(jsondoc::parse(value).unwrap(), &mut w).unwrap();
    }
    assert_eq!(m.disk_fs, "ext4"); assert_eq!(m.disk_free, 4096);
    m.pending = Some("/next".into());
    m.receive(jsondoc::parse(r#"{"t":"fsinfo","path":"/fx/v/work","fs":"stale","free":1}"#).unwrap(), &mut w).unwrap();
    assert_eq!(m.disk_free, 4096);
    drop(w); reader.join().unwrap();
}
#[test]
fn t3_progress_uses_current_backend_samples_and_plain_bar_roles() {
    let (mut w, reader) = wire::echo_wire(); let mut m = model();
    m.transfer_id = 7; m.transfer_total = 5;
    for (id, expected) in [(99, None), (7, Some(40))] {
        let value = format!(r#"{{"t":"transferprogress","id":{id},"index":1,"bytes":100,"total":100}}"#);
        m.receive(jsondoc::parse(&value).unwrap(), &mut w).unwrap();
        assert_eq!(m.progress_percent, expected);
    }
    let theme = Theme::from_text("");
    let output = footer(&m, &theme, 128, Duration::ZERO);
    assert!(plain(&output).contains(&format!("Copying 2 of 5  {}{}  40%", "━".repeat(8), "─".repeat(12))));
    assert!(output.contains(&format!("{}{}", theme.accent, "━".repeat(8))));
    assert!(output.contains(&format!("{}{}", theme.border, "─".repeat(12))));
    let value = r#"{"t":"transferprogress","id":7,"index":0,"bytes":285,"total":100}"#;
    m.receive(jsondoc::parse(value).unwrap(), &mut w).unwrap();
    assert_eq!(m.progress_percent, Some(20), "bytes past the total stop at the item");
    m.transfer_total = 1;
    m.receive(jsondoc::parse(r#"{"t":"transferprogress","id":7,"index":0,"bytes":57,"total":100}"#).unwrap(), &mut w).unwrap();
    assert_eq!(m.progress_percent, Some(57));
    assert!(!output.contains("\x1b[7m"));
    drop(w); reader.join().unwrap();
}

#[test]
fn review_t3_r1_unicode_crumbs_are_fitted_without_reusing_source_offsets() {
    let theme = Theme::from_text("");
    for path in ["/\u{301}a/界e\u{301}", "\u{301}a", "/\x1bancestor/\u{301}a", "/界/\u{301}"] {
        for columns in 0..60 {
            let output = plain(&crumbs(path, columns, &theme));
            assert_eq!(text_width(&output), columns, "{path:?} at {columns}");
            assert!(!output.chars().any(|c| !cells::safe(c)));
        }
    }
}
