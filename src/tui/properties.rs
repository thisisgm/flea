// The same entry/contents distinction as the GUI Properties card.
use super::wire::{count, flag, text};
use crate::jsondoc::Json;

pub(super) fn rows(value: &Json) -> Vec<String> {
    let size = if flag(value, "directory") { "Not calculated".into() }
               else { super::render::bytes(count(value, "bytes")) };
    let mut facts = vec![
        text(value, "path").into(),
        format!("Kind · {}", text(value, "kind")),
        format!("Size · {}", size),
        format!("Modified · {}", super::render::modified(value.get("modified").and_then(Json::as_f64).unwrap_or(0.0) as i64)),
        format!("Permissions · {}", text(value, "mode")),
        format!("Owner · {} ({})", text(value, "owner"), count(value, "uid")),
        format!("Group · {}", count(value, "gid")),
    ];
    if flag(value, "symlink") { facts.push(format!("Target · {}", text(value, "target"))); }
    match text(value, "filesystem") {
        "fuse.rclone" => {
            facts.push("Storage · rclone mount".into());
            facts.push("Upload status · Unavailable. Files may still be uploading after a copy finishes.".into());
        }
        "" => (),
        other => facts.push(format!("Storage · {}", other)),
    }
    facts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_size_and_upload_state_are_not_guessed() {
        let value = crate::jsondoc::parse(r#"{"directory":true,"bytes":0,"filesystem":"fuse.rclone"}"#).unwrap();
        let result = rows(&value);
        assert!(result.contains(&"Size · Not calculated".into()));
        assert!(result.contains(&"Storage · rclone mount".into()));
        assert!(result.iter().any(|line| line.starts_with("Upload status · Unavailable.")));
    }

    #[test]
    fn empty_file_keeps_zero_without_cloud_claims() {
        let value = crate::jsondoc::parse(r#"{"directory":false,"bytes":0,"filesystem":"ext4"}"#).unwrap();
        let result = rows(&value);
        assert!(result.contains(&format!("Size · {}", super::super::render::bytes(0))));
        assert!(!result.iter().any(|line| line.starts_with("Upload status")));
    }
}
