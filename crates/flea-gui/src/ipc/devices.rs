//! Device readers use the published DEVICES rows and the live release machine, without starting I/O.
use super::Adapter;
use crate::g2::device_rail::source::Source;
use flea::{
    ipc::wire::Reply,
    ui_logic_device_rail::{Group, Kind},
};

pub struct DeviceAdapter<'a> {
    pub source: Option<&'a Source>,
}

impl Adapter for DeviceAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        let value = match function {
            "deviceEjectState" => self.source.map(Source::eject_state).unwrap_or_default(),
            // Only rows the rail has published, so a reader never sees a row the window does not draw.
            "deviceEntries" => self
                .source
                .filter(|source| source.devices_published())
                .map(|source| {
                    source
                        .rows()
                        .iter()
                        .map(|row| {
                            let group = match row.group {
                                Group::Device => "device",
                                Group::Network => "network",
                                Group::Favourite => "favourite",
                                Group::None => "",
                            };
                            let kind = match row.kind {
                                Kind::Disk => "disk",
                                Kind::Volume => "volume",
                                Kind::Phone => "phone",
                                Kind::Share => "share",
                                Kind::Dropbox => "dropbox",
                                Kind::Cloud => "cloud",
                                Kind::Favourite => "favourite",
                                Kind::None => "",
                            };
                            format!("{}|{group}|{kind}|{}", field(&row.label), row.mounted)
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default(),
            _ => return None,
        };
        Some(Reply::value(value))
    }
}

/// A label is one field of one line: its separator and line breaks read as spaces.
fn field(label: &str) -> String {
    label
        .chars()
        .map(|ch| {
            if ch == '|' || ch == '\n' || ch == '\r' {
                ' '
            } else {
                ch
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_label_never_splits_its_line_or_its_fields() {
        assert_eq!(super::field("A|B\nC\rD"), "A B C D");
        assert_eq!(super::field("128GB"), "128GB");
    }
}
