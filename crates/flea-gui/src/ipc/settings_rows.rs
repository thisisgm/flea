//! Read-only Settings row values from the mounted page model.
use super::{kind, object, role, word};
use flea::{
    jsondoc::Json,
    ui_logic_settings_pages::rows::{Master, Row},
};

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;

pub(super) fn row_model(row: &Row, catalog: &flea::i18n::I18n) -> Json {
    object([
        ("id", word(&row.id)),
        ("action", word(row.action)),
        ("kind", word(kind(row.kind))),
        ("label", word(row.label.resolve(catalog))),
        ("value", word(row.value.resolve(catalog))),
        ("error", word(row.error.resolve(catalog))),
        (
            "caption",
            if row.caption == flea::ui_logic_settings_pages::Text::Empty {
                Json::Null
            } else {
                word(row.caption.resolve(catalog))
            },
        ),
        (
            "elide",
            if row.elide_head {
                word("head")
            } else if row.elide_right {
                word("right")
            } else {
                Json::Null
            },
        ),
        ("on", Json::Bool(row.on)),
        ("inert", Json::Bool(row.inert)),
        ("available", Json::Bool(row.available)),
        ("role", word(role(row.role))),
        ("master", Json::Bool(row.master)),
        (
            "state",
            word(match row.state {
                Master::All => "all",
                Master::Some => "some",
                Master::None => "none",
                Master::Absent => "",
            }),
        ),
        ("selected", word(&row.selected)),
        ("values", Json::Arr(row.values.iter().map(word).collect())),
        (
            "labels",
            Json::Arr(
                row.labels
                    .iter()
                    .map(|v| word(v.resolve(catalog)))
                    .collect(),
            ),
        ),
        (
            "items",
            Json::Arr(
                row.items
                    .iter()
                    .map(|item| {
                        object([
                            ("keys", word(&item.keys)),
                            ("label", word(item.label.resolve(catalog))),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}
