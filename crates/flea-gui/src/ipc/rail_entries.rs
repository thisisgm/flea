//! JSON for the rail rows the producer retained.
use super::*;

pub(super) fn entry_json(
    entry: &RailEntry,
    catalog: &flea::i18n::I18n,
    trash_count: u64,
) -> String {
    let mut fields = vec![
        format!(
            "\"label\":\"{}\"",
            escape(&crate::g2::rail_places::content::row_label(catalog, entry))
        ),
        format!("\"path\":\"{}\"", escape(&entry.path)),
        format!("\"group\":\"{}\"", group_name(entry.group)),
        format!("\"kind\":\"{}\"", kind_name(entry.kind)),
        format!("\"glyph\":\"{}\"", escape(entry.glyph)),
        format!("\"releasable\":{}", entry.releasable),
    ];
    if entry.kind == Kind::Trash {
        fields.push(format!("\"count\":{trash_count}"));
    }
    if !entry.uri.is_empty() {
        fields.push(format!("\"uri\":\"{}\"", escape(&entry.uri)));
    }
    if !entry.mount_uri.is_empty() {
        fields.push(format!("\"mountUri\":\"{}\"", escape(&entry.mount_uri)));
    }
    if !entry.device.is_empty() {
        fields.push(format!("\"device\":\"{}\"", escape(&entry.device)));
    }
    if let Some(mounted) = entry.mounted {
        fields.push(format!("\"mounted\":{mounted}"));
    }
    if let Some(tag) = entry.tag {
        fields.push(format!("\"tag\":\"{}\"", escape(tag)));
    }
    if let Some(index) = entry.favourite_index {
        fields.push(format!("\"favouriteIndex\":{index}"));
    }
    if let Some(original) = &entry.original {
        fields.push(format!("\"original\":{}", flea::jsondoc::render(original)));
    }
    if let Some(error) = &entry.error {
        let said = match error {
            flea::ui_logic_rail_places::rail::FavouriteError::Record(record) => record.key(),
            flea::ui_logic_rail_places::rail::FavouriteError::Status(status) => status,
        };
        fields.push(format!("\"error\":\"{}\"", escape(said)));
    }
    if !entry.section.is_empty() {
        fields.push(format!("\"section\":\"{}\"", escape(&entry.section)));
    }
    if !entry.id.is_empty() {
        fields.push(format!("\"id\":\"{}\"", escape(&entry.id)));
    }
    format!("{{{}}}", fields.join(","))
}
