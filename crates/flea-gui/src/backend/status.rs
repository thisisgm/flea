//! Status facts use the backend's existing fsinfo wire; this module never probes a filesystem itself.
use super::{Backend, Control, Failure};
use flea::jsondoc::Json;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filesystem {
    pub path: String,
    pub name: String,
    pub free: u64,
}

impl Filesystem {
    // Input: {"path":"/work","fs":"btrfs","free":442000000000}.
    pub(super) fn parse(document: &Json) -> Result<Self, String> {
        let text = |key| {
            document
                .get(key)
                .and_then(Json::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("{key} must be a string"))
        };
        let free = match document.get("free") {
            Some(Json::Num(value)) => value
                .parse()
                .map_err(|_| "free must be an unsigned integer")?,
            _ => return Err("free must be an unsigned integer".into()),
        };
        Ok(Self {
            path: text("path")?,
            name: text("fs")?,
            free,
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
    }
}
