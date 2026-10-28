//! Rail readers: the host's live placement, the producer's drawn boxes and the one rename editor, in ui/Ipc.qml's shapes.
use super::rows::centre_of;
use super::Adapter;
use crate::g2::pl4::producer::Producer;
use crate::g2::pl4::rows::LaidRow;
use crate::rail::host::Host;
use crate::window::composition::Geometry;
use flea::ipc::wire::Reply;
use flea::json::escape;
use flea::ui_logic_pl4::rail::{Group, Kind, RailEntry};
pub use network::NetworkBuilt;
use std::time::Instant;

pub struct RailAdapter<'a> {
    pub host: Option<&'a Host>,
    pub producer: Option<&'a Producer>,
    pub geometry: &'a Geometry,
    pub focus_side: usize,
    pub now: Instant,
    pub network: NetworkBuilt,
}

fn whole(value: f32) -> String {
    if (value - value.round()).abs() < 0.01 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.2}")
    }
}

fn rect_json(rect: crate::rail::geometry::Rect) -> String {
    format!(
        "{{\"x\":{},\"y\":{},\"width\":{},\"height\":{}}}",
        whole(rect.x),
        whole(rect.y),
        whole(rect.width),
        whole(rect.height)
    )
}

pub(super) fn group_name(group: Group) -> &'static str {
    match group {
        Group::Home => "home",
        Group::Recent => "recent",
        Group::Trash => "trash",
        Group::Favourite => "favourite",
        Group::Favorite => "favorite",
        Group::Tag => "tag",
        Group::Network => "network",
        Group::Device => "device",
        Group::Plugin => "plugin",
    }
}

pub(super) fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Home => "home",
        Kind::Recent => "recent",
        Kind::Trash => "trash",
        Kind::Favourite => "favourite",
        Kind::Favorite => "favorite",
        Kind::Tag => "tag",
        Kind::Share => "share",
        Kind::Dropbox => "dropbox",
        Kind::Cloud => "cloud",
        Kind::Volume => "volume",
        Kind::Disk => "disk",
        Kind::Phone => "phone",
        Kind::PluginRow => "pluginrow",
    }
}

fn entry_json(entry: &RailEntry, catalog: &flea::i18n::I18n, trash_count: u64) -> String {
    let mut fields = vec![
        format!(
            "\"label\":\"{}\"",
            escape(&crate::g2::pl4::content::row_label(catalog, entry))
        ),
        format!("\"path\":\"{}\"", escape(&entry.path)),
        format!("\"group\":\"{}\"", group_name(entry.group)),
        format!("\"kind\":\"{}\"", kind_name(entry.kind)),
        format!("\"glyph\":\"{}\"", escape(entry.glyph)),
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
            flea::ui_logic_pl4::rail::FavouriteError::Record(record) => record.key(),
            flea::ui_logic_pl4::rail::FavouriteError::Status(status) => status,
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

fn row_json(index: usize, row: &LaidRow) -> String {
    format!(
        "{{\"index\":{index},\"label\":\"{}\",\"kind\":\"{}\",\"group\":\"{}\",\"detail\":\"{}\",\"detailColor\":\"{}\",\"labelWidth\":{},\"labelNeeds\":{},\"elided\":{},\"elide\":1,\"fontSize\":{},\"tabular\":true,\"rect\":{},\"detailRect\":{},\"labelRect\":{},\"indicatorRect\":{},\"indicatorVisible\":{}}}",
        escape(&row.label),
        "",
        "",
        escape(&row.detail),
        escape(&row.detail_color),
        (row.label_width + 0.5).floor() as i64,
        (row.label_needs + 0.5).floor() as i64,
        row.elided,
        whole(row.font_size),
        rect_json(row.rect),
        rect_json(row.detail_rect),
        rect_json(row.label_rect),
        rect_json(row.indicator_rect),
        row.indicator_visible
    )
}

impl RailAdapter<'_> {
    pub(super) fn shown(&self) -> bool {
        self.host.is_some_and(|host| host.rail.shown())
    }

    pub(super) fn producer(&self) -> Option<&Producer> {
        self.host.filter(|host| host.rail.live()).and(self.producer)
    }

    pub(super) fn entries(&self) -> Vec<RailEntry> {
        if !self.shown() {
            return Vec::new();
        }
        self.producer().map(Producer::entries).unwrap_or_default()
    }

    fn labels(&self) -> Vec<String> {
        if !self.shown() {
            return Vec::new();
        }
        self.producer()
            .map(|producer| {
                producer
                    .provider
                    .drawn
                    .iter()
                    .flat_map(|section| section.rows.iter().map(|row| row.label.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn state(&self) -> Option<String> {
        let host = self.host?;
        let shown = host.rail.shown();
        let width = if shown { host.width() } else { 0. };
        let column = crate::rail::geometry::Rect::new(
            0.,
            self.geometry.rail.y,
            self.geometry.chrome.width,
            self.geometry.rail.height,
        );
        let placement = host.rail.placement(host.width(), column, self.now);
        Some(format!(
            "{{\"hidden\":{},\"width\":{},\"inset\":{},\"pane\":{},\"revealed\":{},\"x\":{}}}",
            !shown,
            whole(width),
            whole(placement.inset),
            whole(self.geometry.chrome.width),
            host.rail.revealed(),
            whole(placement.rail.x)
        ))
    }

    fn details(&self) -> String {
        if !self.shown() {
            return "{\"rowGap\":0,\"headers\":[],\"rows\":[]}".into();
        }
        let (row_gap, headers, laid) = self
            .producer
            .map(|producer| {
                let laid = producer.laid.borrow();
                (laid.row_gap, laid.headers.clone(), laid.clone())
            })
            .unwrap_or_default();
        let mut head = String::from("[");
        for (at, header) in headers.iter().enumerate() {
            if at > 0 {
                head.push(',');
            }
            head.push_str(&format!(
                "{{\"text\":\"{}\",\"visible\":true,\"rect\":{},\"topPadding\":{},\"fontSize\":{}}}",
                escape(&header.text),
                rect_json(header.rect),
                whole(header.top_padding),
                whole(header.font_size)
            ));
        }
        head.push(']');
        let entries = self.producer().map(Producer::entries).unwrap_or_default();
        let mut rows = String::from("[");
        for (index, entry) in entries.iter().enumerate() {
            if index > 0 {
                rows.push(',');
            }
            match laid.row(index) {
                Some(row) => {
                    let mut said = row_json(index, row);
                    // The entry names the group and kind; the painted row carries the rest.
                    said = said.replacen(
                        "\"kind\":\"\"",
                        &format!("\"kind\":\"{}\"", kind_name(entry.kind)),
                        1,
                    );
                    said = said.replacen(
                        "\"group\":\"\"",
                        &format!("\"group\":\"{}\"", group_name(entry.group)),
                        1,
                    );
                    rows.push_str(&said);
                }
                None => rows.push_str(&format!("{{\"index\":{index},\"missing\":true}}")),
            }
        }
        rows.push(']');
        format!(
            "{{\"rowGap\":{},\"headers\":{},\"rows\":{}}}",
            whole(row_gap),
            head,
            rows
        )
    }
}

impl Adapter for RailAdapter<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        let index = || {
            arguments
                .first()
                .and_then(|value| value.parse::<usize>().ok())
        };
        let value = match function {
            "railCount" => {
                if !self.shown() {
                    "0".into()
                } else {
                    self.producer()
                        .map(Producer::count)
                        .unwrap_or(0)
                        .to_string()
                }
            }
            "railCursor" => {
                if !self.shown() {
                    "0".into()
                } else {
                    self.producer()
                        .and_then(Producer::cursor)
                        .map_or_else(|| "-1".into(), |index| index.to_string())
                }
            }
            "railEntries" => {
                if !self.shown() {
                    return Some(Reply::value("[]"));
                }
                let catalog = &self.producer()?.catalog;
                let rows: Vec<String> = self
                    .entries()
                    .iter()
                    .map(|entry| {
                        entry_json(
                            entry,
                            catalog,
                            self.producer().map_or(0, |p| p.model.borrow().trash_count),
                        )
                    })
                    .collect();
                format!("[{}]", rows.join(","))
            }
            "railDetails" => self.details(),
            "railState" => self.state()?,
            "focusView" if self.producer().is_some_and(|p| p.model.borrow().focused) => {
                "rail".into()
            }
            "railLabel" => index()
                .and_then(|index| {
                    if !self.shown() {
                        return None;
                    }
                    self.producer().and_then(|producer| {
                        producer
                            .laid
                            .borrow()
                            .row(index)
                            .map(|row| row.label.clone())
                    })
                })
                .unwrap_or_default(),
            "railLabels" => self.labels().join("|"),
            "railRowCentre" => index()
                .and_then(|index| {
                    if !self.shown() {
                        return None;
                    }
                    self.producer().and_then(|producer| {
                        let laid = producer.laid.borrow();
                        laid.row(index)
                            .filter(|row| {
                                row.rect.y >= self.geometry.rail.y
                                    && row.rect.y + row.rect.height
                                        <= self.geometry.rail.y + laid.height
                            })
                            .map(|row| {
                                centre_of(row.rect.x, row.rect.y, row.rect.width, row.rect.height)
                            })
                    })
                })
                .unwrap_or_default(),
            "railRenamingIndex" => self
                .producer()
                .and_then(Producer::rename_index)
                .map_or("-1".into(), |index| index.to_string()),
            "railRenameEditorLive" => self
                .producer()
                .is_some_and(|producer| producer.rename_index().is_some())
                .to_string(),
            "railRenameEditorText" => self
                .producer()
                .map(Producer::rename_text)
                .unwrap_or_default(),
            "railRenameFieldShown" => self
                .producer()
                .is_some_and(|producer| self.shown() && producer.field_shown())
                .to_string(),
            // One line per entry, "label|group|kind|mounted", in ui/Ipc.qml's shape.
            "networkEntries" => {
                let catalog = self.producer().map(|producer| &producer.catalog);
                self.entries()
                    .iter()
                    .filter(|entry| entry.group == Group::Network)
                    .map(|entry| {
                        let label = catalog.map_or_else(
                            || entry.label.to_string(),
                            |catalog| {
                                crate::g2::pl4::content::row_label(catalog, entry).to_string()
                            },
                        );
                        format!(
                            "{}|{}|{}|{}",
                            label,
                            group_name(entry.group),
                            kind_name(entry.kind),
                            entry.mounted.unwrap_or(false)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            "networkBuilt" => self.network.ready().to_string(),
            // The row whose eject mark holds the pointer, by index (rail/rows.rs RowsModel.mark_hovered, set from producer_view.rs).
            "railMarkHovered" => self
                .producer()
                .is_some_and(|producer| {
                    index().is_some_and(|row| producer.model.borrow().mark_hovered == Some(row))
                })
                .to_string(),
            "networkMarkGeometry" => self.mark_geometry().unwrap_or_default(),
            "networkMarkCentre" => self.mark_centre().unwrap_or_default(),
            _ => return None,
        };
        Some(Reply::value(value))
    }
}

#[path = "rail_network.rs"]
mod network;
