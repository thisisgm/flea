//! A helper's PNG read and decoded off the UI thread into the frame GPUI's atlas takes, BGRA as the PDF pages are.
use gpui::RenderImage;
use std::sync::Arc;

/// The decoded picture and its pixel size; None for a missing, unreadable or malformed file.
pub fn png(path: &str) -> Option<(Arc<RenderImage>, (u32, u32))> {
    let bytes = std::fs::read(path).ok()?;
    let mut rgba = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .ok()?
        .into_rgba8();
    for pixel in rgba.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    let size = rgba.dimensions();
    Some((
        Arc::new(RenderImage::new(vec![image::Frame::new(rgba)])),
        size,
    ))
}

impl super::HtmlView {
    pub(super) fn decoded(
        &mut self,
        token: u64,
        decoded: Option<(Arc<RenderImage>, (u32, u32))>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.decode = None;
        self.doc.picture_loaded(token, decoded.is_some());
        if token != self.doc.image_token {
            return;
        }
        if let Some((image, size)) = decoded {
            self.decoded += 1;
            if let Some(old) = self.image.replace((token, image, size)) {
                self.released.push(old.1);
            }
        }
        cx.notify();
    }
}
