//! One discovery generation per open; looking, empty and errors never add a caption.
use flea::{
    jsondoc::{self, Json},
    ui_logic_pl2::networkaddress::{self, Address},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Host {
    pub name: String,
    pub tailnet: bool,
    pub address: Address,
}
#[derive(Default)]
pub struct Discovery {
    pub id: Option<u64>,
    serial: u64,
    pub rows: Vec<Host>,
    pub diagnostics: Vec<String>,
    pub looking: bool,
}

impl Discovery {
    pub fn open(&mut self) -> String {
        self.serial += 1;
        self.id = Some(self.serial);
        self.rows.clear();
        self.diagnostics.clear();
        self.looking = true;
        format!(r#"{{"c":"nearby","op":"start","id":{}}}"#, self.serial)
    }
    pub fn close(&mut self) -> Option<String> {
        let id = self.id.take()?;
        self.rows.clear();
        self.diagnostics.clear();
        self.looking = false;
        Some(format!(r#"{{"c":"nearby","op":"stop","id":{id}}}"#))
    }
    pub fn line(&mut self, line: &str) -> bool {
        if line.len() > 1 << 20 {
            return false;
        }
        let Ok(node) = jsondoc::parse(line) else {
            return false;
        };
        if node.get("t").and_then(Json::as_str) != Some("nearby")
            || self.id.is_none()
            || node.get("id").and_then(|n| match n {
                Json::Num(n) => n.parse::<u64>().ok(),
                _ => None,
            }) != self.id
        {
            return false;
        }
        let Some(state) = node.get("state").and_then(Json::as_str) else {
            return false;
        };
        if !matches!(state, "looking" | "found" | "empty" | "stopped" | "error") {
            return false;
        }
        let Some(rows) = node.get("rows").and_then(Json::as_array) else {
            return false;
        };
        if rows.len() > 256 {
            return false;
        }
        let parsed = rows.iter().map(host).collect::<Option<Vec<_>>>();
        let Some(parsed) = parsed else {
            return false;
        };
        self.rows = if state == "found" { parsed } else { Vec::new() };
        self.looking = state == "looking";
        self.diagnostics = node
            .get("errors")
            .and_then(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        true
    }
    pub fn filtered(&self, query: &str) -> Vec<usize> {
        let query = query.trim().to_lowercase();
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                query.is_empty()
                    || row.name.to_lowercase().contains(&query)
                    || row.address.host.to_lowercase().contains(&query)
            })
            .map(|(i, _)| i)
            .collect()
    }
}

fn host(node: &Json) -> Option<Host> {
    let name = node.get("name").and_then(Json::as_str)?.to_string();
    if name.chars().any(char::is_control) || name.len() > 8192 {
        return None;
    }
    let tailnet = match node.get("source").and_then(Json::as_str)? {
        "mdns" => false,
        "tailscale" => true,
        _ => return None,
    };
    let address = networkaddress::parse(node.get("uri").and_then(Json::as_str)?).ok()?;
    Some(Host {
        name,
        tailnet,
        address,
    })
}
