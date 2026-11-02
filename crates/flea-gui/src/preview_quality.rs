//! Physical-pixel refinement, retaining the first picture while bounded workers draw its replacement.
mod frame_edges;
mod helper;
mod readiness;
mod render;
pub(crate) mod reuse;
pub(crate) use readiness::{media_ready, pdf_ready};
#[cfg(test)]
mod tests;
use gpui::{App, Asset, Context, RenderImage, Window};
use std::{
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

static NEXT: AtomicU64 = AtomicU64::new(1);
// At most two refinement renders at once, and always one fewer than gpui's background threads, since a render can wait out a helper's deadline.
const WORKERS: usize = 2;
thread_local! {
    static PERMITS: (async_channel::Sender<()>, async_channel::Receiver<()>) = permits();
    // Requests released on the App's thread, even by a drop without App; the next sweep removes their assets.
    static RELEASED: std::cell::RefCell<Vec<Request>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[derive(Clone)]
pub(crate) struct Request {
    pub path: PathBuf,
    pub size: (u32, u32),
    pub version: (u64, i64),
    owner: u64,
    current: Arc<AtomicBool>,
    wake: async_channel::Sender<()>,
    seed: Arc<Mutex<Option<Arc<RenderImage>>>>,
}
impl Hash for Request {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (&self.path, self.size, self.version, self.owner).hash(state);
    }
}

pub(crate) enum Loader {}
impl Asset for Loader {
    type Source = Request;
    type Output = Result<Arc<RenderImage>, ()>;
    fn load(
        request: Request,
        cx: &mut App,
    ) -> impl std::future::Future<Output = Self::Output> + Send + 'static {
        let renderer = cx.svg_renderer();
        let wake = request.wake.clone();
        // A request queued behind a decode of the same file keeps that decode for its own size.
        let source = crate::g2::pv1::photo_source::hold(&request.path);
        let pool = PERMITS.with(Clone::clone);
        async move {
            let _source = source;
            let result = async move {
                pool.1.recv().await.map_err(|_| ())?;
                let _held = Permit(pool.0);
                if !request.current.load(Ordering::Acquire) {
                    return Err(());
                }
                let identity = reuse::file_identity(&request.path);
                let image = render::run(&request, renderer);
                if let Ok(image) = &image {
                    reuse::keep(
                        &request.path,
                        request.version,
                        request.size,
                        image,
                        identity,
                    );
                }
                if let Ok(mut seed) = request.seed.lock() {
                    seed.take();
                }
                request
                    .current
                    .load(Ordering::Acquire)
                    .then_some(image)
                    .ok_or(())?
            }
            .await;
            let _ = wake.try_send(());
            result
        }
    }
}

fn permits() -> (async_channel::Sender<()>, async_channel::Receiver<()>) {
    // Each App thread owns its pool so returning a permit never wakes another App's executor.
    let threads = std::thread::available_parallelism().map_or(2, |count| count.get().max(2));
    let workers = WORKERS.min(threads - 1);
    let (send, receive) = async_channel::bounded(workers);
    for _ in 0..workers {
        send.try_send(()).unwrap();
    }
    (send, receive)
}
struct Permit(async_channel::Sender<()>);
impl Drop for Permit {
    fn drop(&mut self) {
        let _ = self.0.try_send(());
    }
}

/// Removes the asset of every released request, including those whose owner was dropped without App.
pub(crate) fn sweep(cx: &mut App) {
    let released = RELEASED.with_borrow_mut(std::mem::take);
    for request in released {
        cx.remove_asset::<Loader>(&request);
    }
}

/// The size a refined picture was made from, so a picture's own refinement never resizes its next request.
#[derive(Default)]
pub(crate) struct Basis(Option<(std::sync::Weak<RenderImage>, (u32, u32))>);
impl Basis {
    /// The picture's own size, or the size of the picture it replaced when this basis made it.
    pub fn of(&self, picture: &Arc<RenderImage>) -> (u32, u32) {
        match &self.0 {
            Some((made, size)) if std::ptr::eq(made.as_ptr(), Arc::as_ptr(picture)) => *size,
            _ => {
                let size = picture.size(0);
                (size.width.0.max(0) as u32, size.height.0.max(0) as u32)
            }
        }
    }
    /// Records that `picture` replaced one of `size`; the weak reference keeps its address from reuse.
    pub fn made(&mut self, picture: &Arc<RenderImage>, size: (u32, u32)) {
        self.0 = Some((Arc::downgrade(picture), size));
    }
}

#[derive(Default)]
pub(crate) struct Refinement {
    pub requested: Option<Request>,
    pub ready: bool,
    pub basis: Basis,
    notification: Option<gpui::Task<()>>,
    primed: Option<Arc<RenderImage>>,
}
impl Refinement {
    pub fn clear(&mut self, cx: &mut App) {
        self.cancel();
        sweep(cx);
    }
    /// Stops the request at once; its asset leaves the cache at the next sweep.
    pub fn cancel(&mut self) {
        self.ready = false;
        self.notification = None;
        self.primed = None;
        if let Some(request) = self.requested.take() {
            request.current.store(false, Ordering::Release);
            // A drop during thread teardown has no App left to sweep, so its request is let go.
            let _ = RELEASED.try_with(|released| released.borrow_mut().push(request));
        }
    }
    pub fn poll<T: 'static>(
        &mut self,
        path: PathBuf,
        size: (u32, u32),
        version: (u64, i64),
        _window: &mut Window,
        cx: &mut Context<T>,
    ) -> Option<Result<Arc<RenderImage>, ()>> {
        self.poll_with_seed(path, size, version, None, _window, cx)
    }
    pub fn poll_with_seed<T: 'static>(
        &mut self,
        path: PathBuf,
        size: (u32, u32),
        version: (u64, i64),
        seed: Option<Arc<RenderImage>>,
        _window: &mut Window,
        cx: &mut Context<T>,
    ) -> Option<Result<Arc<RenderImage>, ()>> {
        if self
            .requested
            .as_ref()
            .is_none_or(|old| old.path != path || old.size != size || old.version != version)
        {
            self.clear(cx);
            let (wake, receive) = async_channel::bounded(1);
            let request = Request {
                path,
                size,
                version,
                owner: NEXT.fetch_add(1, Ordering::Relaxed),
                current: Arc::new(AtomicBool::new(true)),
                wake,
                seed: Arc::new(Mutex::new(seed)),
            };
            // A kept refinement of this file version already drew these exact device pixels.
            let identity = reuse::file_identity(&request.path);
            self.primed = reuse::kept(&request.path, request.version)
                .filter(|(size, _)| *size == request.size)
                .map(|(_, image)| image);
            if let Some(image) = &self.primed {
                reuse::keep(
                    &request.path,
                    request.version,
                    request.size,
                    image,
                    identity,
                );
            }
            let key = request.clone();
            self.notification = self.primed.is_none().then(|| {
                cx.spawn(async move |this, cx| {
                    if receive.recv().await.is_err() {
                        return;
                    }
                    while key.current.load(Ordering::Acquire) {
                        let ready = this.update(cx, |_, cx| {
                            let ready = cx.fetch_asset::<Loader>(&key).is_some();
                            if ready {
                                cx.notify();
                            }
                            ready
                        });
                        if !matches!(ready, Ok(false)) {
                            break;
                        }
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(1))
                            .await;
                    }
                })
            });
            self.requested = Some(request);
        }
        if let Some(image) = &self.primed {
            self.ready = true;
            return Some(Ok(image.clone()));
        }
        let loaded = cx.fetch_asset::<Loader>(self.requested.as_ref().unwrap());
        self.ready = matches!(loaded, Some(Ok(_)));
        loaded
    }
}
impl Drop for Refinement {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub(crate) fn physical(width: f64, height: f64, scale: f64) -> (u32, u32) {
    flea::ui_logic_pv1::pixels::decode_size(width, height, scale, 0., 0., true)
}

/// Logical dimensions whose raster covers an integer number of device pixels.
pub(crate) fn logical_size(width: f64, height: f64, scale: f64) -> (f32, f32) {
    let (w, h) = physical(width, height, scale);
    ((w as f64 / scale) as f32, (h as f64 / scale) as f32)
}

// A drawn rect stops this far short of its device pixels, so gpui's rounding of each edge never adds one.
const SNAP_SLACK: f64 = 1. / 1024.;

/// The logical size that draws exactly `pixels` device pixels at `scale`: one image pixel to one device pixel.
pub(crate) fn own_size(pixels: (u32, u32), scale: f64) -> (f32, f32) {
    let edge = |count: u32| ((f64::from(count) - SNAP_SLACK).max(0.) / scale) as f32;
    (edge(pixels.0), edge(pixels.1))
}

/// A raster's device pixels when `scale` times its `w`x`h` pixels fit its box; at `limit` it is its source's own size.
pub(crate) fn fitted_pixels(
    w: f64,
    h: f64,
    scale: f64,
    limit: f64,
    device: f64,
) -> Option<(u32, u32)> {
    (scale >= limit).then(|| {
        (
            ((w * limit * device).round() as u32).max(1),
            ((h * limit * device).round() as u32).max(1),
        )
    })
}
