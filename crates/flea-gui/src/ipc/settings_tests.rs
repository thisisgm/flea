use super::*;
use flea::ui_logic_settings_pages::{rows::Kind, Text};

#[test]
fn fxuish2b_tail_settings_model_exports_the_existing_action() {
    let catalog = crate::test_catalogs::english();
    let mut row = Row::new(Kind::Group, Text::Empty);
    row.action = "addFavourite";
    assert_eq!(
        row_model(&row, &catalog)
            .get("action")
            .and_then(Json::as_str),
        Some("addFavourite")
    );
}

#[test]
fn fxuish2b_settings_model_exports_the_existing_row_error() {
    let catalog = crate::test_catalogs::english();
    let mut row = Row::new(Kind::Favourite, Text::Empty);
    row.error = Text::Data("The saved place is invalid.".into());
    let model = row_model(&row, &catalog);
    assert_eq!(
        model.get("error").and_then(Json::as_str),
        Some("The saved place is invalid.")
    );
    row.error = Text::Empty;
    assert_eq!(
        row_model(&row, &catalog)
            .get("error")
            .and_then(Json::as_str),
        Some("")
    );
}
