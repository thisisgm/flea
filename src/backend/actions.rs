// Actions and legacy scripts are discovered only for an explicit selection request, off the loop.
use flea::bytename::ExactText as _;
use super::actionmanifest::{self, Manifest, MAX_MANIFEST};
use super::opsreq::OpMsg;
use crate::json::{escape, field_str, field_str_array, field_usize};
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::Arc;

const MAX_MANIFESTS: usize = 256;
const MAX_SCRIPTS: usize = 256;
const MAX_SELECTION: usize = 4096;

#[derive(Clone)]
pub(crate) struct Action {
    pub id: String,
    pub manifest: Manifest,
    pub script: bool,
}

fn config() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|p| !p.is_empty())
        .map(flea::bytename::exact)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        })
        .join("flea")
}

pub(crate) fn load() -> (Vec<Action>, Vec<(String, String)>) {
    load_from(&config())
}

fn load_from(root: &Path) -> (Vec<Action>, Vec<(String, String)>) {
    let mut actions = Vec::new();
    let mut errors = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root.join("actions")) {
        for (index, entry) in entries.take(MAX_MANIFESTS + 1).enumerate() {
            if index == MAX_MANIFESTS {
                errors.push(("actions".into(), "too many manifests".into()));
                break;
            }
            let Ok(entry) = entry else {
                continue;
            };
            let path = entry.path();
            if path.extension().is_none_or(|s| s != "toml") {
                continue;
            }
            let name = entry.file_name().exact_text().to_string();
            let parsed = super::regfile::open_regular(&path)
                .ok_or_else(|| "not a regular readable manifest".to_string())
                .and_then(|f| {
                    let mut text = Vec::new();
                    f.take(MAX_MANIFEST as u64 + 1)
                        .read_to_end(&mut text)
                        .map_err(|e| e.to_string())?;
                    if text.len() > MAX_MANIFEST {
                        return Err("manifest exceeds 16384 bytes".into());
                    }
                    let text =
                        String::from_utf8(text).map_err(|_| "manifest is not UTF-8".to_string())?;
                    actionmanifest::parse(&text)
                });
            match parsed {
                Ok(manifest) => actions.push(Action {
                    id: format!("action:{name}"),
                    manifest,
                    script: false,
                }),
                Err(error) => errors.push((name, error)),
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(root.join("scripts")) {
        for entry in entries.take(MAX_SCRIPTS).flatten() {
            let Ok(meta) = entry.path().symlink_metadata() else {
                continue;
            };
            if !meta.is_file() || meta.permissions().mode() & 0o111 == 0 {
                continue;
            }
            let name = entry.file_name().exact_text().to_string();
            let label = flea::bytename::view(&name)
                .file_stem()
                .unwrap_or_default()
                .exact_text()
                .to_string();
            actions.push(Action {
                id: format!("script:{name}"),
                script: true,
                manifest: Manifest {
                    name: label,
                    mime: vec!["*/*".into()],
                    selection: "many".into(),
                    mode: "background".into(),
                    run: vec![entry.path().exact_text().into()],
                },
            });
        }
    }
    actions.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name).then(a.id.cmp(&b.id)));
    errors.sort();
    (actions, errors)
}

impl Action {
    pub fn matches(&self, paths: &[String], mime: &super::mime::Db) -> bool {
        !paths.is_empty()
            && (self.manifest.selection == "many" || paths.len() == 1)
            && paths.iter().all(|p| {
                let kind = std::fs::symlink_metadata(p).ok().map(|m| {
                    if m.is_dir() {
                        "inode/directory"
                    } else {
                        mime.lookup(p).unwrap_or("application/octet-stream")
                    }
                });
                kind.is_some_and(|kind| {
                    self.manifest
                        .mime
                        .iter()
                        .any(|glob| actionmanifest::matches(glob, kind))
                })
            })
    }

    pub fn argv(&self, path: &Path) -> Vec<String> {
        let values: [(&str, &[u8]); 5] = [
            ("{path}", path.as_os_str().as_bytes()),
            (
                "{dir}",
                path.parent()
                    .unwrap_or(Path::new("/"))
                    .as_os_str()
                    .as_bytes(),
            ),
            ("{name}", path.file_name().unwrap_or_default().as_bytes()),
            ("{stem}", path.file_stem().unwrap_or_default().as_bytes()),
            ("{ext}", path.extension().unwrap_or_default().as_bytes()),
        ];
        self.manifest
            .run
            .iter()
            .map(|template| {
                // Values splice as raw bytes, so a raw name inside a longer argument still encodes as one exact text.
                let mut output: Vec<u8> = Vec::new();
                let mut rest = template.as_str();
                while let Some(at) = rest.find('{') {
                    output.extend_from_slice(&rest.as_bytes()[..at]);
                    let end = rest[at..].find('}').unwrap() + at;
                    if let Some((_, value)) = values.iter().find(|(key, _)| *key == &rest[at..=end])
                    {
                        output.extend_from_slice(value);
                    }
                    rest = &rest[end + 1..];
                }
                output.extend_from_slice(rest.as_bytes());
                if template.starts_with('{') && output.starts_with(b"-") {
                    output.splice(0..0, *b"./");
                }
                flea::bytename::path_text(Path::new(OsStr::from_bytes(&output))).into_owned()
            })
            .collect()
    }
}

pub(crate) fn request(line: String, mime: Arc<super::mime::Db>, tx: Sender<OpMsg>) {
    let preview = field_str(&line, "op").as_deref() == Some("preview");
    let ticket = preview.then(|| super::codereq::begin("action-preview"));
    std::thread::spawn(move || {
        let id = field_usize(&line, "id").unwrap_or(0);
        let op = field_str(&line, "op").unwrap_or_else(|| "list".into());
        let paths = field_str_array(&line, "paths");
        let valid = paths.len() <= MAX_SELECTION
            && paths
                .iter()
                .all(|p| flea::bytename::view(p).is_absolute() && !flea::bytename::is_refused(p));
        let (actions, errors) = load();
        let matching: Vec<_> = actions
            .into_iter()
            .filter(|a| valid && a.matches(&paths, &mime))
            .collect();
        if op == "list" {
            let rows: Vec<_> = matching
                .iter()
                .map(|a| {
                    format!(
                        r#"{{"id":"{}","name":"{}","mode":"{}","selection":"{}","script":{},"mime":[{}]}}"#,
                        escape(&a.id),
                        escape(&a.manifest.name),
                        a.manifest.mode,
                        a.manifest.selection,
                        a.script,
                        a.manifest
                            .mime
                            .iter()
                            .map(|glob| format!("\"{}\"", escape(glob)))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                })
                .collect();
            let errors: Vec<_> = errors
                .iter()
                .map(|(file, error)| {
                    format!(
                        r#"{{"file":"{}","error":"{}"}}"#,
                        escape(file),
                        escape(error)
                    )
                })
                .collect();
            let line = format!(
                r#"{{"t":"actions","id":{id},"actions":[{}],"errors":[{}],"error":"{}"}}"#,
                rows.join(","),
                errors.join(","),
                if valid {
                    ""
                } else {
                    "Actions require absolute selection paths."
                }
            );
            let _ = tx.send(OpMsg::Meta { line });
            return;
        }
        let wanted = field_str(&line, "action").unwrap_or_default();
        let result = matching
            .into_iter()
            .find(|a| a.id == wanted)
            .ok_or_else(|| "That action no longer matches the selection.".to_string());
        if preview {
            super::actionpreview::answer(&line, result, &paths, ticket.unwrap(), &tx);
        } else if op == "run" {
            let _ = tx.send(OpMsg::ActionReady {
                request: id,
                result: result.map(|a| (a, paths)),
            });
        } else {
            let _ = tx.send(OpMsg::Meta {
                line: response(id, false, "Unknown Actions request."),
            });
        }
    });
}

pub(crate) fn response(id: usize, ok: bool, error: &str) -> String {
    format!(
        r#"{{"t":"actionstarted","requestId":{id},"ok":{ok},"error":"{}"}}"#,
        escape(error)
    )
}

pub(crate) fn ready(
    out: &mut impl Write,
    ops: &mut super::opsdispatch::Ops,
    request: usize,
    result: Result<(Action, Vec<String>), String>,
) {
    match result {
        Ok((action, paths)) if action.manifest.mode == "background" => {
            let id = ops.claim_id();
            writeln!(
                out,
                r#"{{"t":"actionstarted","requestId":{request},"id":{id},"ok":true,"error":""}}"#
            )
            .ok();
            let label = action.manifest.name.clone();
            ops.jobs.submit(
                out,
                id,
                super::jobwork::Spec { kind: "action", label: &label, target: None },
                super::jobwork::Work::Action {
                    action,
                    paths,
                    selection: None,
                },
                &ops.detached,
                &ops.tx,
            );
        }
        Ok((action, paths)) if action.manifest.mode == "terminal" => {
            std::thread::spawn({
                let tx = ops.tx.clone();
                move || {
                    let mut error = String::new();
                    for path in &paths {
                        let argv = action.argv(&flea::bytename::view(path));
                        if let Err(e) = crate::terminal::open_argv(
                            flea::bytename::view(path)
                                .parent()
                                .unwrap_or(Path::new("/")),
                            &argv,
                        ) {
                            error = e.to_string();
                            break;
                        }
                    }
                    let _ = tx.send(OpMsg::Meta {
                        line: response(request, error.is_empty(), &error),
                    });
                }
            });
        }
        Ok(_) => {
            writeln!(
                out,
                "{}",
                response(
                    request,
                    false,
                    "Preview actions run only for the cursor row."
                )
            )
            .ok();
        }
        Err(error) => {
            writeln!(out, "{}", response(request, false, &error)).ok();
        }
    }
    out.flush().ok();
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;
