use crate::jsondoc::{self, Json};
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
extern "C" {
    fn geteuid() -> u32;
}

#[derive(Clone, Debug)]
pub struct Target {
    pub id: String,
    pub label: String,
    pub remote: String,
    pub root: String,
}
pub fn relative(raw: &str) -> bool {
    raw.is_empty()
        || (raw.len() <= 4096
            && !raw.chars().any(char::is_control)
            && !raw.contains([':', '\\'])
            && raw
                .split('/')
                .all(|p| !p.is_empty() && p != "." && p != ".."))
}
fn name(s: &str) -> bool {
    s.bytes()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
        && s.len() <= 128
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}
pub fn parse(body: &str) -> Result<Vec<Target>, String> {
    let value = jsondoc::parse(body)?;
    let items = value
        .get("targets")
        .and_then(Json::as_array)
        .ok_or("Expected targets array")?;
    if items.len() > 32 {
        return Err("Too many cloud targets".into());
    }
    let mut out: Vec<Target> = Vec::new();
    for item in items {
        let field = |key| {
            item.get(key)
                .and_then(Json::as_str)
                .ok_or_else(|| format!("Missing target {key}"))
        };
        let (id, label, remote) = (field("id")?, field("label")?, field("remote")?);
        let root = match item.get("root") {
            None => "",
            Some(value) => value.as_str().ok_or("Target root must be text")?,
        };
        if !name(id)
            || !name(remote)
            || label.is_empty()
            || label.len() > 120
            || label.chars().any(char::is_control)
            || !relative(root)
            || out.iter().any(|t| t.id == id)
        {
            return Err("Invalid or duplicate cloud target".into());
        }
        out.push(Target {
            id: id.into(),
            label: label.into(),
            remote: remote.into(),
            root: root.into(),
        });
    }
    Ok(out)
}
pub fn load() -> Result<Vec<Target>, String> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or("Configuration directory unavailable")?;
    if !base.is_absolute() {
        return Err("Configuration directory must be absolute".into());
    }
    let file = crate::backend::regfile::open_if_regular(
        &base.join("flea/cloud-targets.json"),
        crate::oflags::O_NOFOLLOW,
    )
    .map_err(|_| "Configure a target in flea/cloud-targets.json first")?;
    let meta = file
        .metadata()
        .map_err(|_| "Cannot inspect cloud targets")?;
    if meta.uid() != unsafe { geteuid() } || meta.mode() & 0o022 != 0 {
        return Err("Cloud targets must be user-owned and not writable by others".into());
    }
    let mut body = String::new();
    file.take(65537)
        .read_to_string(&mut body)
        .map_err(|_| "Cannot read cloud targets")?;
    if body.len() > 65536 {
        return Err("Cloud targets file too large".into());
    }
    parse(&body)
}
pub fn destination(t: &Target, folder: &str, basename: &str) -> Result<(String, String), String> {
    if !relative(folder) || !relative(basename) || basename.contains('/') || basename.is_empty() {
        return Err("Use a relative folder without traversal, colons or control characters".into());
    }
    let parent = [t.root.as_str(), folder]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("/");
    let parent = format!("{}:{}", t.remote, parent);
    let dest = format!(
        "{}{basename}",
        if parent.ends_with(':') {
            parent.clone()
        } else {
            format!("{parent}/")
        }
    );
    Ok((parent, dest))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_inline_backends_or_path_escape() {
        for s in ["/x", "../x", "x/../y", "x//y", "x:", "x\\y", "x\ny", "."] {
            assert!(!relative(s));
        }
        let t = parse(
            r#"{"targets":[{"id":"drive","label":"Drive","remote":"gdrive","root":"test"}]}"#,
        )
        .unwrap();
        assert_eq!(
            destination(&t[0], "Photos", "folder").unwrap().1,
            "gdrive:test/Photos/folder"
        );
        assert!(parse(r#"{"targets":[{"id":"a","label":"X","remote":":local:"}]}"#).is_err());
        assert!(
            parse(r#"{"targets":[{"id":"a","label":"X","remote":"x","root":"../x"}]}"#).is_err()
        );
    }
}
