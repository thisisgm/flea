//! Status facts use the backend's existing fsinfo wire; this module never probes a filesystem itself.
use super::{Backend, Control, Failure};
use flea::jsondoc::Json;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filesystem {
    pub path: String,
    pub name: String,
    pub free: u64,
    /// The backend's storage class for the folder: network, phone, usb, or empty for local.
    pub class: String,
    pub tagable: Option<bool>,
}

impl Filesystem {
    // Input: {"path":"/work","fs":"btrfs","free":442000000000,"class":"usb"}; no name with -1 or no free is unknown.
    pub(super) fn parse(document: &Json) -> Result<Self, String> {
        let text = |key| {
            document
                .get(key)
                .and_then(Json::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("{key} must be a string"))
        };
        let name = match document.get("fs") {
            Some(_) => text("fs")?,
            None => String::new(),
        };
        let class = match document.get("class") {
            Some(_) => text("class")?,
            None => String::new(),
        };
        let free = match document.get("free") {
            Some(Json::Num(value)) => match value.parse() {
                Ok(free) => free,
                Err(_) if name.is_empty() && value.parse::<i64>().is_ok_and(|free| free < 0) => 0,
                Err(_) => return Err("free must be an unsigned integer".into()),
            },
            None if name.is_empty() => 0,
            _ => return Err("free must be an unsigned integer".into()),
        };
        Ok(Self {
            path: text("path")?,
            name,
            free,
            class,
            tagable: document.get("tagable").and_then(Json::as_bool),
        })
    }
}

impl Backend {
    pub fn fsinfo(&self) -> Result<(), Failure> {
        self.send(Control::Line("{\"c\":\"fsinfo\"}".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn u05_fsinfo_preserves_paths_and_refuses_malformed_facts() {
        let parse = |s| Filesystem::parse(&flea::jsondoc::parse(s).unwrap());
        let info = parse(r#"{"path":"/a\nb","fs":"btrfs","free":442000000000}"#).unwrap();
        assert_eq!(info.path, "/a\nb");
        assert_eq!(info.free, 442_000_000_000);
        assert!(parse(r#"{"path":"/a","fs":"btrfs","free":-1}"#).is_err());
        assert!(parse(r#"{"path":"/a","fs":false,"free":0}"#).is_err());
        for nothing in [
            r#"{"path":"/a","fs":"","free":-1}"#,
            r#"{"path":"/a","fs":"","free":0}"#,
            r#"{"path":"/a","fs":""}"#,
        ] {
            let info = parse(nothing).expect("an answer with nothing still answers");
            assert_eq!((info.name.as_str(), info.free), ("", 0));
        }
    }
}
