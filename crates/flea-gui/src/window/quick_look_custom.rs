//! Custom preview policy, independent from the listing cursor and its row numbers.
use super::{popout::Popout, token};
use crate::{
    backend::Event,
    g2::pv1::custom::{Custom, Item, Options},
    state::Shell,
};
use flea::{
    json::escape,
    jsondoc::Json,
    protocol::RowFacts,
    ui_logic_pv1::{
        kinds::{self, State},
        meta,
    },
};
use gpui::{Context, Window, WindowHandle};

#[derive(Default)]
pub(crate) struct Host {
    pub(crate) detached: bool,
    pub(crate) window: Option<WindowHandle<Popout>>,
    pub(super) folder: Option<String>,
    info_token: usize,
    pub(super) path: String,
}
pub(crate) fn enabled() -> bool {
    std::env::var_os("FLEA_CUSTOM_PREVIEW").is_some_and(|v| v == "1")
}
fn previewable(row: &RowFacts) -> bool {
    !row.dir
        && (kinds::quick_look_kind(&row.icon, &row.name, false) != State::Unsupported
            || super::mount::Mount::of(row).is_some())
}

impl Shell {
    pub(super) fn custom_preview_open(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pane) = self.panes[side]
            .as_ref()
            .filter(|p| !p.pane.busy() && !p.stopped)
        else {
            return false;
        };
        let Some(row) = pane.pane.held.row(pane.pane.cursor).cloned() else {
            return false;
        };
        let folder = row
            .dir
            .then(|| crate::navigation::history::join(pane.pane.path(), &row.name));
        let cursor = pane.pane.cursor;
        let base = pane.pane.path().to_owned();
        let items: Vec<_> = pane
            .pane
            .held
            .rows
            .iter()
            .filter(|r| previewable(r))
            .map(|row| Item {
                path: crate::navigation::history::join(&base, &row.name),
                row: row.clone(),
            })
            .collect();
        let chosen = items
            .iter()
            .position(|i| i.row.name == row.name)
            .or_else(|| {
                pane.pane
                    .held
                    .rows
                    .iter()
                    .enumerate()
                    .find(|(i, r)| *i + pane.pane.held.start > cursor && previewable(r))
                    .and_then(|(_, r)| items.iter().position(|i| i.row.name == r.name))
            });
        let options = Options::read(&self.launch.settings.data);
        let host = self.quick_look_host(side, window, cx);
        host.view.update(cx, |view, _| {
            let mut custom = Custom::new(options);
            custom.items = items;
            view.custom = Some(custom);
        });
        host.custom.folder = folder.clone();
        if let Some(path) = folder {
            self.quick_look_send(
                side,
                format!(
                    r#"{{"c":"peek","path":"{}","first":511,"hidden":false,"hiddenLast":false}}"#,
                    escape(&path)
                ),
            );
            return true;
        }
        if let Some(index) = chosen {
            self.custom_preview_select(index, window, cx);
        }
        true
    }

    pub(super) fn custom_preview_select(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.quick_look.as_ref() else {
            return;
        };
        let side = host.side;
        let view = host.view.clone();
        let Some(item) = view
            .read(cx)
            .custom
            .as_ref()
            .and_then(|c| c.items.get(index))
            .cloned()
        else {
            return;
        };
        view.update(cx, |view, _| {
            let custom = view.custom.as_mut().unwrap();
            custom.current = index;
            custom.metadata = None;
            custom.fps = 0.;
            custom.bitrate = 0;
        });
        // A synthetic row is only an identity for the preview. It never moves the browser cursor.
        self.quick_look.as_mut().unwrap().custom.path = item.path.clone();
        let path = std::path::PathBuf::from(&item.path);
        self.show_row(side, index, &item.row, item.path, window, cx);
        view.update(cx, |view, cx| {
            let image = view
                .custom
                .as_ref()
                .and_then(|c| c.preloads.get(&path.to_string_lossy().to_string()))
                .filter(|p| crate::preview_quality::reuse::file_identity(&path) == Some(p.identity))
                .map(|p| p.image.clone());
            if let Some(image) = image {
                view.picture_interim(path, image, cx);
            }
        });
    }

    pub(super) fn custom_preview_present(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.quick_look.as_mut() else {
            return;
        };
        let Some(path) = host
            .view
            .read(cx)
            .custom
            .as_ref()
            .and_then(|c| c.path())
            .map(str::to_owned)
        else {
            return;
        };
        let side = host.side;
        let existing = if host.meta != 0 {
            host.meta
        } else if matches!(host.ask, super::Ask::Meta { .. }) {
            host.token
        } else {
            0
        };
        let id = if existing != 0 {
            existing as usize
        } else {
            token() as usize
        };
        host.custom.info_token = id;
        if existing == 0 {
            self.quick_look_send(
                side,
                format!(
                    r#"{{"c":"previewmeta","path":"{}","row":0,"token":{id},"media":false}}"#,
                    escape(&path)
                ),
            );
        }
        self.custom_preview_window(window, cx);
    }

    /// Native preview requests also use the displayed path when the subject comes from a folder peek.
    pub(super) fn custom_preview_line(&self, line: String) -> String {
        if !enabled() {
            return line;
        }
        let Some(path) = self
            .quick_look
            .as_ref()
            .map(|h| &h.custom.path)
            .filter(|p| !p.is_empty())
        else {
            return line;
        };
        let Ok(doc) = flea::jsondoc::parse(&line) else {
            return line;
        };
        if doc.get("c").and_then(Json::as_str) != Some("meta") {
            return line;
        }
        let row = doc.get("row").and_then(Json::as_f64).unwrap_or(0.) as usize;
        let token = doc.get("token").and_then(Json::as_f64).unwrap_or(0.) as usize;
        let media = doc.get("media").and_then(Json::as_bool) == Some(true);
        let archive = doc.get("archive").and_then(Json::as_bool) == Some(true);
        format!(
            r#"{{"c":"previewmeta","path":"{}","row":{row},"token":{token},"media":{media},"archive":{archive}}}"#,
            escape(&path)
        )
    }

    pub(super) fn custom_preview_reply(
        &mut self,
        side: usize,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.quick_look.as_ref().filter(|h| h.side == side) else {
            return;
        };
        let view = host.view.clone();
        if view.read(cx).custom.is_none() {
            return;
        }
        if let Event::Producer(line) = event {
            if let Some(meta) =
                meta::parse(line).filter(|m| m.token == Some(host.custom.info_token))
            {
                let doc = flea::jsondoc::parse(line).ok();
                view.update(cx, |v, cx| {
                    let c = v.custom.as_mut().unwrap();
                    c.metadata = Some(meta);
                    if let (Some(row), Some(doc)) = (c.items.get_mut(c.current), doc) {
                        c.fps = doc.get("fps").and_then(Json::as_f64).unwrap_or(0.);
                        c.bitrate = doc.get("bitrate").and_then(Json::as_f64).unwrap_or(0.) as u64;
                        row.row.size =
                            doc.get("size")
                                .and_then(Json::as_f64)
                                .unwrap_or(row.row.size as f64) as u64;
                        row.row.mtime =
                            doc.get("mtime")
                                .and_then(Json::as_f64)
                                .unwrap_or(row.row.mtime as f64) as i64;
                        row.row.mode =
                            doc.get("mode")
                                .and_then(Json::as_f64)
                                .unwrap_or(row.row.mode as f64) as u32;
                    }
                    cx.notify();
                });
            }
        }
        if let Event::Peeked(doc) = event {
            let path = doc.get("path").and_then(Json::as_str).unwrap_or("");
            if host.custom.folder.as_deref() != Some(path)
                || doc.get("first").and_then(Json::as_f64) != Some(511.)
            {
                return;
            }
            let items = doc
                .get("rows")
                .and_then(Json::as_array)
                .unwrap_or(&[])
                .iter()
                .filter_map(|r| {
                    let row = RowFacts {
                        name: r.get("n")?.as_str()?.into(),
                        dir: r.get("d")?.as_bool()?,
                        icon: r.get("i")?.as_str()?.into(),
                        ..Default::default()
                    };
                    previewable(&row).then(|| Item {
                        path: crate::navigation::history::join(path, &row.name),
                        row,
                    })
                })
                .collect();
            view.update(cx, |v, _| {
                v.custom.as_mut().unwrap().items = items;
            });
            self.custom_preview_select(0, window, cx);
        }
    }

    pub(super) fn custom_preview_default(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = &self.quick_look else { return };
        let side = host.side;
        let path = host
            .view
            .read(cx)
            .custom
            .as_ref()
            .and_then(|c| c.path())
            .map(str::to_owned);
        if let Some(path) = path {
            self.open_file(side, path, window, cx);
            self.quick_look_tab_switch(window, cx);
        }
    }
}
