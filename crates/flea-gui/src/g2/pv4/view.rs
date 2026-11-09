//! The mounted 3D preview: requests the model, applies only its live reply, builds the model off the main thread and swaps it in whole.
use super::{
    element::{Presented, Viewport},
    live::{Live, Model},
    parts,
    seam::{Host, ModelReader, Wire},
    session::Session,
};
use crate::list::Palette as Roles;
use flea::{
    i18n::I18n,
    ui_logic_pv4::{
        color::Palette,
        control::{action, Action, Surface},
        facts::{self, FileFacts, Row},
        wire::{self, Data, State},
    },
};
use gpui::{div, img, prelude::*, Context, Render, SharedString, Task, Window};
use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::Arc, time::Instant};

/// Where the preview stands.
#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    Idle,
    Loading,
    Ready,
    /// The view says the shared cannot-preview sentence; the reason is kept for diagnostics only.
    Failed(String),
}

/// What the host's bar shows for the model: its one fact and the plate words with the chosen one.
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub fact: Option<String>,
    pub words: Vec<String>,
    pub chosen: usize,
}

pub struct Model3dView {
    pub host: Host,
    pub phase: Phase,
    pub live: Rc<RefCell<Live>>,
    pub presented: Rc<RefCell<Presented>>,
    session: Session,
    wire: Rc<dyn Wire>,
    reader: Arc<dyn ModelReader>,
    i18n: Arc<I18n>,
    path: Option<String>,
    plate: usize,
    data: Option<Data>,
    thumbnail: String,
    generation: u64,
    epoch: Instant,
    _build: Option<Task<()>>,
}

fn slot(surface: Surface) -> &'static str {
    if surface == Surface::Column {
        "column"
    } else {
        "look"
    }
}

impl Model3dView {
    pub fn new(
        surface: Surface,
        wire: Rc<dyn Wire>,
        reader: Arc<dyn ModelReader>,
        i18n: Arc<I18n>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            host: Host::default(),
            phase: Phase::Idle,
            live: Rc::new(RefCell::new(Live::new(surface))),
            presented: Rc::default(),
            session: Session::new(slot(surface)),
            wire,
            reader,
            i18n,
            path: None,
            plate: 0,
            data: None,
            thumbnail: String::new(),
            generation: 0,
            epoch: cx.background_executor().now(),
            _build: None,
        }
    }

    /// Prepares the resting column's first complete frame before the host swaps it on screen.
    pub fn prepare_column(
        &mut self,
        size: (f32, f32),
        window: &Window,
        theme: crate::palette::Theme,
        cx: &mut Context<Self>,
    ) {
        if self.phase != Phase::Ready
            || self.live.borrow().thumbnail
            || self.presented.borrow().image.is_some()
        {
            return;
        }
        let roles = Roles::inherited(&window.text_style(), theme);
        let palette = super::seam::palette(&roles, theme.background);
        let frame =
            self.live
                .borrow_mut()
                .frame(0., &self.host, size, window.scale_factor(), palette);
        let Some(job) = frame.job else { return };
        let job = cx.background_executor().spawn(async move {
            let mut drawn = job.draw();
            let image = drawn.picture();
            (drawn, image)
        });
        cx.spawn(async move |this, cx| {
            let (drawn, image) = job.await;
            let _ = this.update(cx, |view, cx| {
                if view.live.borrow_mut().finish(drawn) {
                    if let Some(image) = image {
                        view.presented.borrow_mut().present(image);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn set_catalog(&mut self, catalog: Arc<I18n>, cx: &mut Context<Self>) {
        self.i18n = catalog;
        cx.notify();
    }

    fn surface(&self) -> Surface {
        self.live.borrow().surface
    }

    /// Shows plate `plate` of `path`; the same file keeps drawing until the new plate is built, another file clears at once.
    pub fn show(&mut self, path: &str, plate: usize, cx: &mut Context<Self>) {
        if self.path.as_deref() == Some(path) && self.plate == plate && self.phase != Phase::Idle {
            return;
        }
        if self.path.as_deref() != Some(path) {
            self.clear();
        }
        self.cancel();
        self.path = Some(path.to_string());
        self.plate = plate;
        let line = self.session.open(path, Some(plate));
        if self.wire.send(line) {
            self.phase = Phase::Loading;
        } else {
            self.fail("backend gone".into());
        }
        cx.notify();
    }

    /// The preview closed: the request is cancelled, a build in flight is dropped and the model is released.
    pub fn hide(&mut self, cx: &mut Context<Self>) {
        let picture = self.presented.borrow().image.clone();
        if let Some(picture) = picture {
            cx.defer(move |cx| cx.drop_image(picture, None));
        }
        self.cancel();
        self.clear();
        self.path = None;
        self.phase = Phase::Idle;
        cx.notify();
    }

    fn cancel(&mut self) {
        if let Some(line) = self.session.close() {
            self.wire.send(line);
        }
        self.generation += 1;
        self._build = None;
    }

    fn clear(&mut self) {
        self.live.borrow_mut().set(None, false);
        self.data = None;
        self.thumbnail.clear();
        self.presented.borrow_mut().retire();
    }

    /// Applies one backend line; answers whether it was this view's live reply.
    pub fn on_line(&mut self, line: &str, cx: &mut Context<Self>) -> bool {
        let Ok(reply) = wire::parse(line) else {
            return false;
        };
        if !self.session.accept(&reply) {
            return false;
        }
        match reply.state {
            State::Ready {
                file,
                thumbnail,
                data,
            } => self.build(file, thumbnail, *data, cx),
            State::Failed { reason } => self.fail(reason),
            State::Cancelled => self.phase = Phase::Idle,
        }
        cx.notify();
        true
    }

    fn build(&mut self, file: String, thumbnail: String, data: Data, cx: &mut Context<Self>) {
        self.generation += 1;
        let (generation, plate, reader) = (self.generation, self.plate, self.reader.clone());
        let palette = self.live.borrow().palette.unwrap_or(Palette {
            ground: [0.; 3],
            foreground: [1.; 3],
            muted: [0.6; 3],
            frame: [0.5; 3],
        });
        let read = data.clone();
        let job = cx.background_executor().spawn(async move {
            reader
                .read(&flea::bytename::view(&file), &read)
                .and_then(|scene| Model::build(scene, plate, palette))
        });
        self._build = Some(cx.spawn(async move |this, cx| {
            let built = job.await;
            let _ = this.update(cx, |view, cx| {
                if view.generation != generation {
                    return;
                }
                match built {
                    Ok(model) => {
                        view.live
                            .borrow_mut()
                            .set(Some(model), !thumbnail.is_empty());
                        view.thumbnail = thumbnail;
                        view.data = Some(data);
                        view.phase = Phase::Ready;
                    }
                    Err(reason) => view.fail(reason),
                }
                cx.notify();
            });
        }));
    }

    // A failure never leaves the previous plate on screen.
    fn fail(&mut self, reason: String) {
        self.clear();
        self.phase = Phase::Failed(reason);
    }

    // The plates the reply describes one by one; its bare count is a fact to show, never a size to allocate.
    fn plates(&self) -> usize {
        self.data.as_ref().map_or(1, |d| d.plates.len().max(1))
    }

    /// Applies a preview key action by its key table name; answers whether the view took it.
    pub fn act(&mut self, name: &str, cx: &mut Context<Self>) -> bool {
        let Some(action) = action(name) else {
            return false;
        };
        // A hidden or unfocused preview takes no key, so only the focused preview moves.
        if !self.host.shown || !self.host.focused {
            return false;
        }
        if matches!(action, Action::PlatePrevious | Action::PlateNext) {
            let plates = self.plates();
            if plates < 2 || self.surface() == Surface::Column || self.phase != Phase::Ready {
                return false;
            }
            let step = if action == Action::PlateNext {
                1
            } else {
                plates - 1
            };
            let path = self.path.clone().unwrap_or_default();
            self.show(&path, (self.plate + step) % plates, cx);
            return true;
        }
        let now = cx
            .background_executor()
            .now()
            .saturating_duration_since(self.epoch)
            .as_secs_f64();
        let animations = self.host.animations;
        let taken = self
            .live
            .borrow_mut()
            .viewer
            .as_mut()
            .is_some_and(|v| v.act(action, now, animations));
        if taken {
            cx.notify();
        }
        taken
    }

    pub fn bar(&self) -> Option<Bar> {
        let data = self.data.as_ref()?;
        Some(Bar {
            fact: facts::bar_fact(&self.i18n, data, self.plate),
            words: facts::plate_words(&self.i18n, self.plates()),
            chosen: self.plate,
        })
    }

    pub fn facts(&self, file: FileFacts) -> Vec<Vec<Row>> {
        self.data.as_ref().map_or_else(Vec::new, |data| {
            facts::rows(&self.i18n, file, data, self.plate)
        })
    }

    /// The shared sentence a failed preview says, for the host's no-preview state.
    pub fn failed(&self) -> Option<String> {
        matches!(self.phase, Phase::Failed(_)).then(|| facts::cannot_preview(&self.i18n))
    }

    fn layer_label(&self) -> Option<String> {
        let live = self.live.borrow();
        let (viewer, model) = (live.viewer.as_ref()?, live.model.as_ref()?);
        let layer = viewer.layer?;
        let z = model.scene.layers.get(layer)?.z;
        Some(facts::layer_label(
            &self.i18n,
            layer + 1,
            viewer.layers(),
            z,
        ))
    }
}

impl Render for Model3dView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let roles = Roles::inherited(&window.text_style(), theme);
        // The viewport stays mounted while empty so a retired picture still leaves the atlas; a plate step keeps the old frame.
        let mut root = div().size_full().flex().flex_col();
        let mut stage = div().relative().flex_1().min_h_0();
        if self.live.borrow().thumbnail {
            stage = stage.child(
                div()
                    .absolute()
                    .size_full()
                    .child(img(PathBuf::from(&self.thumbnail)).size_full()),
            );
        }
        root = root.child(stage.child(Viewport {
            live: self.live.clone(),
            presented: self.presented.clone(),
            host: self.host,
            epoch: self.epoch,
        }));
        if let Some(label) = self.layer_label() {
            root = root.child(parts::layer_strip(
                self.live.clone(),
                label,
                &roles,
                self.host.ground,
            ));
        }
        root
    }
}

/// The scene id a native capture of this board state is filed under.
pub fn scene_id(board: &str) -> SharedString {
    format!("g2.pv4.{board}.primary").into()
}
