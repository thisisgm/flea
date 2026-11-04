//! #336: every preview picture decodes and every SVG rasterizes at physical pixels.
use super::pixels::{
    decode_size, is_avif_name, is_compressed_vector, is_host_decoded, is_vector_name,
    needs_redecode, svg_within_byte_bound, within_decode_bound, MAX_PICTURE_BYTES, MAX_SVG_BYTES,
};

// With scale 2, a 4000x3000 source fitted at 1000x750 logical requests 2000x1500.
#[test]
fn hidpi_fit_requests_physical_pixels() {
    assert_eq!(
        decode_size(1000.0, 750.0, 2.0, 4000.0, 3000.0, false),
        (2000, 1500)
    );
}

// An 800x600 source requests 800x600: the request never passes the source's own size.
#[test]
fn small_source_bounds_the_request() {
    assert_eq!(
        decode_size(1000.0, 750.0, 2.0, 800.0, 600.0, false),
        (800, 600)
    );
}

// With scale 1 the same fit requests 1000x750.
#[test]
fn scale_one_requests_the_fit() {
    assert_eq!(
        decode_size(1000.0, 750.0, 1.0, 4000.0, 3000.0, false),
        (1000, 750)
    );
}

// An SVG zoomed 2x at scale 2 rasterizes at 4 times its logical fit, unbounded by any source size.
#[test]
fn zoomed_svg_rasterizes_at_zoom_times_scale() {
    assert_eq!(
        decode_size(2000.0, 1500.0, 2.0, 0.0, 0.0, true),
        (4000, 3000)
    );
}

// A crafted or giant request clamps to the raster bounds: 8192 a side and 16M pixels at most.
#[test]
fn a_giant_request_clamps_to_the_raster_bound() {
    let (w, h) = decode_size(100_000.0, 100_000.0, 4.0, 0.0, 0.0, true);
    assert!(
        w <= 8192 && h <= 8192,
        "{w}x{h} stays inside the edge bound"
    );
    assert!(
        u64::from(w) * u64::from(h) <= 16_777_216,
        "{w}x{h} stays inside the pixel bound"
    );
}

// A crafted file over the byte bound never reaches the parser; one at the bound does.
#[test]
fn a_vector_file_over_the_byte_bound_is_refused() {
    assert!(svg_within_byte_bound(4 * 1024 * 1024));
    assert!(!svg_within_byte_bound(4 * 1024 * 1024 + 1));
}

// A monitor move rescales the window, so the picture decodes again; the same scale decodes once.
#[test]
fn scale_change_requests_a_new_decode() {
    assert!(needs_redecode(1.0, 2.0));
    assert!(!needs_redecode(2.0, 2.0));
}

// The rasterizer runs for SVG alone, in any case.
#[test]
fn vector_names_its_extension() {
    assert!(is_vector_name("mark.svg"));
    assert!(is_vector_name("MARK.SVG"));
    assert!(!is_vector_name("photo.png"));
    assert!(!is_vector_name("nosuffix"));
}

// A compressed vector inflates before parsing, in any case.
#[test]
fn compressed_vector_names_its_extension() {
    assert!(is_vector_name("mark.svgz"));
    assert!(is_vector_name("MARK.SVGZ"));
    assert!(is_compressed_vector("mark.svgz"));
    assert!(!is_compressed_vector("mark.svg"));
}

// The kinds GPUI's own img(path) cannot read, or would hold whole, decode in the card's raster job.
#[test]
fn host_decoded_names_cover_required_formats_and_no_others() {
    for name in [
        "a.svg", "a.SVGZ", "a.avif", "A.AVIF", "a.heic", "A.HEIC", "a.heif", "A.HEIF", "a.tiff",
        "a.tif", "a.TIF", "a.ico",
    ] {
        assert!(is_host_decoded(name), "{name}");
    }
    for name in [
        "a.png", "a.jpg", "a.webp", "a.gif", "a.bmp", "tiff", "avif", "heic", "heif",
    ] {
        assert!(!is_host_decoded(name), "{name}");
    }
    assert!(is_avif_name("a.Avif"));
    assert!(!is_avif_name("a.tiff"));
    assert!(!is_avif_name("a.heic"));
}

// A vector keeps its 4 MB parse bound; a raster kind reads up to its own, larger bound.
#[test]
fn each_host_decoded_kind_has_its_own_byte_bound() {
    assert!(within_decode_bound("a.svg", MAX_SVG_BYTES));
    assert!(!within_decode_bound("a.svgz", MAX_SVG_BYTES + 1));
    assert!(within_decode_bound("a.tiff", MAX_SVG_BYTES + 1));
    assert!(within_decode_bound("a.avif", MAX_PICTURE_BYTES));
    assert!(within_decode_bound("a.heic", MAX_PICTURE_BYTES));
    assert!(!within_decode_bound("a.heif", MAX_PICTURE_BYTES + 1));
    assert!(!within_decode_bound("a.ico", MAX_PICTURE_BYTES + 1));
}
