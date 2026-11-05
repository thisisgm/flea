use super::{
    archivework::Work, codereq, jail, model3d, model3dcache as cache, opsreq::OpMsg, regfile,
    sandbox, thumbcache, thumbs, thumbwrite,
};
use crate::json::{escape, field_bool, field_str, field_usize};
use flea::bytename::ExactText as _;
use flea::ui_logic_model_preview::wire::{Failure, Refusal};
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(20);
const STDERR_MAX: usize = 8192;
const STDERR_CHUNK: usize = 4096;
const REASON_MAX_CHARS: usize = 1024;
pub(super) const THUMB_TABLE: &str = "flea-3d-v2";
static PARSING: super::model3dadmit::Gate = super::model3dadmit::Gate::new();

#[path = "model3dreq_worker.rs"]
mod worker;
pub(super) fn worker_entry() -> Option<i32> {
    worker::entry()
}
#[cfg(test)]
use worker::args as worker_args;

pub struct Ask {
    token: usize,
    slot: String,
    path: String,
    plate: Option<usize>,
    cancel: bool,
}

impl Ask {
    // Sample input: {"c":"model3d","slot":"look","token":4,"path":"/models/part.obj","plate":0}.
    pub fn parse(line: &str) -> Self {
        Self {
            token: field_usize(line, "token").unwrap_or(0),
            slot: field_str(line, "slot").unwrap_or_default(),
            path: field_str(line, "path").unwrap_or_default(),
            plate: field_usize(line, "plate"),
            cancel: field_bool(line, "cancel"),
        }
    }
}

pub fn spawn(ask: Ask, tx: Sender<OpMsg>) {
    let ticket = codereq::begin(&format!("model3d:{}", ask.slot));
    std::thread::spawn(move || {
        let answer = if ask.cancel {
            reply(&ask, "cancelled", None, None, &Failure::default())
        } else {
            match ensure(
                &flea::bytename::view(&ask.path),
                &cache::root(),
                ask.plate,
                &ticket,
            ) {
                Ok(data) => reply(&ask, "ready", Some(&data), None, &Failure::default()),
                Err(failure) => reply(
                    &ask,
                    "failed",
                    None,
                    Some("This file cannot be previewed."),
                    &failure,
                ),
            }
        };
        ticket.finish(answer, &tx);
    });
}

struct Ready {
    file: PathBuf,
    thumbnail: Option<PathBuf>,
    summary: String,
}

fn reply(
    ask: &Ask,
    state: &str,
    ready: Option<&Ready>,
    message: Option<&str>,
    failure: &Failure,
) -> String {
    let mut line = format!(
        r#"{{"t":"model3d","slot":"{}","token":{},"state":"{}""#,
        escape(&ask.slot),
        ask.token,
        state
    );
    if let Some(ready) = ready {
        line.push_str(&format!(
            r#", "file":"{}","thumbnail":"{}","data":{}"#,
            escape(&ready.file.exact_text()),
            escape(
                &ready
                    .thumbnail
                    .as_ref()
                    .map(|p| p.exact_text().into_owned())
                    .unwrap_or_default()
            ),
            ready.summary
        ));
    }
    if let Some(message) = message {
        line.push_str(&format!(
            r#", "message":"{}","reason":"{}""#,
            escape(message),
            escape(&failure.reason)
        ));
        if failure.refusal == Some(Refusal::TooLarge) {
            line.push_str(r#", "refusal":"too_large""#);
        }
    }
    line.push('}');
    line
}

fn drain(mut reader: impl Read) -> String {
    let mut kept = Vec::new();
    let mut chunk = [0; STDERR_CHUNK];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                kept.extend_from_slice(&chunk[..n]);
                if kept.len() > STDERR_MAX {
                    kept.drain(..kept.len() - STDERR_MAX);
                }
            }
        }
    }
    String::from_utf8_lossy(&kept)
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .chars()
        .take(REASON_MAX_CHARS)
        .collect()
}

fn ensure(
    path: &Path,
    root: &Path,
    plate: Option<usize>,
    ticket: &codereq::Ticket,
) -> Result<Ready, Failure> {
    ensure_until(path, root, plate, ticket, Instant::now() + DEADLINE, false)
}

fn ensure_until(
    path: &Path,
    root: &Path,
    plate: Option<usize>,
    ticket: &codereq::Ticket,
    deadline: Instant,
    background: bool,
) -> Result<Ready, Failure> {
    if ticket.superseded() {
        return Err("preview superseded".into());
    }
    if !path.is_absolute() {
        return Err("3D preview requires an absolute path".into());
    }
    let format = model3d::supported(&path.to_string_lossy()).ok_or("unsupported 3D format")?;
    let input = path.canonicalize().map_err(|e| e.to_string())?;
    let (_, meta) = regfile::open_if_regular_with_meta(&input, 0).map_err(|e| e.to_string())?;
    if meta.len() > model3d::MAX_INPUT as u64 {
        return Err("3D input exceeds 128 MiB".into());
    }
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(root)
        .map_err(|e| e.to_string())?;
    let key = cache::key(path, &meta);
    let dest = root.join(format!("{key}.3d"));
    if let Some(header) = cache::header(&dest) {
        return ready(root, &key, dest, plate, header);
    }
    let _guard = PARSING
        .enter(ticket, background, deadline)
        .map_err(|reason| format!("3D preview {}: {reason}", path.display()))?;
    let header = if let Some(header) = cache::header(&dest) {
        header
    } else {
        if !sandbox::available() {
            return Err("3D previews require bwrap and prlimit".into());
        }
        let exe = std::env::current_exe()
            .and_then(|p| p.canonicalize())
            .map_err(|e| e.to_string())?;
        let work = Work::new(root, "model3d").map_err(|e| e.msg)?;
        let temp = work.dir.join("model.3d");
        let output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        let inner = vec![
            exe.exact_text().into_owned(),
            "--backend".into(),
            "--3d-worker".into(),
            format.into(),
            "--".into(),
            input.exact_text().into_owned(),
        ];
        let mut argv = sandbox::wrap_readonly_extra(&inner, &[&input, &exe]);
        sandbox::add_status(&mut argv, inner.len());
        let mut jailed = jail::spawn_jailed(&argv, |cmd| {
            cmd.stdin(Stdio::null())
                .stdout(Stdio::from(output))
                .stderr(Stdio::piped());
        })
        .map_err(|e| e.to_string())?;
        let stderr = jailed
            .child
            .stderr
            .take()
            .ok_or("3D preview has no error pipe")?;
        let log = std::thread::spawn(move || drain(stderr));
        if !ticket.attach(jailed) {
            return Err("preview superseded".into());
        }
        let status = ticket
            .reap_until(Some(deadline))
            .ok_or("preview superseded")?;
        let reason = log.join().unwrap_or_default();
        let status = status.map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(worker::failure(
                status,
                if reason.is_empty() {
                    "3D parser failed".into()
                } else {
                    reason
                },
            ));
        }
        let header = cache::header(&temp).ok_or("3D parser produced no model")?;
        let (_, after) =
            regfile::open_if_regular_with_meta(&input, 0).map_err(|e| e.to_string())?;
        if !cache::same(&meta, &after) {
            return Err("3D file changed during preview".into());
        }
        if ticket.superseded() {
            return Err("preview superseded".into());
        }
        std::fs::rename(&temp, &dest).map_err(|e| e.to_string())?;
        header
    };
    ready(root, &key, dest, plate, header)
}

fn ready(
    root: &Path,
    key: &str,
    dest: PathBuf,
    plate: Option<usize>,
    header: (String, Vec<model3d::Thumbnail>),
) -> Result<Ready, Failure> {
    let (summary, thumbnails) = header;
    let data = crate::jsondoc::parse(&summary)?;
    if plate.is_some_and(|p| {
        data.get("plates")
            .and_then(|v| v.as_array())
            .is_none_or(|a| p >= a.len())
    }) {
        return Err("3D plate is out of range".into());
    }
    let thumbnail = cache::embedded(root, key, &thumbnails, plate);
    cache::evict(root);
    Ok(Ready {
        file: dest,
        thumbnail,
        summary,
    })
}

pub(super) fn thumbnail(tables: &thumbs::Tables, job: &thumbs::Job) -> thumbs::Outcome {
    let root = tables
        .cache
        .large_dir()
        .parent()
        .unwrap_or(Path::new("/"))
        .join("flea-3d");
    let ticket = codereq::begin(&format!("model3d-thumb:{}", thumbcache::uri_for(&job.path)));
    let answer = ensure_until(
        &job.path,
        &root,
        None,
        &ticket,
        Instant::now() + DEADLINE,
        true,
    );
    ticket.discard();
    let Ok(ready) = answer else {
        return thumbs::Outcome::Failed;
    };
    let Some(image) = ready.thumbnail else {
        let uri = thumbcache::uri_for(&job.path);
        thumbs::record_failure_for(
            &tables.cache,
            &uri,
            job.mtime,
            THUMB_TABLE,
            job.trash_identity.as_deref(),
        );
        return thumbs::Outcome::Failed;
    };
    let dir = tables.cache.large_dir();
    if std::fs::create_dir_all(&dir).is_err() { return thumbs::Outcome::Failed }
    let Some(temp) = thumbwrite::exclusive_temp(&dir) else { return thumbs::Outcome::Failed };
    let mime = match image.extension().and_then(|e| e.to_str()) { Some("png") => "image/png", Some("jpg") => "image/jpeg", _ => "image/qoi" };
    let mut child_job = thumbs::Job { replies: None, path: image, mime: mime.into(), mtime: job.mtime, source_size: None, trash_identity: None, trace: None };
    let source = match thumbs::run_one_in(tables,&mut child_job,&root) { thumbs::Outcome::Ready(p) => p, _ => return thumbs::discard(&temp) };
    if std::fs::copy(source,&temp).is_err() { return thumbs::discard(&temp) }
    let uri = thumbcache::uri_for(&job.path);
    thumbs::publish(
        &temp,
        &uri,
        job.mtime,
        tables
            .cache
            .large_path_for(&uri, job.trash_identity.as_deref()),
    )
}

#[cfg(test)]
#[path = "model3dreq_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "model3dreq_regression_tests.rs"]
mod regression_tests;
