// Remote discovery reads only configuration; the send producer owns all remote I/O.
use super::opsdispatch::Ops;
use super::opsreq::OpMsg;
use crate::json::{escape, field_str, field_str_array, field_usize};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

// Discovery reads only local configuration, so a slower answer means rclone is stuck.
const LIST_DEADLINE: Duration = Duration::from_secs(2);

// Sample input: listremotes stdout "gdrive:\nnas-crypt:\n", one configured remote per line ending in a colon.
pub(super) fn remotes(cancel: &AtomicBool) -> Result<Vec<String>, String> {
    let mut log = super::rclonelog::Log::default();
    let output = super::rcloneproc::run(
        &["listremotes".into(), "--use-json-log".into()],
        cancel,
        Some(LIST_DEADLINE),
        true,
        |line| {
            log.line(line);
        },
    )?;
    if !output.success {
        return Err(log.explain("Could not list rclone remotes.".into()));
    }
    let mut remotes = Vec::new();
    for line in output.text.lines() {
        if let Some(name) = line.strip_suffix(':').filter(|name| remote_name(name)) {
            if !remotes.iter().any(|held| held == name) {
                remotes.push(name.to_string());
            }
        }
    }
    Ok(remotes)
}

fn remote_name(name: &str) -> bool {
    !name.is_empty()
        && !name
            .chars()
            .any(|c| c.is_control() || matches!(c, ':' | '/' | '\\' | ','))
}

pub(super) fn request(out: &mut impl Write, ops: &mut Ops, line: &str) {
    let request = field_usize(line, "id").unwrap_or(0);
    let op = field_str(line, "op").unwrap_or_default();
    if op == "remotes" {
        let tx = ops.tx.clone();
        std::thread::spawn(move || {
            let names = remotes(&AtomicBool::new(false)).unwrap_or_default();
            let values: Vec<_> = names
                .iter()
                .map(|name| format!("\"{}\"", escape(name)))
                .collect();
            let _ = tx.send(OpMsg::Meta {
                line: format!(
                    r#"{{"t":"rcloneremotes","id":{},"visible":{},"remotes":[{}]}}"#,
                    request,
                    !names.is_empty(),
                    values.join(",")
                ),
            });
        });
        return;
    }
    let remote = field_str(line, "remote").unwrap_or_default();
    let folder = field_str(line, "folder").unwrap_or_default();
    let paths = field_str_array(line, "paths");
    let error = if op != "send" {
        "Unknown rclone request.".to_string()
    } else if !remote_name(&remote) || folder.contains('\0') {
        "Choose a configured rclone remote and folder.".into()
    } else if folder.split('/').any(|part| part == "..") {
        "An rclone folder cannot contain a .. component.".into()
    } else if paths.is_empty() {
        "Choose at least one item to send with rclone.".into()
    } else {
        paths.iter().find_map(|p| path_error(p)).unwrap_or_default()
    };
    if !error.is_empty() {
        writeln!(
            out,
            r#"{{"t":"rclonestarted","requestId":{},"ok":false,"error":"{}"}}"#,
            request,
            escape(&error)
        )
        .ok();
        out.flush().ok();
        return;
    }
    let id = ops.claim_id();
    writeln!(
        out,
        r#"{{"t":"rclonestarted","requestId":{},"id":{},"ok":true,"error":""}}"#,
        request, id
    )
    .ok();
    ops.jobs.submit(
        out,
        id,
        super::jobwork::Spec {
            kind: "send",
            label: "Sending",
            target: None,
        },
        super::jobwork::Work::Rclone(super::rclonerun::Send {
            remote,
            folder,
            paths,
            request,
            selection: None,
        }),
        &ops.detached,
        &ops.tx,
    );
}

// The sentence that refuses one selection path, naming it; None for an absolute path with a file name.
fn path_error(path: &str) -> Option<String> {
    if path.contains('\0') {
        Some(format!(
            "rclone cannot send {}, it contains a NUL byte.",
            path
        ))
    } else if !Path::new(path).is_absolute() {
        Some(format!(
            "rclone requires an absolute selection path, not {}.",
            path
        ))
    } else if Path::new(path).file_name().is_none() {
        Some(format!("rclone cannot send {}, it has no file name.", path))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rclone_accepts_remote_labels_without_connection_string_overrides() {
        assert!(remote_name("nas-crypt"));
        assert!(remote_name("my drive"));
        for name in [
            "",
            ":local:",
            "gdrive,token=value",
            "folder/name",
            "bad\nname",
        ] {
            assert!(!remote_name(name), "{name:?}");
        }
    }
}
