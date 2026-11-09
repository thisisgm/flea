//! Typed place operations retain the originating listing and menu confirmation.
use super::{place::refusal, place_model::*};
use flea::{jsondoc::Json, ui_logic_context_menus::build};

pub(super) enum Completion {
    List,
    Rename(String),
    Transfer { count: usize, moving: bool },
    Delete(u64, usize),
}
pub(super) enum Action {
    Replies(Vec<Json>),
    Runtime {
        plugin: String,
        request: Json,
        completion: Completion,
    },
}
fn replies(rows: Vec<Json>) -> Result<Action, String> {
    Ok(Action::Replies(rows))
}
fn array(paths: &[String]) -> Json {
    Json::Arr(paths.iter().map(string).collect())
}
fn menu(doc: &Json, ok: bool, more: &[(&str, Json)]) -> Json {
    let mut fields = vec![
        ("t", string("menuaction")),
        ("op", string(text(doc, "op"))),
        ("id", number(num(doc, "id"))),
        ("ok", Json::Bool(ok)),
    ];
    fields.extend_from_slice(more);
    object(&fields)
}

impl Model {
    pub fn command(&mut self, doc: &Json) -> Result<Action, String> {
        match text(doc, "c") {
            "list" => {
                let uri = text(doc, "path");
                let (plugin, relative) = path(uri).ok_or("invalid plugin place path")?;
                self.path = uri.into();
                self.snapshots.clear();
                self.delete_tokens.clear();
                Ok(Action::Runtime {
                    plugin: plugin.into(),
                    request: object(&[("op", string("list")), ("path", string(relative))]),
                    completion: Completion::List,
                })
            }
            "window" => replies(vec![
                self.window(num(doc, "start") as usize, num(doc, "count") as usize)
            ]),
            "sort" => {
                self.settings(doc);
                replies(self.publish(doc)?)
            }
            "paths" => {
                let mut request = doc
                    .as_object()
                    .ok_or("invalid plugin paths request")?
                    .to_vec();
                // Clipboard reads resolve the current rows; its queue checks the captured listing on reply.
                if doc.get("listing").is_none() {
                    request.push(("listing".into(), number(self.listing)));
                }
                replies(vec![object(&[
                    ("t", string("paths")),
                    ("paths", array(&self.targets(&Json::Obj(request))?)),
                ])])
            }
            "locate" => {
                let locate = |uri: &str| {
                    self.rows.iter().position(|r| {
                        format!("{}/{}", self.path.trim_end_matches('/'), r.name) == uri
                    })
                };
                let matches = doc
                    .get("paths")
                    .and_then(Json::as_array)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(|p| {
                        let uri = p.as_str()?;
                        Some(object(&[
                            ("path", string(uri)),
                            ("index", number(locate(uri)?)),
                        ]))
                    })
                    .collect();
                replies(vec![object(&[
                    ("t", string("located")),
                    ("path", string(text(doc, "path"))),
                    ("directory", string(&self.path)),
                    ("id", number(num(doc, "id"))),
                    (
                        "index",
                        number(locate(text(doc, "path")).map_or(-1, |n| n as i64)),
                    ),
                    ("matches", Json::Arr(matches)),
                    ("ok", Json::Bool(true)),
                ])])
            }
            "menuaction" => self.menu(doc),
            "rename" => {
                let source = text(doc, "path");
                let (plugin, relative) = path(source).ok_or("invalid plugin rename path")?;
                let name = text(doc, "to");
                if name.is_empty()
                    || name == "."
                    || name == ".."
                    || name.contains(['/', '\\'])
                    || name.chars().any(char::is_control)
                {
                    return Err("invalid plugin rename name".into());
                }
                let (parent, _) = source
                    .rsplit_once('/')
                    .ok_or("invalid plugin rename path")?;
                let dest = format!("{parent}/{name}");
                if self.rows.iter().any(|r| r.name == name) {
                    return Err("Destination already exists.".into());
                }
                Ok(Action::Runtime {
                    plugin: plugin.into(),
                    request: object(&[
                        ("op", string("rename")),
                        ("paths", array(&[relative.into()])),
                        ("path", string(relative)),
                        ("to", string(name)),
                        ("overwrite", Json::Bool(false)),
                    ]),
                    completion: Completion::Rename(dest),
                })
            }
            "collisions" | "mergecollisions" => {
                let paths = self.operation_paths(doc)?;
                if paths.is_empty() {
                    return Err("no plugin operation paths".into());
                }
                let dest = text(doc, "dest");
                let names: Vec<_> = paths
                    .iter()
                    .filter_map(|p| {
                        let name = p.rsplit('/').next()?;
                        (dest == self.path && self.rows.iter().any(|r| r.name == name))
                            .then(|| string(name))
                    })
                    .collect();
                replies(vec![object(&[
                    ("t", string(text(doc, "c"))),
                    ("id", number(num(doc, "id"))),
                    ("total", number(names.len())),
                    ("names", Json::Arr(names)),
                    ("mergeable", Json::Bool(false)),
                ])])
            }
            "transfer" => self.transfer(doc),
            "duplicate" | "trash" | "mkdir" | "newfile" | "bulkrename" | "link" | "undo"
            | "redo" => Err("This operation is unavailable in a plugin place.".into()),
            "thumb" | "thumbcancel" | "dirsize" | "dirsizecancel" | "cancel" | "transfercancel"
            | "fsinfo" | "formats" => replies(Vec::new()),
            _ => Err("This operation is unavailable in a plugin place.".into()),
        }
    }
    fn snapshot(&self, doc: &Json) -> Result<Snapshot, String> {
        let snapshot = self
            .snapshots
            .get(&num(doc, "id"))
            .ok_or("Plugin selection changed. Reopen the menu.")?;
        if snapshot.listing != self.listing || snapshot.path != self.path {
            return Err("Plugin selection changed. Reopen the menu.".into());
        }
        Ok(snapshot.clone())
    }
    fn menu(&mut self, doc: &Json) -> Result<Action, String> {
        let id = num(doc, "id");
        match text(doc, "op") {
            "snapshot" => {
                let paths = self.targets(doc)?;
                if id == 0 {
                    return Err("invalid plugin menu".into());
                }
                if self.snapshots.len() >= super::limits::SNAPSHOTS {
                    self.snapshots.clear();
                    self.delete_tokens.clear();
                }
                let kinds = paths
                    .iter()
                    .map(|p| {
                        number(
                            self.rows
                                .iter()
                                .find(|r| p.ends_with(&format!("/{}", r.name)))
                                .map_or(0, |r| {
                                    if r.dir {
                                        build::S_IFDIR
                                    } else {
                                        build::S_IFREG
                                    }
                                }),
                        )
                    })
                    .collect();
                self.snapshots.insert(
                    id,
                    Snapshot {
                        path: self.path.clone(),
                        paths: paths.clone(),
                        listing: self.listing,
                    },
                );
                replies(vec![menu(
                    doc,
                    true,
                    &[("count", number(paths.len())), ("kinds", Json::Arr(kinds))],
                )])
            }
            "close" => {
                self.snapshots.remove(&id);
                self.delete_tokens.remove(&id);
                replies(vec![menu(doc, true, &[])])
            }
            "prepareDelete" | "refreshDelete" => {
                let snapshot = self.snapshot(doc)?;
                self.delete_tokens.insert(id);
                replies(vec![menu(
                    doc,
                    true,
                    &[
                        ("token", number(id)),
                        ("count", number(snapshot.paths.len())),
                        ("bytes", number(0)),
                    ],
                )])
            }
            "checkDelete" => replies(vec![menu(
                doc,
                true,
                &[(
                    "valid",
                    Json::Bool(
                        self.delete_tokens.contains(&id)
                            && num(doc, "token") == id
                            && self.snapshot(doc).is_ok(),
                    ),
                )],
            )]),
            "delete" => {
                let snapshot = self.snapshot(doc)?;
                if num(doc, "token") != id || !self.delete_tokens.remove(&id) {
                    return Err("Delete confirmation expired. Review the selection again.".into());
                }
                let (plugin, _) = path(&snapshot.path).ok_or("invalid plugin place")?;
                // Only the place's own items are deleted, as a transfer only moves its owner's.
                let paths: Vec<_> = snapshot
                    .paths
                    .iter()
                    .map(|p| match path(p) {
                        Some((id, relative)) if id == plugin => Ok(relative.to_owned()),
                        _ => Err("invalid plugin delete path"),
                    })
                    .collect::<Result<_, _>>()?;
                Ok(Action::Runtime {
                    plugin: plugin.into(),
                    request: object(&[("op", string("delete")), ("paths", array(&paths))]),
                    completion: Completion::Delete(id, paths.len()),
                })
            }
            "activate" | "validate" => {
                let snapshot = self.snapshot(doc)?;
                replies(vec![menu(
                    doc,
                    true,
                    &[
                        ("action", string(text(doc, "action"))),
                        ("paths", array(&snapshot.paths)),
                        ("dest", string(text(doc, "dest"))),
                    ],
                )])
            }
            "copyTo" | "moveTo" => self.transfer(doc),
            _ => Err("This menu operation is unavailable in a plugin place.".into()),
        }
    }
    fn operation_paths(&self, doc: &Json) -> Result<Vec<String>, String> {
        let menu_id = num(doc, "menuId");
        if menu_id > 0 {
            let snapshot = self
                .snapshots
                .get(&menu_id)
                .filter(|s| s.listing == self.listing && s.path == self.path)
                .ok_or("Plugin selection changed. Reopen the menu.")?;
            return Ok(snapshot.paths.clone());
        }
        self.targets(doc)
    }
    fn transfer(&self, doc: &Json) -> Result<Action, String> {
        let paths = self.operation_paths(doc)?;
        let op = match text(doc, "op") {
            "copy" | "copyTo" => "copy",
            "move" | "moveTo" => "move",
            _ => return Err("invalid plugin transfer operation".into()),
        };
        let destination = text(doc, "dest");
        let owner = paths
            .iter()
            .filter_map(|p| path(p).map(|(id, _)| id))
            .next()
            .or_else(|| path(destination).map(|(id, _)| id))
            .ok_or("missing plugin transfer owner")?;
        let convert = |p: &str| -> Result<String, String> {
            match path(p) {
                Some((id, relative)) if id == owner => Ok(relative.into()),
                Some(_) => Err("Copy between different plugin places is unavailable.".into()),
                None if p.starts_with('/') && !p.chars().any(char::is_control) => Ok(p.into()),
                _ => Err("invalid plugin transfer path".into()),
            }
        };
        let relative = paths
            .iter()
            .map(|p| convert(p))
            .collect::<Result<Vec<_>, _>>()?;
        let dest = convert(destination)?;
        Ok(Action::Runtime {
            plugin: owner.into(),
            request: object(&[
                ("op", string(op)),
                ("paths", array(&relative)),
                ("dest", string(dest)),
                ("destinationLocal", Json::Bool(path(destination).is_none())),
                (
                    "sourceLocal",
                    Json::Bool(paths.iter().all(|p| path(p).is_none())),
                ),
                ("overwrite", Json::Bool(false)),
            ]),
            completion: Completion::Transfer {
                count: paths.len(),
                moving: op == "move",
            },
        })
    }
    pub fn complete(&mut self, doc: &Json, message: &Json, completion: Completion) -> Vec<Json> {
        if text(message, "op") == "error" {
            return vec![refusal(doc, &self.path, text(message, "message"))];
        }
        match completion {
            Completion::List => self
                .accept(message, doc)
                .unwrap_or_else(|e| vec![refusal(doc, &self.path, &e)]),
            Completion::Rename(dest) => vec![object(&[
                ("t", string("renamed")),
                ("ok", Json::Bool(true)),
                ("path", string(dest)),
            ])],
            Completion::Transfer { count: n, moving } => vec![
                object(&[
                    ("t", string("transferstarted")),
                    ("id", number(num(doc, "id"))),
                    ("n", number(n)),
                    ("moving", Json::Bool(moving)),
                ]),
                object(&[
                    ("t", string("transferdone")),
                    ("id", number(num(doc, "id"))),
                    ("ok", number(n)),
                    ("failed", number(0)),
                    ("skipped", number(0)),
                ]),
            ],
            Completion::Delete(id, n) => {
                self.snapshots.remove(&id);
                vec![menu(
                    doc,
                    true,
                    &[("deleted", number(n)), ("failed", number(0))],
                )]
            }
        }
    }
}
