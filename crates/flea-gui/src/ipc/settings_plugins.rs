use flea::{jsondoc::Json, ui_logic_st2::pages::Row};

pub(super) fn model(row: &Row, catalog: &flea::i18n::I18n) -> Json {
    let (kind, label, value) = match row {
        Row::Group { label, action, .. } => {
            ("group", label.clone(), action.clone().unwrap_or_default())
        }
        Row::Rule => ("rule", String::new(), String::new()),
        Row::Description { text } | Row::Hint { text } => ("hint", text.clone(), String::new()),
        Row::Plugin { name, failure, .. } => {
            ("plugin", name.clone(), failure.clone().unwrap_or_default())
        }
        Row::Check { label, .. } => ("check", label.clone(), String::new()),
        Row::Choice {
            label,
            values,
            labels,
            selected,
            ..
        } => (
            "choice",
            label.clone(),
            values
                .iter()
                .position(|v| v == selected)
                .and_then(|at| labels.get(at))
                .cloned()
                .unwrap_or_default(),
        ),
        Row::Number {
            label, value, unit, ..
        } => {
            let count = catalog.locale().format_count(*value);
            let shown = if unit.is_empty() {
                count
            } else {
                catalog.tr_args("plugins.value.unit", &[&count, unit])
            };
            ("number", label.clone(), shown)
        }
        Row::Field { label, value, .. } => ("field", label.clone(), value.clone()),
        Row::Fact { text, value } => ("fact", text.clone(), value.clone()),
        Row::Action { label, value, .. } => ("action", label.clone(), value.clone()),
        Row::Favourite { label, error, .. } => ("favourite", label.clone(), error.clone()),
    };
    let on = match row {
        Row::Plugin { on, .. } | Row::Check { on, .. } => *on,
        Row::Group { switch, .. } => switch.unwrap_or(false),
        _ => false,
    };
    Json::Obj(vec![
        ("id".into(), Json::Str(row.id().into())),
        ("kind".into(), Json::Str(kind.into())),
        ("label".into(), Json::Str(label)),
        ("value".into(), Json::Str(value)),
        ("on".into(), Json::Bool(on)),
    ])
}

pub(super) fn text(row: &Row, catalog: &flea::i18n::I18n) -> String {
    let json = model(row, catalog);
    ["kind", "label", "value"]
        .map(|key| json.get(key).and_then(Json::as_str).unwrap_or(""))
        .join("|")
}
