//! Per-folder sort and view memory in the existing ui.json schema, written only by a user choice.
use super::history::trim_slash;
use flea::{json::escape, jsondoc::Json};

mod writer;
pub use writer::Writer;

pub const MAX: usize = 500;
pub const ORDERS: [&str; 4] = ["name", "size", "mtime", "kind"];
pub const VIEWS: [&str; 3] = ["list", "columns", "grid"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Order {
    pub key: &'static str,
    pub desc: bool,
}

impl Default for Order {
    fn default() -> Self {
        Self {
            key: "name",
            desc: false,
        }
    }
}

// Settings stores Modified as "date" while the backend spells it "mtime".
pub fn live_key(key: &str) -> Option<&'static str> {
    let key = if key == "date" { "mtime" } else { key };
    ORDERS.iter().copied().find(|order| *order == key)
}

pub fn stored_key(key: &str) -> &str {
    if key == "mtime" {
        "date"
    } else {
        key
    }
}

fn order_of(value: &Json) -> Option<Order> {
    Some(Order {
        key: live_key(value.get("key")?.as_str()?)?,
        desc: value.get("reverse").and_then(Json::as_bool) == Some(true),
    })
}

fn flag(data: &Json, key: &str, absent: bool) -> bool {
    data.get(key).and_then(Json::as_bool).unwrap_or(absent)
}

#[derive(Clone, Debug, Default)]
pub struct Folders {
    sorts: Vec<(String, Order)>,
    views: Vec<(String, &'static str)>,
    pub remember_sort: bool,
    pub remember_view: bool,
    pub global: Order,
    pub newest: Vec<String>,
    pub folders_first: bool,
    pub group_by_kind: bool,
    pub hidden_last: bool,
    pub hidden: bool,
}

impl Folders {
    pub fn from_settings(data: &Json, newest: Vec<String>) -> Self {
        let map = |key| data.get(key).and_then(Json::as_object).unwrap_or(&[]);
        let global = data.get("sort").map_or(Order::default(), |sort| Order {
            key: sort
                .get("key")
                .and_then(Json::as_str)
                .and_then(live_key)
                .unwrap_or("name"),
            desc: sort.get("reverse").and_then(Json::as_bool) == Some(true),
        });
        Self {
            sorts: map("folderSorts")
                .iter()
                .filter_map(|(path, value)| Some((path.clone(), order_of(value)?)))
                .collect(),
            views: map("folderViews")
                .iter()
                .filter_map(|(path, value)| {
                    let view = value.as_str()?;
                    Some((path.clone(), VIEWS.iter().copied().find(|v| *v == view)?))
                })
                .collect(),
            remember_sort: flag(data, "rememberSort", true),
            remember_view: flag(data, "rememberViewPerFolder", false),
            global,
            newest,
            folders_first: flag(data, "foldersFirst", true),
            group_by_kind: flag(data, "groupByKind", false),
            hidden_last: flag(data, "hiddenLast", false),
            hidden: flag(data, "hidden", false),
        }
    }

    pub fn get(&self, path: &str) -> Option<Order> {
        self.sorts
            .iter()
            .find(|(own, _)| own == path)
            .map(|(_, order)| *order)
    }

    pub fn has(&self, path: &str) -> bool {
        self.remember_sort && self.get(path).is_some()
    }

    pub fn sort_count(&self) -> usize {
        self.sorts.len()
    }

    // Downloads, Pictures and Videos open newest first until a user order replaces it.
    pub fn default_order(&self, path: &str) -> Option<Order> {
        let folder = path.trim_end_matches('/');
        self.newest
            .iter()
            .any(|own| own == folder)
            .then_some(Order {
                key: "mtime",
                desc: true,
            })
    }

    pub fn order_for(&self, path: &str) -> Order {
        if self.remember_sort {
            if let Some(own) = self.get(path).or_else(|| self.default_order(path)) {
                return own;
            }
        }
        self.global
    }

    // Columns pass keep: kind is not supported by peeks, and name ascending sends no fields.
    pub fn peek_order(&self, path: &str) -> Option<Order> {
        let own = if self.remember_sort {
            self.get(path).or_else(|| self.default_order(path))
        } else {
            Some(self.global).filter(|own| *own != Order::default())
        };
        own.filter(|own| own.key != "kind")
    }

    // A re-sort moves its folder to the most recent end; past the cap the oldest entries go.
    pub fn remember(&mut self, path: &str, order: Order) -> Option<String> {
        if !self.remember_sort || path.is_empty() {
            return None;
        }
        self.sorts.retain(|(own, _)| own != path);
        self.sorts.push((path.into(), order));
        if self.sorts.len() > MAX {
            self.sorts.remove(0);
        }
        Some(format!(
            "{{\"folderSorts\":{{\"{}\":{{\"key\":\"{}\",\"reverse\":{}}}}}}}",
            escape(path),
            stored_key(order.key),
            order.desc
        ))
    }

    pub fn forget(&mut self, path: &str) -> Option<String> {
        if !self.remember_sort || path.is_empty() {
            return None;
        }
        self.sorts.retain(|(own, _)| own != path);
        Some(format!("{{\"folderSorts\":{{\"{}\":null}}}}", escape(path)))
    }

    /// A folder holds its own view only while views are remembered per folder.
    pub fn owns_view(&self, path: &str) -> bool {
        self.remember_view && self.views.iter().any(|(own, _)| own == path)
    }

    pub fn view_for(&self, path: &str, fallback: &'static str) -> &'static str {
        if self.remember_view {
            if let Some((_, view)) = self.views.iter().find(|(own, _)| own == path) {
                return view;
            }
        }
        VIEWS
            .iter()
            .copied()
            .find(|view| *view == fallback)
            .unwrap_or("list")
    }

    pub fn remember_view(&mut self, path: &str, view: &'static str) -> Option<String> {
        if !self.remember_view || path.is_empty() || !VIEWS.contains(&view) {
            return None;
        }
        self.views.retain(|(own, _)| own != path);
        self.views.push((path.into(), view));
        if self.views.len() > MAX {
            self.views.remove(0);
        }
        Some(format!(
            "{{\"folderViews\":{{\"{}\":\"{view}\"}}}}",
            escape(path)
        ))
    }
}

pub(crate) fn user_dir(line: &str, home: &str) -> Option<(String, Option<String>)> {
    let line = line.trim_start_matches([' ', '\t']);
    let rest = line.strip_prefix("XDG_")?;
    let cut = rest.find("_DIR")?;
    let role = &rest[..cut];
    if role.is_empty() || !role.bytes().all(|b| b.is_ascii_uppercase()) {
        return None;
    }
    let value = rest[cut + 4..]
        .trim_start_matches([' ', '\t'])
        .strip_prefix('=')?
        .trim_start_matches([' ', '\t']);
    let Some(body) = value.strip_prefix('"') else {
        return Some((role.into(), None));
    };
    let (mut path, body) = match body.strip_prefix("$HOME") {
        Some(after) if after.starts_with('/') || after.starts_with('"') => (home.to_owned(), after),
        _ => (String::new(), body),
    };
    let mut chars = body.chars();
    let mut closed = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => path.extend(chars.next()),
            '"' => {
                closed = true;
                break;
            }
            c => path.push(c),
        }
    }
    let path = path.trim_end_matches('/').to_owned();
    Some((
        role.into(),
        (closed && path.starts_with('/')).then_some(path),
    ))
}

// Sample input: XDG_DOWNLOAD_DIR="$HOME/Inbox"; absent PICTURES and VIDEOS use their conventional names.
pub fn newest_paths(body: &str, home: &str) -> Vec<String> {
    if home.is_empty() {
        return Vec::new();
    }
    let mut configured: Vec<(String, String)> = Vec::new();
    for (role, path) in body.lines().filter_map(|line| user_dir(line, home)) {
        if let Some(path) = path {
            configured.retain(|(own, _)| *own != role);
            configured.push((role, path));
        }
    }
    let mut paths = Vec::new();
    for (role, name) in [
        ("DOWNLOAD", "Downloads"),
        ("PICTURES", "Pictures"),
        ("VIDEOS", "Videos"),
    ] {
        let path = configured
            .iter()
            .find(|(own, _)| own == role)
            .map_or_else(|| format!("{home}/{name}"), |(_, path)| path.clone());
        if path != trim_slash(home) && !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}

#[cfg(test)]
mod window_tests {
    use super::*;
    use crate::{
        backend::{Backend, Event},
        state::tests::shell_with,
        window::navigate::Step,
    };
    use flea::protocol::{Reply, RowFacts};
    use gpui::{AppContext, TestAppContext};

    #[gpui::test]
    fn m9_g1rows_05_shell_browsing_and_refused_forget_never_write(cx: &mut TestAppContext) {
        let (backend, _requests) = Backend::test_channel();
        let cx = cx.add_empty_window();
        let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
        cx.update(|window, cx| {
            entity.update(cx, |shell, cx| {
                let data = flea::uistate::from_file(
                    r#"{"folderSorts":{"/files":{"key":"size","reverse":true},"/B":{"key":"kind","reverse":false}}}"#,
                );
                *shell.panes[0].as_ref().unwrap().nav.folders.borrow_mut() =
                    Folders::from_settings(&data, Vec::new());
                assert_eq!(shell.step(0, Step::Open("/B".into()), window, cx), Ok(true));
                assert_eq!(
                    shell.step(0, Step::ForgetSort, window, cx),
                    Err(crate::navigation::Refusal::Busy)
                );
                let host = shell.panes[0].as_ref().unwrap();
                assert_eq!(host.nav.writer.borrow().sort_writes(), 0);
                assert!(host.nav.folders.borrow().has("/files"));
                for reply in [
                    Reply::Listed {
                        path: "/B".into(),
                        total: 1,
                    },
                    Reply::Rows {
                        start: 0,
                        rows: vec![RowFacts::default()],
                        listing: 3,
                    },
                ] {
                    shell.receive(0, Event::Reply(reply), window, cx);
                }
                let host = shell.panes[0].as_ref().unwrap();
                assert_eq!(host.path, "/B");
                assert_eq!(host.nav.order, Order { key: "kind", desc: false });
                assert_eq!(shell.step(0, Step::Open("/files".into()), window, cx), Ok(true));
                for reply in [
                    Reply::Listed { path: "/files".into(), total: 1 },
                    Reply::Rows { start: 0, rows: vec![RowFacts::default()], listing: 4 },
                ] {
                    shell.receive(0, Event::Reply(reply), window, cx);
                }
                let host = shell.panes[0].as_ref().unwrap();
                assert_eq!(host.path, "/files");
                assert_eq!(host.nav.order, Order { key: "size", desc: true });
                assert_eq!(host.nav.writer.borrow().sort_writes(), 0);
                host.nav.folders.borrow_mut().remember_sort = false;
                assert_eq!(shell.step(0, Step::ForgetSort, window, cx), Ok(true));
                assert_eq!(
                    shell.panes[0].as_ref().unwrap().nav.writer.borrow().sort_writes(),
                    0
                );
            })
        });
    }
}

#[cfg(test)]
#[path = "folder_sorts_tests.rs"]
mod folder_sorts_tests;
