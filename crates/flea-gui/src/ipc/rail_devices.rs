//! railEntries adds v0.3.12 device facts from the source that built each row.
use super::Adapter;
use crate::g2::device_rail::source::Source;
use flea::ipc::wire::Reply;
use flea::json::escape;
use flea::jsondoc::{self, Json};
use flea::ui_logic_device_rail::Kind;

pub(crate) struct RailDeviceFacts<'a> {
    pub rail: &'a dyn Adapter,
    pub devices: Option<&'a Source>,
}

// One line, as the rail reader writes its rows.
fn compact(out: &mut String, value: &Json) {
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Json::Num(value) => out.push_str(value),
        Json::Str(value) => {
            out.push('"');
            out.push_str(&escape(value));
            out.push('"');
        }
        Json::Arr(items) => {
            out.push('[');
            for (at, item) in items.iter().enumerate() {
                if at > 0 {
                    out.push(',');
                }
                compact(out, item);
            }
            out.push(']');
        }
        Json::Obj(pairs) => {
            out.push('{');
            for (at, (key, item)) in pairs.iter().enumerate() {
                if at > 0 {
                    out.push(',');
                }
                out.push('"');
                out.push_str(&escape(key));
                out.push_str("\":");
                compact(out, item);
            }
            out.push('}');
        }
    }
}

impl RailDeviceFacts<'_> {
    // [{"group":"device","kind":"volume","device":"/dev/sda1"}]
    fn enrich(&self, text: &str) -> Option<String> {
        let devices = self.devices?.rows();
        let Ok(Json::Arr(mut entries)) = jsondoc::parse(text) else {
            return None;
        };
        for entry in &mut entries {
            let Json::Obj(pairs) = entry else { continue };
            let field = |key: &str| {
                pairs
                    .iter()
                    .find(|(name, _)| name == key)
                    .and_then(|(_, value)| value.as_str())
                    .unwrap_or_default()
                    .to_owned()
            };
            let (group, kind, device) = (field("group"), field("kind"), field("device"));
            if group != "device" || device.is_empty() || !matches!(kind.as_str(), "volume" | "disk")
            {
                continue;
            }
            let Some(row) = devices
                .iter()
                .find(|row| matches!(row.kind, Kind::Volume | Kind::Disk) && row.device == device)
            else {
                continue;
            };
            let size = row
                .size
                .map_or(Json::Null, |size| Json::Num(size.to_string()));
            pairs.push(("size".into(), size));
            // v0.3.12's disk row names only its size; a volume also says whether it is a locked container and its type.
            if row.kind == Kind::Volume {
                pairs.push(("locked".into(), Json::Bool(row.locked)));
                pairs.push(("fstype".into(), Json::Str(row.fstype.clone())));
            }
        }
        let mut out = String::new();
        compact(&mut out, &Json::Arr(entries));
        Some(out)
    }
}

impl Adapter for RailDeviceFacts<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        let reply = self.rail.call(function, arguments)?;
        if function != "railEntries" || reply.code != 0 {
            return Some(reply);
        }
        Some(
            self.enrich(reply.stdout.trim_end())
                .map_or(reply, Reply::value),
        )
    }
}
