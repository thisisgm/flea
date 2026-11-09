//! Shared state for the custom preview window. Content stays in the native preview views.
use super::{Event, QuickLook};
use flea::{
    jsondoc::Json,
    protocol::RowFacts,
    ui_logic_pv1::{kinds::State, meta::Meta},
};
use gpui::{point, px, size, Bounds, Context, Pixels, Size};
use std::time::Duration;

#[derive(Clone, Debug)]
pub(crate) struct Item {
    pub row: RowFacts,
    pub path: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Options {
    pub preload: bool,
    pub media_controls: bool,
    pub info: bool,
    pub hide_navbar: bool,
    pub delay: u64,
}
impl Options {
    pub fn read(data: &Json) -> Self {
        let p = data.get("preview");
        let on = |key| p.and_then(|p| p.get(key)).and_then(Json::as_bool) != Some(false);
        Self {
            preload: on("preloadAdjacent"),
            media_controls: on("mediaControls"),
            info: p.and_then(|p| p.get("infoPanel")).and_then(Json::as_bool) == Some(true),
            hide_navbar: on("hideNavbarFullscreen"),
            delay: p
                .and_then(|p| p.get("navbarHideMs"))
                .and_then(Json::as_f64)
                .unwrap_or(4000.)
                .clamp(1000., 10000.) as u64,
        }
    }
}

pub(crate) struct Custom {
    pub options: Options,
    pub items: Vec<Item>,
    pub current: usize,
    pub info: bool,
    pub filmstrip: bool,
    pub fullscreen: bool,
    pub navbar: bool,
    pub hide: Option<gpui::Task<()>>,
    pub metadata: Option<Meta>,
    pub fps: f64,
    pub bitrate: u64,
    pub thumbnails: std::collections::HashMap<String, Option<std::sync::Arc<gpui::RenderImage>>>,
    pub preloads: std::collections::HashMap<String, super::custom_preload::Preloaded>,
    pub preload_failed: std::collections::HashSet<String>,
    pub preload_task: Option<gpui::Task<()>>,
    pub thumbnail_task: Option<gpui::Task<()>>,
}
impl Custom {
    pub fn new(options: Options) -> Self {
        Self {
            info: options.info,
            options,
            items: Vec::new(),
            current: 0,
            thumbnails: Default::default(),
            thumbnail_task: None,
            preloads: Default::default(),
            preload_failed: Default::default(),
            preload_task: None,
            filmstrip: true,
            fullscreen: false,
            navbar: true,
            hide: None,
            metadata: None,
            fps: 0.,
            bitrate: 0,
        }
    }
    pub fn path(&self) -> Option<&str> {
        self.items.get(self.current).map(|i| i.path.as_str())
    }
    pub fn filmstrip_visible(&self) -> bool {
        self.filmstrip && self.items.len() > 1
    }
    pub fn card(&self, viewport: Size<Pixels>) -> Bounds<Pixels> {
        let width = f32::from(viewport.width);
        let height = f32::from(viewport.height);
        // Chrome overlays a fixed top lane. Hiding it cannot move the content.
        let top = if self.fullscreen { 0. } else { 36. };
        let bottom = if self.filmstrip_visible() { 92. } else { 0. };
        let info = if self.info {
            (width * 0.3).clamp(180., 300.)
        } else {
            0.
        };
        Bounds::new(
            point(px(0.), px(top)),
            size(
                px((width - info).max(1.)),
                px((height - top - bottom).max(1.)),
            ),
        )
    }
}

impl QuickLook {
    pub(crate) fn custom_reveal(&mut self, cx: &mut Context<Self>) {
        let Some(custom) = &mut self.custom else {
            return;
        };
        custom.navbar = true;
        let delay = custom.options.delay;
        custom.hide = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(delay))
                .await;
            let _ = this.update(cx, |view, cx| {
                if let Some(custom) = &mut view.custom {
                    if custom.fullscreen && custom.options.hide_navbar {
                        custom.navbar = false;
                    }
                    custom.hide = None;
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub(crate) fn custom_action(&mut self, action: &str, cx: &mut Context<Self>) -> bool {
        if self.custom.is_none() {
            return false;
        }
        self.custom_reveal(cx);
        self.reveal_strip(cx);
        let media = self
            .subject
            .as_ref()
            .is_some_and(|s| matches!(s.state, State::Audio | State::Video));
        if matches!(action, "previousPage" | "nextPage") {
            if let Some(doc) = self.subject.as_ref().and_then(|s| s.body.doc()).cloned() {
                let name = if action == "nextPage" {
                    "previewPageForward"
                } else {
                    "previewPageBack"
                };
                doc.action(name, cx);
                cx.notify();
            }
            return true;
        }
        if matches!(action, "previousSheet" | "nextSheet") {
            if let Some(doc) = self.subject.as_ref().and_then(|s| s.body.doc()).cloned() {
                doc.action(action, cx);
                cx.notify();
            }
            return true;
        }
        if action == "preview" && media {
            if let Some(doc) = self.subject.as_ref().and_then(|s| s.body.doc()).cloned() {
                doc.action("playPause", cx);
            }
            return true;
        }
        let custom = self.custom.as_mut().unwrap();
        match action {
            "toggleInfo" => custom.info = !custom.info,
            "toggleFilmstrip" => custom.filmstrip = !custom.filmstrip,
            "fullscreen" => cx.emit(Event::Fullscreen),
            "openDefault" => cx.emit(Event::OpenDefault),
            "cursorDown" | "cursorUp" | "previousFile" | "nextFile" => {
                let step = if matches!(action, "cursorDown" | "nextFile") {
                    1
                } else {
                    -1
                };
                let next = (custom.current as isize + step)
                    .clamp(0, custom.items.len().saturating_sub(1) as isize)
                    as usize;
                if next != custom.current {
                    cx.emit(Event::Navigate(next));
                }
            }
            _ => return false,
        }
        cx.notify();
        true
    }
}
