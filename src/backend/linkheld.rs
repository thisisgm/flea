//! Source identities captured at Copy and checked again by the journaled link worker.
use flea::bytename::ExactText as _;
use super::{opsreq::OpMsg, undo::ItemIdentity};
use crate::jsondoc::Json;
use std::{path::PathBuf, sync::mpsc::Sender};

#[derive(Clone)]
pub struct Identity {
    pub path: PathBuf,
    pub item: ItemIdentity,
}
pub fn capture(paths: &[String]) -> Result<Vec<Identity>, String> {
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    paths
        .iter()
        .map(|path| {
            let path = flea::bytename::exact(path);
            if !path.is_absolute() {
                return Err("link sources must be absolute paths".into());
            }
            let item = ItemIdentity::inspect_in(&path, &body).map_err(|e| e.msg)?;
            Ok(Identity { path, item })
        })
        .collect()
}
pub fn encode(held: &[Identity]) -> Json {
    Json::Arr(
        held.iter()
            .map(|held| {
                let (dev, ino, kind) = held.item.parts();
                Json::Obj(vec![
                    (
                        "path".into(),
                        Json::Str(held.path.exact_text().into_owned()),
                    ),
                    ("dev".into(), Json::Str(dev.to_string())),
                    ("ino".into(), Json::Str(ino.to_string())),
                    ("kind".into(), Json::Str(kind.to_string())),
                    (
                        "born".into(),
                        held.item.born().map_or(Json::Null, |(s, ns)| {
                            Json::Arr(vec![Json::Str(s.to_string()), Json::Str(ns.to_string())])
                        }),
                    ),
                ])
            })
            .collect(),
    )
}
pub fn decode(value: &Json) -> Result<Vec<Identity>, String> {
    let invalid = || "the held link source identity is invalid".to_string();
    value
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .map(|v| {
            let path =
                flea::bytename::exact(v.get("path").and_then(Json::as_str).ok_or_else(invalid)?);
            let num = |key| {
                v.get(key)
                    .and_then(Json::as_str)
                    .and_then(|s| s.parse::<u64>().ok())
                    .ok_or_else(invalid)
            };
            let (dev, ino, kind) = (num("dev")?, num("ino")?, num("kind")?);
            if !path.is_absolute()
                || !matches!(
                    kind,
                    0o100000 | 0o040000 | 0o120000 | 0o010000 | 0o140000 | 0o020000 | 0o060000
                )
            {
                return Err(invalid());
            }
            let born = match v.get("born") {
                Some(Json::Null) => None,
                Some(Json::Arr(parts)) if parts.len() == 2 => {
                    let seconds = parts[0]
                        .as_str()
                        .and_then(|s| s.parse::<u64>().ok())
                        .ok_or_else(invalid)?;
                    let nanos = parts[1]
                        .as_str()
                        .and_then(|s| s.parse::<u32>().ok())
                        .filter(|ns| *ns < 1_000_000_000)
                        .ok_or_else(invalid)?;
                    Some((seconds, nanos))
                }
                _ => return Err(invalid()),
            };
            Ok(Identity {
                path,
                item: ItemIdentity::from_parts(dev, ino, kind as u32, 0, (0, 0), (0, 0), born),
            })
        })
        .collect()
}
pub fn matches(held: Option<&[Identity]>, source: &std::path::Path) -> bool {
    held.map_or(true, |held| {
        held.iter()
            .find(|h| h.path == source)
            .is_some_and(|h| ItemIdentity::inspect(source).is_ok_and(|now| h.item.same_item(&now)))
    })
}
pub fn paths(replies: Sender<OpMsg>, paths: Vec<String>, generation: u64, id: usize) {
    std::thread::spawn(move || {
        let line = match capture(&paths) {
            Ok(held) => {
                let mut line = super::proto::paths_line(&paths);
                line.insert_str(
                    line.len() - 1,
                    &format!(
                        ",\"id\":{id},\"held\":{}",
                        crate::jsondoc::render(&encode(&held)).replace('\n', "")
                    ),
                );
                super::rowguard::stamped(line, generation)
            }
            Err(msg) => {
                let mut line = super::proto::error_line(&crate::error::FleaError {
                    where_: "paths".into(),
                    path: String::new(),
                    msg,
                });
                line.insert_str(line.len() - 1, &format!(",\"id\":{id}"));
                super::rowguard::stamped(line, generation)
            }
        };
        let _ = replies.send(OpMsg::Meta { line });
    });
}

pub fn request(
    out: &mut impl std::io::Write,
    replies: Sender<OpMsg>,
    paths: Vec<String>,
    generation: u64,
    hold: bool,
    id: usize,
) {
    if hold {
        self::paths(replies, paths, generation, id);
    } else {
        let _ = writeln!(out, "{}", super::proto::paths_line(&paths));
        let _ = out.flush();
    }
}

pub fn start(
    out: &mut impl std::io::Write,
    ops: &mut super::opsdispatch::Ops,
    held: Option<Json>,
    op: &str,
    paths: Vec<String>,
    dest: &str,
    collide: super::collide::Ask,
) {
    match held.as_ref().map(decode).transpose() {
        Ok(None) => super::opsdispatch::start_link(out, ops, op, paths, dest, collide),
        Ok(Some(held)) => {
            super::opsdispatch::start_link_held(out, ops, op, paths, dest, collide, Some(held))
        }
        Err(msg) => {
            let _ = writeln!(
                out,
                "{}",
                super::proto::error_line(&crate::error::FleaError {
                    where_: "link".into(),
                    path: dest.into(),
                    msg
                })
            );
            let _ = out.flush();
        }
    }
}

pub fn target(ops: &mut super::opsdispatch::Ops, path: &str, id: usize, expected: Option<String>) {
    let Some(expected) = expected else {
        return super::opsdispatch::start_link_target(ops, path, id);
    };
    let path = path.to_owned();
    let replies = ops.tx.clone();
    std::thread::spawn(move || {
        let mut out = Vec::new();
        super::opsdispatch::do_link_target_held(&mut out, &path, id, Some(&expected));
        let _ = replies.send(OpMsg::Meta {
            line: String::from_utf8_lossy(&out).trim_end().into(),
        });
    });
}

#[cfg(test)]
#[path = "linkheld_tests.rs"]
mod tests;
