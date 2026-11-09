//! Scaled photo decoding through GdkPixbuf, which requests decoder-side sizing before allocating the picture.
use gpui::RenderImage;
use std::{
    ffi::{c_char, c_int, c_void, CString},
    io::{Read, Seek, SeekFrom},
    os::fd::AsRawFd,
    path::Path,
    sync::Arc,
};

type Pixbuf = c_void;

#[link(
    name = "libgdk_pixbuf-2.0.so.0",
    kind = "dylib",
    modifiers = "+verbatim"
)]
extern "C" {
    fn gdk_pixbuf_new_from_file_at_scale(
        path: *const c_char,
        width: c_int,
        height: c_int,
        preserve: c_int,
        error: *mut *mut c_void,
    ) -> *mut Pixbuf;
    fn gdk_pixbuf_apply_embedded_orientation(source: *mut Pixbuf) -> *mut Pixbuf;
    fn gdk_pixbuf_scale_simple(
        source: *mut Pixbuf,
        width: c_int,
        height: c_int,
        interpolation: c_int,
    ) -> *mut Pixbuf;
    fn gdk_pixbuf_get_width(source: *const Pixbuf) -> c_int;
    fn gdk_pixbuf_get_height(source: *const Pixbuf) -> c_int;
    fn gdk_pixbuf_get_rowstride(source: *const Pixbuf) -> c_int;
    fn gdk_pixbuf_get_n_channels(source: *const Pixbuf) -> c_int;
    fn gdk_pixbuf_get_pixels(source: *const Pixbuf) -> *mut u8;
}
#[link(name = "libgobject-2.0.so.0", kind = "dylib", modifiers = "+verbatim")]
extern "C" {
    fn g_object_unref(object: *mut c_void);
}
#[link(name = "libglib-2.0.so.0", kind = "dylib", modifiers = "+verbatim")]
extern "C" {
    fn g_error_free(error: *mut c_void);
}

struct Picture(*mut Pixbuf);
impl Picture {
    fn of(pointer: *mut Pixbuf) -> Result<Self, ()> {
        if pointer.is_null() {
            Err(())
        } else {
            Ok(Self(pointer))
        }
    }
}
impl Drop for Picture {
    fn drop(&mut self) {
        // SAFETY: Picture owns exactly one live GObject reference from its constructor.
        unsafe {
            g_object_unref(self.0);
        }
    }
}

pub(super) fn decode(
    path: &Path,
    target: (u32, u32),
    current: &dyn Fn() -> bool,
) -> Result<Arc<RenderImage>, ()> {
    let (w, h) = target;
    if w == 0 || h == 0 || flea::ui_logic_pv1::pixels::bounded_raster(w, h) != target {
        return Err(());
    }
    let mut file = flea::backend::regfile::open_regular(path).ok_or(())?;
    let mut magic = [0; 32];
    let read = file.read(&mut magic).map_err(|_| ())?;
    file.seek(SeekFrom::Start(0)).map_err(|_| ())?;
    // A renamed SVG must not bypass the bounded vector parser by reaching a pixbuf loader.
    let format = image::guess_format(&magic[..read]).map_err(|_| ())?;
    if format == image::ImageFormat::Png {
        crate::preview_quality::reuse::decoded(path);
        return super::photo_png_first::decode(file, target, current);
    }
    // PNG/JPEG bound decode memory; animations and rare-codec fallbacks retain their source buffers.
    if matches!(format, image::ImageFormat::Gif | image::ImageFormat::WebP) {
        if let Some(animation) = super::photo_frames::animated(
            file.try_clone().map_err(|_| ())?,
            format,
            target,
            current,
        )? {
            crate::preview_quality::reuse::decoded(path);
            return Ok(animation);
        }
        file.seek(SeekFrom::Start(0)).map_err(|_| ())?;
    }
    crate::preview_quality::reuse::decoded(path);
    // The decoder opens the pinned regular handle, so a path swapped for a FIFO cannot hold the worker.
    let pinned = CString::new(format!("/proc/self/fd/{}", file.as_raw_fd())).map_err(|_| ())?;
    let edge = w.max(h) as c_int;
    // SAFETY: the path lives through the synchronous decode; error and returned GObjects are owned below.
    unsafe {
        let mut error = std::ptr::null_mut();
        let decoded = gdk_pixbuf_new_from_file_at_scale(pinned.as_ptr(), edge, edge, 1, &mut error);
        if !error.is_null() {
            g_error_free(error);
        }
        if decoded.is_null() {
            // Preserve GPUI's other raster formats when this system has no pixbuf loader for one.
            file.seek(SeekFrom::Start(0)).map_err(|_| ())?;
            return super::photo_frames::static_image(path, file, format, target, current);
        }
        let decoded = Picture::of(decoded)?;
        let upright = Picture::of(gdk_pixbuf_apply_embedded_orientation(decoded.0))?;
        // A one-pixel aspect-rounding difference is filled at the requested fit, as PreviewImage's Stretch does.
        let exact = if gdk_pixbuf_get_width(upright.0) != w as c_int
            || gdk_pixbuf_get_height(upright.0) != h as c_int
        {
            Picture::of(gdk_pixbuf_scale_simple(
                upright.0, w as c_int, h as c_int, 2,
            ))?
        } else {
            upright
        };
        let channels = gdk_pixbuf_get_n_channels(exact.0) as usize;
        let stride = gdk_pixbuf_get_rowstride(exact.0) as usize;
        let pixels = gdk_pixbuf_get_pixels(exact.0);
        if !(3..=4).contains(&channels) || stride < w as usize * channels || pixels.is_null() {
            return Err(());
        }
        let mut bgra = Vec::with_capacity(w as usize * h as usize * 4);
        for y in 0..h as usize {
            let row = std::slice::from_raw_parts(pixels.add(y * stride), w as usize * channels);
            for pixel in row.chunks_exact(channels) {
                bgra.extend_from_slice(&[
                    pixel[2],
                    pixel[1],
                    pixel[0],
                    if channels == 4 { pixel[3] } else { 255 },
                ]);
            }
        }
        let image = image::RgbaImage::from_raw(w, h, bgra).ok_or(())?;
        Ok(Arc::new(RenderImage::new(vec![image::Frame::new(image)])))
    }
}

/// True when the file's own bytes say PNG, whatever its name.
pub(crate) fn is_png(path: &Path) -> bool {
    let mut magic = [0; 8];
    flea::backend::regfile::open_regular(path)
        .is_some_and(|mut file| file.read_exact(&mut magic).is_ok())
        && image::guess_format(&magic).is_ok_and(|format| format == image::ImageFormat::Png)
}

/// Full detail replaces the first scaled picture on the existing background executor.
pub(crate) fn decode_sharp(
    path: &Path,
    target: (u32, u32),
    current: &dyn Fn() -> bool,
) -> Result<Arc<RenderImage>, ()> {
    if !current()
        || target.0 == 0
        || target.1 == 0
        || flea::ui_logic_pv1::pixels::bounded_raster(target.0, target.1) != target
    {
        return Err(());
    }
    let mut file = flea::backend::regfile::open_regular(path).ok_or(())?;
    let mut magic = [0; 32];
    let read = file.read(&mut magic).map_err(|_| ())?;
    file.seek(SeekFrom::Start(0)).map_err(|_| ())?;
    let format = image::guess_format(&magic[..read]).map_err(|_| ())?;
    if format == image::ImageFormat::Png {
        crate::preview_quality::reuse::decoded(path);
        return super::photo_png::decode(file, target, current);
    }
    if matches!(format, image::ImageFormat::Gif | image::ImageFormat::WebP) {
        if let Some(image) = super::photo_frames::animated(
            file.try_clone().map_err(|_| ())?,
            format,
            target,
            current,
        )? {
            crate::preview_quality::reuse::decoded(path);
            return Ok(image);
        }
        file.seek(SeekFrom::Start(0)).map_err(|_| ())?;
    }
    if !current() {
        return Err(());
    }
    super::photo_frames::static_image(path, file, format, target, current)
}
