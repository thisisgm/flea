// A preview action sees one cursor file in the existing read-only jail and returns code-renderer text.
use flea::bytename::ExactText as _;
use super::actions::Action;
use super::codereq::Ticket;
use super::opsreq::OpMsg;
use crate::json::{escape, field_usize};
use std::io::Read;
use std::process::Stdio;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

const LIMIT: usize = 1_048_576;
const TIMEOUT: Duration = Duration::from_secs(5);
const OUTPUT_WAIT: Duration = Duration::from_millis(200);

pub(crate) fn answer(
    line: &str,
    action: Result<Action, String>,
    paths: &[String],
    ticket: Ticket,
    tx: &Sender<OpMsg>,
) {
    let id = field_usize(line, "id").unwrap_or(0);
    let result = action.and_then(|action| {
        if action.manifest.mode != "preview" || paths.len() != 1 {
            return Err("Preview actions require only the cursor row.".into());
        }
        if !super::sandbox::available() {
            return Err("Preview sandbox unavailable.".into());
        }
        if ticket.superseded() {
            return Err("Preview superseded.".into());
        }
        let path = flea::bytename::view(&paths[0]);
        let path = &*path;
        let metadata = path.symlink_metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() {
            return Err("Preview actions require a regular cursor file.".into());
        }
        let argv = action.argv(path);
        let invoked =
            program(&argv[0]).ok_or_else(|| "Preview program is unavailable.".to_string())?;
        let executable = invoked
            .canonicalize()
            .map_err(|_| "Preview program is unavailable.".to_string())?;
        let mut argv = argv;
        argv[0] = executable.exact_text().into();
        let mut full = super::sandbox::wrap_readonly_extra(&argv, &[path, &executable]);
        super::sandbox::add_status(&mut full, argv.len());
        // bwrap cannot bind onto a symlink, so it runs the resolved file under the alias name instead.
        if invoked != executable {
            let at = full.len() - argv.len();
            full.splice(at..at, ["--argv0".to_string(), invoked.exact_text().into()]);
        }
        let mut jailed = super::jail::spawn_jailed(&full, |c| {
            c.stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
        })
        .map_err(|e| e.to_string())?;
        let stdout = jailed
            .child
            .stdout
            .take()
            .ok_or_else(|| "Preview output unavailable.".to_string())?;
        if !ticket.attach(jailed) {
            return Err("Preview superseded.".into());
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = stdout
                .take(LIMIT as u64 + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            let _ = sender.send(result);
        });
        let status = ticket
            .reap_until(Some(Instant::now() + TIMEOUT))
            .ok_or_else(|| "Preview superseded.".to_string())?
            .map_err(|e| e.to_string())?;
        let bytes = receiver
            .recv_timeout(OUTPUT_WAIT)
            .map_err(|_| "Preview output timed out.".to_string())?
            .map_err(|e| e.to_string())?;
        // A previewer the full pipe killed exits unsuccessfully, so the cap is the reason it failed.
        if bytes.len() > LIMIT {
            return Err("Preview output exceeds 1048576 bytes.".into());
        }
        if !status.success() {
            return Err("Preview action failed.".into());
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    });
    let (text, error) = match result {
        Ok(text) => (text, String::new()),
        Err(error) => (String::new(), error),
    };
    let reply = format!(
        r#"{{"t":"actionpreview","id":{id},"ok":{},"text":"{}","error":"{}"}}"#,
        error.is_empty(),
        escape(&text),
        escape(&error)
    );
    ticket.finish(reply, tx);
}

// A multicall tool picks its behaviour from argv[0], so only the directory resolves and the alias name stays.
fn program(name: &str) -> Option<std::path::PathBuf> {
    let candidate = if flea::bytename::view(name).is_absolute() {
        flea::bytename::exact(name)
    } else {
        std::env::split_paths(&std::env::var_os("PATH")?)
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.join(name))
            .find(|p| p.is_file())?
    };
    if !candidate.is_file() {
        return None;
    }
    Some(
        candidate
            .parent()?
            .canonicalize()
            .ok()?
            .join(candidate.file_name()?),
    )
}

#[cfg(test)]
#[path = "actionpreview_tests.rs"]
mod tests;
