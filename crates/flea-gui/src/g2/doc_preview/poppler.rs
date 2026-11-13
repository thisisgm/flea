//! Poppler's GLib binding and cairo, loaded at run time, so a PDF page renders in this process with no PDF crate linked.
use std::ffi::{c_char, c_double, c_int, c_void, CStr};
use std::os::unix::io::IntoRawFd;
use std::path::Path;
use std::sync::LazyLock;

// std already links the system libc, so the loader's two symbols are declared here rather than taking a crate.
extern "C" {
    fn dlopen(file: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}
const RTLD_NOW: c_int = 2;

// cairo_format_t CAIRO_FORMAT_ARGB32: premultiplied, native endian, so BGRA bytes on little endian, which GPUI draws.
const FORMAT_ARGB32: c_int = 0;
// A raster past this many pixels is drawn smaller and scaled up, so a deep zoom never holds hundreds of megabytes.
pub const MAX_PIXELS: u64 = 24 * 1024 * 1024;
// cairo refuses an image surface edge past 32767; a page is held well inside it.
pub const MAX_EDGE: u32 = 16384;

// An owned GError, only ever freed here; its message is diagnostic and never a sentence on screen.
#[repr(C)]
struct GError {
    _opaque: [u8; 0],
}

type NewFromFd = unsafe extern "C" fn(c_int, *const c_char, *mut *mut GError) -> *mut c_void;
type Pages = unsafe extern "C" fn(*mut c_void) -> c_int;
type GetPage = unsafe extern "C" fn(*mut c_void, c_int) -> *mut c_void;
type PageSize = unsafe extern "C" fn(*mut c_void, *mut c_double, *mut c_double);
type PageRender = unsafe extern "C" fn(*mut c_void, *mut c_void);
type Unref = unsafe extern "C" fn(*mut c_void);
type ErrorFree = unsafe extern "C" fn(*mut GError);
type SurfaceCreate = unsafe extern "C" fn(c_int, c_int, c_int) -> *mut c_void;
type Status = unsafe extern "C" fn(*mut c_void) -> c_int;
type Create = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
type SetRgb = unsafe extern "C" fn(*mut c_void, c_double, c_double, c_double);
type Paint = unsafe extern "C" fn(*mut c_void);
type Scale = unsafe extern "C" fn(*mut c_void, c_double, c_double);
type Data = unsafe extern "C" fn(*mut c_void) -> *mut u8;
type Stride = unsafe extern "C" fn(*mut c_void) -> c_int;

struct Api {
    new_from_fd: NewFromFd,
    pages: Pages,
    get_page: GetPage,
    page_size: PageSize,
    page_render: PageRender,
    unref: Unref,
    error_free: ErrorFree,
    surface_create: SurfaceCreate,
    surface_status: Status,
    surface_flush: Paint,
    surface_data: Data,
    surface_stride: Stride,
    surface_destroy: Paint,
    create: Create,
    status: Status,
    destroy: Paint,
    set_rgb: SetRgb,
    paint: Paint,
    scale: Scale,
}

// SAFETY: the table holds only function pointers into libraries that stay loaded for the process's life.
unsafe impl Send for Api {}
unsafe impl Sync for Api {}

static API: LazyLock<Result<Api, String>> = LazyLock::new(|| unsafe { load() });

unsafe fn library(name: &CStr) -> Result<*mut c_void, String> {
    let handle = dlopen(name.as_ptr(), RTLD_NOW);
    if handle.is_null() {
        return Err(format!("{} is not loadable", name.to_string_lossy()));
    }
    Ok(handle)
}

unsafe fn symbol<T: Copy>(handle: *mut c_void, name: &CStr) -> Result<T, String> {
    let found = dlsym(handle, name.as_ptr());
    if found.is_null() {
        return Err(format!("no {}", name.to_string_lossy()));
    }
    Ok(std::mem::transmute_copy(&found))
}

unsafe fn load() -> Result<Api, String> {
    let poppler = library(c"libpoppler-glib.so.8")?;
    let gobject = library(c"libgobject-2.0.so.0")?;
    let glib = library(c"libglib-2.0.so.0")?;
    let cairo = library(c"libcairo.so.2")?;
    Ok(Api {
        new_from_fd: symbol(poppler, c"poppler_document_new_from_fd")?,
        pages: symbol(poppler, c"poppler_document_get_n_pages")?,
        get_page: symbol(poppler, c"poppler_document_get_page")?,
        page_size: symbol(poppler, c"poppler_page_get_size")?,
        page_render: symbol(poppler, c"poppler_page_render")?,
        unref: symbol(gobject, c"g_object_unref")?,
        error_free: symbol(glib, c"g_error_free")?,
        surface_create: symbol(cairo, c"cairo_image_surface_create")?,
        surface_status: symbol(cairo, c"cairo_surface_status")?,
        surface_flush: symbol(cairo, c"cairo_surface_flush")?,
        surface_data: symbol(cairo, c"cairo_image_surface_get_data")?,
        surface_stride: symbol(cairo, c"cairo_image_surface_get_stride")?,
        surface_destroy: symbol(cairo, c"cairo_surface_destroy")?,
        create: symbol(cairo, c"cairo_create")?,
        status: symbol(cairo, c"cairo_status")?,
        destroy: symbol(cairo, c"cairo_destroy")?,
        set_rgb: symbol(cairo, c"cairo_set_source_rgb")?,
        paint: symbol(cairo, c"cairo_paint")?,
        scale: symbol(cairo, c"cairo_scale")?,
    })
}

fn api() -> Result<&'static Api, Failure> {
    // SAFETY: load only resolves symbols; their signatures are the libraries' documented C API.
    API.as_ref()
        .map_err(|reason| Failure::Library(reason.clone()))
}

/// Why a document or a page gave nothing: no renderer on this box, or a file that could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    Library(String),
    Unreadable,
}

/// One page's pixels, BGRA rows with no padding, on the document's own white paper.
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// The pixel size a page renders at for a width in device pixels, held inside the edge and pixel caps.
pub fn raster_size(points: (f64, f64), width: u32) -> Option<(u32, u32)> {
    let (w, h) = points;
    if !(w > 0. && h > 0.) || width == 0 {
        return None;
    }
    let mut scale = f64::from(width.min(MAX_EDGE)) / w;
    scale = scale.min(f64::from(MAX_EDGE) / h);
    let pixels = w * h * scale * scale;
    if pixels > MAX_PIXELS as f64 {
        scale *= (MAX_PIXELS as f64 / pixels).sqrt();
    }
    let size = (
        (w * scale).round().max(1.) as u32,
        (h * scale).round().max(1.) as u32,
    );
    Some(size)
}

/// An open document; it lives on the one thread that renders, since a Poppler document is not shared between threads.
pub struct Document {
    raw: *mut c_void,
    api: &'static Api,
}

impl Drop for Document {
    fn drop(&mut self) {
        // SAFETY: raw is the document new_from_fd returned and this is its one reference.
        unsafe { (self.api.unref)(self.raw) }
    }
}

impl Document {
    /// Opens a regular file through its link if it is one; a fifo, a directory or a device is refused before Poppler sees it.
    pub fn open(path: &Path) -> Result<Self, Failure> {
        let api = api()?;
        let file = flea::backend::regfile::open_regular(path).ok_or(Failure::Unreadable)?;
        let fd = file.into_raw_fd();
        let mut error: *mut GError = std::ptr::null_mut();
        // SAFETY: Poppler takes ownership of the descriptor, and error is either left null or set to an owned GError.
        let raw = unsafe { (api.new_from_fd)(fd, std::ptr::null(), &mut error) };
        if !error.is_null() {
            // SAFETY: an error Poppler set is ours to free.
            unsafe { (api.error_free)(error) };
        }
        if raw.is_null() {
            return Err(Failure::Unreadable);
        }
        Ok(Document { raw, api })
    }

    pub fn page_count(&self) -> usize {
        // SAFETY: raw is a live document.
        usize::try_from(unsafe { (self.api.pages)(self.raw) }).unwrap_or(0)
    }

    fn page(&self, index: usize) -> Option<Page<'_>> {
        let index = c_int::try_from(index).ok()?;
        // SAFETY: raw is a live document and Poppler answers null for an index outside it.
        let raw = unsafe { (self.api.get_page)(self.raw, index) };
        (!raw.is_null()).then_some(Page {
            raw,
            api: self.api,
            _document: std::marker::PhantomData,
        })
    }

    /// A page's size in points, as the page shows after its own rotation.
    pub fn page_size(&self, index: usize) -> Option<(f64, f64)> {
        self.page(index).map(|page| page.size())
    }

    /// One page at `width` device pixels on white paper, None when it cannot be drawn.
    pub fn render(&self, index: usize, width: u32) -> Option<Raster> {
        let page = self.page(index)?;
        let points = page.size();
        let (w, h) = raster_size(points, width)?;
        let api = self.api;
        // SAFETY: every cairo object made here is checked and destroyed before return; the page outlives the render.
        unsafe {
            let surface = (api.surface_create)(FORMAT_ARGB32, w as c_int, h as c_int);
            if (api.surface_status)(surface) != 0 {
                (api.surface_destroy)(surface);
                return None;
            }
            let cr = (api.create)(surface);
            (api.set_rgb)(cr, 1., 1., 1.);
            (api.paint)(cr);
            (api.scale)(cr, f64::from(w) / points.0, f64::from(h) / points.1);
            (api.page_render)(page.raw, cr);
            let drawn = (api.status)(cr) == 0;
            (api.destroy)(cr);
            (api.surface_flush)(surface);
            let data = (api.surface_data)(surface);
            let stride = usize::try_from((api.surface_stride)(surface)).unwrap_or(0);
            let row = w as usize * 4;
            let raster = (drawn && !data.is_null() && stride >= row).then(|| {
                let mut bgra = Vec::with_capacity(row * h as usize);
                for y in 0..h as usize {
                    bgra.extend_from_slice(std::slice::from_raw_parts(data.add(y * stride), row));
                }
                Raster {
                    width: w,
                    height: h,
                    bgra,
                }
            });
            (api.surface_destroy)(surface);
            raster
        }
    }
}

struct Page<'a> {
    raw: *mut c_void,
    api: &'static Api,
    _document: std::marker::PhantomData<&'a Document>,
}

impl Page<'_> {
    fn size(&self) -> (f64, f64) {
        let (mut w, mut h) = (0., 0.);
        // SAFETY: raw is a live page and both outputs are valid doubles.
        unsafe { (self.api.page_size)(self.raw, &mut w, &mut h) };
        (w, h)
    }
}

impl Drop for Page<'_> {
    fn drop(&mut self) {
        // SAFETY: raw is the page get_page returned, owned by this value.
        unsafe { (self.api.unref)(self.raw) }
    }
}

/// Whether this box can render a PDF in process; the reason names the library that is missing.
pub fn available() -> Result<(), String> {
    api().map(|_| ()).map_err(|failure| match failure {
        Failure::Library(reason) => reason,
        Failure::Unreadable => String::new(),
    })
}
