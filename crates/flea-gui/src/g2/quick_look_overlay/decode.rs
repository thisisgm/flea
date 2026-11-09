//! The card's own TIFF and ICO decode through the image crate, and AVIF through the system libheif (SW-13).
use super::Rasterizer;
use crate::g2::media_preview::helper::dl::{symbols, Library};
use flea::ui_logic_preview_core::pixels::MAX_SOURCE_PIXELS;
use gpui::RenderImage;
use image::{DynamicImage, ImageDecoder, ImageReader, RgbaImage};
use std::ffi::{c_char, c_int, c_void, CStr};
use std::io::Cursor;
use std::sync::{Arc, Mutex, OnceLock};

/// The soname libheif 1.x ships; a box without it shows AVIF as a file that could not be read.
pub const LIBHEIF: &str = "libheif.so.1";

// heif_colorspace_RGB, heif_chroma_interleaved_RGBA and heif_channel_interleaved from libheif's heif.h.
const COLORSPACE_RGB: c_int = 1;
const CHROMA_RGBA: c_int = 11;
const CHANNEL_INTERLEAVED: c_int = 10;

// struct heif_error, returned by value: code 0 is heif_error_Ok, and the message is owned by libheif.
#[repr(C)]
#[derive(Clone, Copy)]
struct HeifError {
    code: c_int,
    subcode: c_int,
    message: *const c_char,
}

impl HeifError {
    fn check(self, call: &str) -> Result<(), String> {
        if self.code == 0 {
            return Ok(());
        }
        let detail = if self.message.is_null() {
            String::new()
        } else {
            // SAFETY: libheif returns a NUL-terminated static or context-owned message string.
            unsafe { CStr::from_ptr(self.message) }
                .to_string_lossy()
                .into_owned()
        };
        Err(format!("{call}: {detail} ({}.{})", self.code, self.subcode))
    }
}

type Init = unsafe extern "C" fn(*mut c_void) -> HeifError;
type Alloc = unsafe extern "C" fn() -> *mut c_void;
type ReadMemory =
    unsafe extern "C" fn(*mut c_void, *const c_void, usize, *const c_void) -> HeifError;
type Primary = unsafe extern "C" fn(*mut c_void, *mut *mut c_void) -> HeifError;
type Side = unsafe extern "C" fn(*const c_void) -> c_int;
type ChannelSide = unsafe extern "C" fn(*const c_void, c_int) -> c_int;
type Decode =
    unsafe extern "C" fn(*const c_void, *mut *mut c_void, c_int, c_int, *const c_void) -> HeifError;
type Plane = unsafe extern "C" fn(*const c_void, c_int, *mut c_int) -> *const u8;
type Release = unsafe extern "C" fn(*const c_void);
type Free = unsafe extern "C" fn(*mut c_void);

/// libheif's entry points; plain function pointers, so the table is shared across the raster jobs.
#[derive(Clone, Copy)]
struct Heif {
    alloc: Alloc,
    read: ReadMemory,
    primary: Primary,
    handle_width: Side,
    handle_height: Side,
    decode: Decode,
    image_width: ChannelSide,
    image_height: ChannelSide,
    plane: Plane,
    release_image: Release,
    release_handle: Release,
    free: Free,
}

impl Heif {
    fn open(library: &str) -> Result<Self, String> {
        let lib = Library::open(library)?;
        // SAFETY: heif_init's documented signature; a null parameter block is allowed.
        let init: Init = unsafe { lib.get(b"heif_init\0")? };
        // SAFETY: heif_init is the first call into the library, as heif.h asks.
        unsafe { init(std::ptr::null_mut()) }.check("heif_init")?;
        let heif = symbols!(
            lib,
            Heif {
                alloc: Alloc = "heif_context_alloc",
                read: ReadMemory = "heif_context_read_from_memory_without_copy",
                primary: Primary = "heif_context_get_primary_image_handle",
                handle_width: Side = "heif_image_handle_get_width",
                handle_height: Side = "heif_image_handle_get_height",
                decode: Decode = "heif_decode_image",
                image_width: ChannelSide = "heif_image_get_width",
                image_height: ChannelSide = "heif_image_get_height",
                plane: Plane = "heif_image_get_plane_readonly",
                release_image: Release = "heif_image_release",
                release_handle: Release = "heif_image_handle_release",
                free: Free = "heif_context_free",
            }
        );
        // Library never calls dlclose, so the table stays valid after `lib` goes out of scope.
        Ok(heif)
    }

    /// The primary image, upright (libheif applies irot and imir), as straight RGBA rows.
    fn decode(&self, bytes: &[u8]) -> Result<RgbaImage, String> {
        struct Owned<'a>(&'a Heif, *mut c_void, *mut c_void, *mut c_void);
        impl Drop for Owned<'_> {
            fn drop(&mut self) {
                // SAFETY: each pointer is null or was handed out by the matching libheif call, released once here.
                unsafe {
                    if !self.3.is_null() {
                        (self.0.release_image)(self.3);
                    }
                    if !self.2.is_null() {
                        (self.0.release_handle)(self.2);
                    }
                    if !self.1.is_null() {
                        (self.0.free)(self.1);
                    }
                }
            }
        }
        // SAFETY: heif_context_alloc takes no input; a null answer is refused below.
        let mut held = Owned(
            self,
            unsafe { (self.alloc)() },
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        if held.1.is_null() {
            return Err("heif_context_alloc returned null".into());
        }
        // SAFETY: `bytes` outlives the context, which `held` frees before this function returns.
        unsafe { (self.read)(held.1, bytes.as_ptr().cast(), bytes.len(), std::ptr::null()) }
            .check("read")?;
        // SAFETY: a live context and an out pointer the call fills.
        unsafe { (self.primary)(held.1, &mut held.2) }.check("primary image")?;
        // SAFETY: a live handle.
        let sides = unsafe { ((self.handle_width)(held.2), (self.handle_height)(held.2)) };
        bounded(sides)?;
        // SAFETY: a live handle, an out pointer, and libheif's documented RGB/RGBA enums; null options take defaults.
        unsafe {
            (self.decode)(
                held.2,
                &mut held.3,
                COLORSPACE_RGB,
                CHROMA_RGBA,
                std::ptr::null(),
            )
        }
        .check("decode")?;
        // SAFETY: a live decoded image; its own sides size the copy, as a clean-aperture crop can make it smaller.
        let (w, h) = bounded(unsafe {
            (
                (self.image_width)(held.3, CHANNEL_INTERLEAVED),
                (self.image_height)(held.3, CHANNEL_INTERLEAVED),
            )
        })?;
        let mut stride: c_int = 0;
        // SAFETY: a live decoded image; the plane lives until heif_image_release in `held`'s drop.
        let plane = unsafe { (self.plane)(held.3, CHANNEL_INTERLEAVED, &mut stride) };
        let row = w as usize * 4;
        let stride = usize::try_from(stride).map_err(|_| "negative stride")?;
        if plane.is_null() || stride < row {
            return Err("no interleaved RGBA plane".into());
        }
        let mut pixels = Vec::with_capacity(row * h as usize);
        for y in 0..h as usize {
            // SAFETY: libheif's plane holds `h` rows of `stride` bytes, each at least `row` long (checked above).
            pixels.extend_from_slice(unsafe {
                std::slice::from_raw_parts(plane.add(y * stride), row)
            });
        }
        RgbaImage::from_raw(w, h, pixels).ok_or_else(|| "plane size mismatch".into())
    }
}

// libheif answers -1 for a missing channel; a side past the source bound is never decoded or copied.
fn bounded((w, h): (c_int, c_int)) -> Result<(u32, u32), String> {
    match (u32::try_from(w), u32::try_from(h)) {
        (Ok(w), Ok(h)) if w > 0 && h > 0 && u64::from(w) * u64::from(h) <= MAX_SOURCE_PIXELS => {
            Ok((w, h))
        }
        _ => Err(format!("{w}x{h} is outside the decode bound")),
    }
}

/// Whether the named library file loads at all; a box without it skips the AVIF decode tests.
pub fn heif_present(library: &str) -> bool {
    Library::open(library).is_ok()
}

/// Whether the named libheif loads, initialises and names every entry point the decode uses.
pub fn heif_loads(library: &str) -> bool {
    Heif::open(library).is_ok()
}

type Opened = Vec<(&'static str, Result<Heif, String>)>;

// One open per soname for the process: a missing library is tried once, and its reason is kept for every job.
fn heif(library: &'static str) -> Result<Heif, String> {
    static OPENED: OnceLock<Mutex<Opened>> = OnceLock::new();
    let mut opened = OPENED
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "libheif table poisoned")?;
    if let Some((_, found)) = opened.iter().find(|(name, _)| *name == library) {
        return found.clone();
    }
    let found = Heif::open(library);
    opened.push((library, found.clone()));
    found
}

/// AVIF through `library`, decoded whole (AV1 has no reduced-size decode) and held at the requested size.
pub fn heif_raster(library: &'static str) -> Rasterizer {
    Arc::new(move |bytes: &[u8], at: (u32, u32)| {
        let upright = heif(library)?.decode(bytes)?;
        Ok(held_at(upright, at))
    })
}

/// TIFF and ICO through the image crate, turned upright by the file's orientation as GPUI's own loader does.
pub fn picture_raster() -> Rasterizer {
    Arc::new(|bytes: &[u8], at: (u32, u32)| {
        let reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
        let (w, h) = decoder.dimensions();
        if u64::from(w) * u64::from(h) > MAX_SOURCE_PIXELS {
            return Err(format!("{w}x{h} is outside the decode bound"));
        }
        let orientation = decoder.orientation().map_err(|e| e.to_string())?;
        let mut image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
        image.apply_orientation(orientation);
        Ok(held_at(image.into_rgba8(), at))
    })
}

/// The decoded source reduced to the request with Catmull-Rom; a smaller source keeps its own size.
fn held_at(source: RgbaImage, (w, h): (u32, u32)) -> Arc<RenderImage> {
    let size = super::photo_frames::within_source(source.dimensions(), (w, h));
    let held = super::photo_frames::resized_bgra(source, size);
    Arc::new(RenderImage::new(vec![image::Frame::new(held)]))
}
