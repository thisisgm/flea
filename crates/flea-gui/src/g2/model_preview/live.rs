//! The loaded model and the frames it asks for: the viewer runs on the UI thread, and each changed frame is a job a worker rasterizes.
use super::seam::Host;
use flea::ui_logic_model_preview::{
    camera::{Home, View},
    color::Palette,
    control::{Surface, Viewer},
    raster::{Prepared, Raster, Stats, Style},
    scene::{decimate, edges, Edge, PlateRect, Scene, CPU_LIVE_TRIANGLES},
};
use gpui::RenderImage;
use std::sync::{Arc, OnceLock};

/// One plate ready to draw, built off the main thread and shared read-only with the frame worker.
pub struct Model {
    pub scene: Scene,
    pub prepared: Prepared,
    /// The start view, measured here so the UI thread never walks the model.
    pub home: Home,
    edges: OnceLock<Vec<Edge>>,
}

impl Model {
    /// Checks and centres the scene, keeps only plate `plate` of a mesh decimated to the CPU live limit, and drops a toolpath's travel; fails on a bad or empty scene.
    pub fn build(mut scene: Scene, plate: usize, palette: Palette) -> Result<Self, String> {
        scene.check()?;
        scene.recentre();
        let plate = if plate < scene.plate_count() {
            plate
        } else {
            0
        };
        let scene = if scene.is_toolpath() {
            scene.keep_extrusions();
            scene
        } else {
            decimate(&scene, plate, CPU_LIVE_TRIANGLES)
        };
        let home = Home::new(&scene, 0).ok_or("empty model")?;
        let prepared = Prepared::new(&scene, palette);
        Ok(Self {
            scene,
            prepared,
            home,
            edges: OnceLock::new(),
        })
    }

    /// The wireframe's edges, built by the worker on the first wireframe frame.
    pub fn edges(&self) -> &[Edge] {
        self.edges.get_or_init(|| {
            if self.scene.is_toolpath() {
                Vec::new()
            } else {
                edges(&self.scene, 0)
            }
        })
    }

    pub fn has_edges(&self) -> bool {
        self.edges.get().is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Key {
    view: View,
    size: (usize, usize, usize),
    palette: Palette,
    wire: bool,
    layer: Option<usize>,
}

/// One frame to rasterize off the UI thread, carrying the reused buffers there and back.
pub struct Job {
    pub generation: u64,
    model: Arc<Model>,
    raster: Raster,
    /// A pixel buffer the atlas gave back, reused so a view allocates none while it stays the same size.
    pixels: Vec<u8>,
    view: View,
    size: (usize, usize, usize),
    palette: Palette,
    wire: bool,
    layer: Option<usize>,
    plate: PlateRect,
}

/// A rasterized frame: its BGRA pixels and their size, and the buffers going back, released after a still.
pub struct Drawn {
    pub generation: u64,
    pub pixels: Vec<u8>,
    pub size: (usize, usize),
    pub raster: Raster,
    pub stats: Stats,
    /// Whether the pixel buffer had to grow to hold the frame.
    pub grew: bool,
}

impl Job {
    /// Draws the frame into the pixel buffer it carries; a still at two samples a side hands its raster back empty, so a view at rest holds none.
    pub fn draw(self) -> Drawn {
        let Self {
            generation,
            model,
            mut raster,
            mut pixels,
            view,
            size: (w, h, ss),
            palette,
            wire,
            layer,
            plate,
        } = self;
        let style = Style {
            palette,
            wire: wire.then(|| model.edges()),
            layer,
            plate: Some(plate),
            ground: Some(palette.ground),
        };
        let grew = pixels.capacity() < w * h * 4;
        // A moving frame draws straight into the buffer it hands over.
        if ss == 1 {
            raster.lend(std::mem::take(&mut pixels));
        }
        raster.resize(w, h, ss);
        raster.render(&model.scene, &model.prepared, 0, &view, &style);
        raster.frame_into(&mut pixels);
        let stats = raster.stats;
        if ss > 1 {
            raster.release();
        }
        Drawn {
            generation,
            pixels,
            size: (w, h),
            raster,
            stats,
            grew,
        }
    }
}

impl Drawn {
    /// Hands the pixels to GPUI as they are, with nothing encoded or decoded; the buffer comes back through `Live::recycle` once the atlas drops the picture.
    pub fn picture(&mut self) -> Option<Arc<RenderImage>> {
        let (w, h) = (
            u32::try_from(self.size.0).ok()?,
            u32::try_from(self.size.1).ok()?,
        );
        RenderImage::from_bgra(w, h, std::mem::take(&mut self.pixels)).map(Arc::new)
    }
}

/// What one frame produced: whether the view still moves, and a job when the picture changed and no worker is busy.
pub struct Frame {
    pub moving: bool,
    pub job: Option<Job>,
}

/// Counts for the gates: frames rasterized, frames asked for, and buffer growth over every frame drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Counters {
    pub rasters: usize,
    pub requests: usize,
    pub allocations: usize,
    /// Pixel buffers that grew to hold a frame.
    pub pixel_allocations: usize,
    /// Pixel buffers taken back from pictures that left the screen.
    pub recycled: usize,
    /// The last drawn frame's cost.
    pub last: Stats,
}

pub struct Live {
    pub surface: Surface,
    pub model: Option<Arc<Model>>,
    pub viewer: Option<Viewer>,
    /// The column shows the file's embedded picture until the first press.
    pub thumbnail: bool,
    pub dragging: Option<(f32, f32, bool)>,
    /// A press on the layer strip is held, so a drag seeks.
    pub seeking: bool,
    pub palette: Option<Palette>,
    pub counters: Counters,
    /// The buffers, absent while a worker holds them.
    raster: Option<Raster>,
    /// The pixel buffer the atlas gave back last, empty while a worker or a picture holds every buffer.
    spare: Vec<u8>,
    key: Option<Key>,
    paused: bool,
    generation: u64,
}

impl Live {
    pub fn new(surface: Surface) -> Self {
        Self {
            surface,
            model: None,
            viewer: None,
            thumbnail: false,
            dragging: None,
            seeking: false,
            palette: None,
            counters: Counters::default(),
            raster: Some(Raster::default()),
            spare: Vec::new(),
            key: None,
            paused: false,
            generation: 0,
        }
    }

    /// Swaps in a new model whole; the next frame frames it from home, and a frame still being drawn for the old one is dropped.
    pub fn set(&mut self, model: Option<Model>, thumbnail: bool) {
        self.model = model.map(Arc::new);
        self.viewer = None;
        self.key = None;
        self.dragging = None;
        self.thumbnail = thumbnail && self.surface == Surface::Column;
        self.generation += 1;
        self.raster = Some(Raster::default());
    }

    /// Whether a worker is drawing a frame.
    pub fn busy(&self) -> bool {
        self.raster.is_none()
    }

    /// Takes a worker's frame back; answers whether it belongs to the model on screen and should be presented.
    pub fn finish(&mut self, drawn: Drawn) -> bool {
        self.counters.pixel_allocations += usize::from(drawn.grew);
        if drawn.generation != self.generation {
            return false;
        }
        self.raster = Some(drawn.raster);
        self.counters.rasters += 1;
        self.counters.allocations += drawn.stats.allocations;
        self.counters.last = drawn.stats;
        true
    }

    /// Takes back the pixel buffer of a picture nothing else holds, so the next moving frame draws into it; a view at rest keeps none.
    pub fn recycle(&mut self, image: Arc<RenderImage>) {
        let moving = self.key.is_some_and(|key| key.size.2 == 1);
        if let (true, Some(pixels)) = (
            moving,
            Arc::try_unwrap(image).ok().and_then(RenderImage::into_bgra),
        ) {
            self.counters.recycled += 1;
            if pixels.capacity() > self.spare.capacity() {
                self.spare = pixels;
            }
        }
    }

    /// Advances the view to `now` and, when anything it shows changed and no worker is busy, hands out the frame at `scale`.
    pub fn frame(
        &mut self,
        now: f64,
        host: &Host,
        size: (f32, f32),
        scale: f32,
        palette: Palette,
    ) -> Frame {
        self.palette = Some(palette);
        let idle = Frame {
            moving: false,
            job: None,
        };
        let Some(model) = self.model.clone() else {
            return idle;
        };
        let (w, h) = size;
        if w < 1. || h < 1. {
            return idle;
        }
        let viewer = self.viewer.get_or_insert_with(|| {
            let mut viewer = Viewer::from_home(
                self.surface,
                model.home,
                (0, 1),
                model.scene.layers.len(),
                w,
                h,
            );
            viewer.opened(now, host.animations);
            viewer
        });
        viewer.resize(w, h);
        if !host.animations && viewer.spinning() {
            viewer.still();
        }
        let moving = if host.shown && host.focused {
            if std::mem::take(&mut self.paused) {
                viewer.resume(now);
            }
            viewer.advance(now)
        } else {
            if !self.paused {
                viewer.pause();
                self.paused = true;
            }
            false
        };
        if self.thumbnail || !host.shown {
            return Frame { moving, job: None };
        }
        let ss = if moving || self.dragging.is_some() {
            1
        } else {
            2
        };
        let (wd, hd) = ((w * scale).round() as usize, (h * scale).round() as usize);
        let key = Key {
            view: viewer.view,
            size: (wd, hd, ss),
            palette,
            wire: viewer.wire,
            layer: viewer.layer,
        };
        if self.key == Some(key) {
            return Frame { moving, job: None };
        }
        let Some(raster) = self.raster.take() else {
            return Frame { moving, job: None };
        };
        self.key = Some(key);
        let view = View {
            scale: viewer.view.scale * scale,
            cx: viewer.view.cx * scale,
            cy: viewer.view.cy * scale,
            ..viewer.view
        };
        let pixels = std::mem::take(&mut self.spare);
        let job = Job {
            generation: self.generation,
            model,
            raster,
            pixels,
            view,
            size: key.size,
            palette,
            wire: key.wire,
            layer: key.layer,
            plate: viewer.rect,
        };
        Frame {
            moving,
            job: Some(job),
        }
    }
}
