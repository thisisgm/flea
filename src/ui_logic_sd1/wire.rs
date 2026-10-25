//! The backend's localsend replies parsed once into events; foreign, out-of-order or malformed lines change nothing.
//! Sample input: {"t":"localsendsent","id":3,"ok":true,"reason":""}
use crate::jsondoc::{self, Json};

// One device the backend discovered, named the way its owner named it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Discovered {
    pub name: String,
    pub address: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Peers { id: u64, peers: Vec<Discovered>, reason: String },
    Sent { id: u64, ok: bool, reason: String },
}

fn text(doc: &Json, key: &str) -> Result<String, String> {
    match doc.get(key) {
        None | Some(Json::Null) => Ok(String::new()),
        Some(Json::Str(value)) => Ok(value.clone()),
        Some(_) => Err(format!("{key} must be a string")),
    }
}

fn number(doc: &Json, key: &str) -> Result<u64, String> {
    match doc.get(key) {
        None | Some(Json::Null) => Ok(0),
        Some(Json::Num(value)) => value.parse().map_err(|_| format!("{key} must be an unsigned integer")),
        Some(_) => Err(format!("{key} must be a number")),
    }
}

fn flag(doc: &Json, key: &str) -> bool {
    doc.get(key).and_then(Json::as_bool).unwrap_or(false)
}

fn peers(doc: &Json) -> Result<Vec<Discovered>, String> {
    let mut out = Vec::new();
    if let Some(items) = doc.get("peers").and_then(Json::as_array) {
        for item in items {
            out.push(Discovered { name: text(item, "name")?, address: text(item, "address")? });
        }
    }
    Ok(out)
}

// A malformed line is dropped, never a panic and never a refusal: the request simply stays out.
pub fn parse_line(line: &str) -> Option<Event> {
    let doc = jsondoc::parse(line).ok()?;
    match doc.get("t").and_then(Json::as_str)? {
        "localsendpeers" => Some(Event::Peers { id: number(&doc, "id").ok()?, peers: peers(&doc).ok()?, reason: text(&doc, "reason").ok()? }),
        "localsendsent" => {
            Some(Event::Sent { id: number(&doc, "id").ok()?, ok: flag(&doc, "ok"), reason: text(&doc, "reason").ok()? })
        }
        _ => None,
    }
}

pub fn peers_line(id: u64) -> String {
    format!(r#"{{"c":"localsend","op":"peers","peer":"","paths":[],"id":{id}}}"#)
}

// Paths are absolute and resolved once by the caller from one listing.
pub fn send_line(id: u64, peer: &str, paths: &[String]) -> String {
    format!(
        r#"{{"c":"localsend","op":"send","peer":"{}","paths":{},"id":{id}}}"#,
        crate::json::escape(peer),
        crate::json::string_array(paths)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peers_and_terminal_lines_parse_and_anything_else_drops() {
        let event = parse_line(r#"{"t":"localsendpeers","id":3,"peers":[{"name":"Clean Lemon","address":"192.168.21.23"}],"reason":""}"#)
            .expect("peers parse");
        assert_eq!(
            event,
            Event::Peers { id: 3, peers: vec![Discovered { name: "Clean Lemon".into(), address: "192.168.21.23".into() }], reason: String::new() }
        );
        let event = parse_line(r#"{"t":"localsendsent","id":3,"ok":false,"reason":"Send cancelled."}"#).expect("sent parses");
        assert_eq!(event, Event::Sent { id: 3, ok: false, reason: "Send cancelled.".into() });
        // The exact terminal line jobwork.rs writes for a cancelled send job.
        let event = parse_line(r#"{"t":"localsendsent","id":7,"ok":false,"reason":"Send cancelled."}"#).expect("job line parses");
        assert_eq!(event, Event::Sent { id: 7, ok: false, reason: "Send cancelled.".into() });
        assert!(parse_line("not json").is_none());
        assert!(parse_line(r#"{"t":"transferdone","id":1}"#).is_none());
        assert!(parse_line(r#"{"t":"localsendsent","id":"x"}"#).is_none());
    }

    #[test]
    fn request_lines_carry_the_backend_s_field_shape() {
        assert_eq!(peers_line(3), r#"{"c":"localsend","op":"peers","peer":"","paths":[],"id":3}"#);
        assert_eq!(
            send_line(4, "Clean Lemon", &["/home/gm/a.txt".to_string()]),
            r#"{"c":"localsend","op":"send","peer":"Clean Lemon","paths":["/home/gm/a.txt"],"id":4}"#
        );
    }
}
