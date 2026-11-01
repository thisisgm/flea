// The picture's requested decode size in physical pixels (#336): the drawn fit times the window scale.
pub fn decode_size(
    fit_w: f64,
    fit_h: f64,
    scale: f64,
    src_w: f64,
    src_h: f64,
    vector: bool,
) -> (u32, u32) {
    let sane = |v: f64| v.is_finite() && v > 0.0;
    if !sane(fit_w) || !sane(fit_h) || !sane(scale) {
        return (0, 0);
    }
    let (mut w, mut h) = ((fit_w * scale).ceil() as u32, (fit_h * scale).ceil() as u32);
    if !vector {
        w = w.min(src_w.max(0.0) as u32);
        h = h.min(src_h.max(0.0) as u32);
    }
    bounded_raster(w.max(1), h.max(1))
}

// A vector file over this never reaches the parser: crafted parse cost has no smaller bound.
pub const MAX_SVG_BYTES: u64 = 4 * 1024 * 1024;

// One raster may not pass 8192 a side or 16M pixels (64 MB of BGRA), whatever the request said.
pub const MAX_RASTER_EDGE: u32 = 8192;
pub const MAX_RASTER_PIXELS: u64 = 16_777_216;

// Whether a vector file's length is inside the byte bound a crafted file cannot pass.
pub fn svg_within_byte_bound(len: u64) -> bool {
    len <= MAX_SVG_BYTES
}

// Clamps a request to the raster bounds, keeping its aspect; a request is never padded.
pub fn bounded_raster(w: u32, h: u32) -> (u32, u32) {
    let (mut w, mut h) = (w.max(1), h.max(1));
    if w > MAX_RASTER_EDGE || h > MAX_RASTER_EDGE {
        let s = f64::from(MAX_RASTER_EDGE) / f64::from(w.max(h));
        w = ((f64::from(w) * s).floor() as u32).max(1);
        h = ((f64::from(h) * s).floor() as u32).max(1);
    }
    if u64::from(w) * u64::from(h) > MAX_RASTER_PIXELS {
        let s = (MAX_RASTER_PIXELS as f64 / (f64::from(w) * f64::from(h))).sqrt();
        w = ((f64::from(w) * s).floor() as u32).max(1);
        h = ((f64::from(h) * s).floor() as u32).max(1);
    }
    (w, h)
}

// A scale change (a monitor move, a rescale) requests a new decode at the new size.
pub fn needs_redecode(old_scale: f64, new_scale: f64) -> bool {
    old_scale != new_scale
}

// An SVG rasterizes; anything else decodes from its own pixels. v0.3.12 treats .svgz as a vector.
pub fn is_vector_name(name: &str) -> bool {
    name.rsplit('.')
        .next()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg") || ext.eq_ignore_ascii_case("svgz"))
}

// Whether the vector is gzip-compressed, inflated under the byte bound before parsing.
pub fn is_compressed_vector(name: &str) -> bool {
    name.rsplit('.')
        .next()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svgz"))
}

// A raster kind the card decodes itself reads up to this many bytes; past it the card says the file is too large.
pub const MAX_PICTURE_BYTES: u64 = 256 * 1024 * 1024;
// A source past this many pixels is never decoded: 64 Mi pixels is 256 MB of RGBA before the downscale.
pub const MAX_SOURCE_PIXELS: u64 = 64 * 1024 * 1024;

fn extension_is(name: &str, wanted: &[&str]) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| wanted.iter().any(|w| ext.eq_ignore_ascii_case(w)))
}

// AVIF decodes through the system libheif, which GPUI's own image loader does not carry.
pub fn is_avif_name(name: &str) -> bool {
    extension_is(name, &["avif"])
}

// AVIF and HEIC/HEIF share the system libheif decoder and its bounded raster job.
pub fn is_heif_name(name: &str) -> bool {
    is_avif_name(name) || extension_is(name, &["heic", "heif"])
}

// Decoded in the card's raster job at the drawn size: vectors, HEIF containers, TIFF and ICO.
pub fn is_host_decoded(name: &str) -> bool {
    is_vector_name(name) || is_heif_name(name) || extension_is(name, &["tif", "tiff", "ico"])
}

// The most bytes a host-decoded file's kind reads: a vector's parse bound, or the raster bound.
pub fn decode_byte_bound(name: &str) -> u64 {
    if is_vector_name(name) {
        MAX_SVG_BYTES
    } else {
        MAX_PICTURE_BYTES
    }
}

// Whether a host-decoded file's length is inside its kind's byte bound.
pub fn within_decode_bound(name: &str, len: u64) -> bool {
    len <= decode_byte_bound(name)
}
