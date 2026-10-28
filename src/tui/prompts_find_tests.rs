use super::*;

// Find uses the shared Home and folder scope wording while preserving the prompt cells.
#[test]
fn find_prompt_names_its_scope_and_tab_changes_it() {
    let catalog = crate::tui::contents_process_tests::Root::new();
    let theme = Theme::from_text("");
    let (mut w, reader) = echo_wire();
    let mut m = frame_tests::model();
    m.path = std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join("work");
    m.prompts.i18n = crate::tui::contents_tests::catalog(&catalog);
    m.editor = Some(Editor::new("search", "item".into(), m.path.clone()));
    let (cells, caret) = row(&m);
    assert_eq!(span(&cells, 0, "Find").0, theme.muted);
    assert_eq!(span(&cells, 5, "item").1, theme.field);
    assert_eq!(cells[9].2, theme.field, "the caret's cell is the field's");
    assert_eq!(span(&cells, 11, "in ~ · tab changes scope").0, theme.muted);
    assert_eq!(caret, Some(9));
    press(&mut m, &mut w, Key::named("Tab", ""));
    let (cells, _) = row(&m);
    span(&cells, 11, "in ~/work · tab changes scope");
    finish(w, reader);
}

// Results retain the folder metadata ink and complete Find query and scope wording.
#[test]
fn search_results_print_their_folder_and_the_status_names_the_search() {
    let catalog = crate::tui::contents_process_tests::Root::new();
    let theme = Theme::from_text("");
    let (mut w, reader) = echo_wire();
    let mut m = frame_tests::model();
    m.rows
        .insert(0, frame_tests::entry("Many/item-02.txt", false, 7));
    let cells = inks(&crate::tui::cells::row(
        &m.rows[&0],
        51,
        Some("7 B"),
        true,
        Default::default(),
        &theme,
        "",
    ));
    assert_eq!(span(&cells, 3, "item-02.txt").0, theme.foreground);
    assert_eq!(span(&cells, 15, "Many").0, theme.meta);
    m.prompts.i18n = crate::tui::contents_tests::catalog(&catalog);
    m.search = "x".into();
    m.search_query = "item".into();
    m.find.session.scope = flea::ui_logic_sr1::Scope::Folder;
    event(&mut m, &mut w, r#"{"t":"searching","n":80,"scanned":114}"#);
    assert_eq!(m.search, "80 found · 114 scanned");
    event(&mut m, &mut w, r#"{"t":"searched","n":80,"scanned":114}"#);
    let (cells, _) = row(&m);
    assert_eq!(span(&cells, 0, "80 found").0, theme.foreground);
    assert_eq!(span(&cells, 10, "Find “item” in /fx/v/work").0, theme.muted);
    finish(w, reader);
}
