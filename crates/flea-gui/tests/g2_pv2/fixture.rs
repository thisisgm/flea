//! Small PDFs written byte by byte: each page a size, a rotation and a black box over its unrotated top left quarter.
use std::fmt::Write as _;

#[derive(Clone, Copy)]
pub struct Page {
    pub width: f64,
    pub height: f64,
    pub rotate: u32,
}

pub const LETTER: Page = Page {
    width: 612.,
    height: 792.,
    rotate: 0,
};
pub const LANDSCAPE: Page = Page {
    width: 792.,
    height: 612.,
    rotate: 0,
};
pub const TURNED: Page = Page {
    width: 612.,
    height: 792.,
    rotate: 90,
};

/// A document of these pages, with a cross reference table whose offsets are exact.
pub fn pdf(pages: &[Page]) -> Vec<u8> {
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    let count = pages.len();
    let kids: Vec<String> = (0..count).map(|i| format!("{} 0 R", 3 + 2 * i)).collect();
    let mut object = |out: &mut String, body: String| {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n{}\nendobj\n", offsets.len(), body);
    };
    object(&mut out, "<< /Type /Catalog /Pages 2 0 R >>".into());
    object(
        &mut out,
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            count
        ),
    );
    for (i, page) in pages.iter().enumerate() {
        let (w, h) = (page.width, page.height);
        object(
            &mut out,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Rotate {} /Contents {} 0 R /Resources << >> >>",
                page.rotate,
                4 + 2 * i
            ),
        );
        let stream = format!("0 g 0 {} {} {} re f", h / 2., w / 2., h / 2.);
        object(
            &mut out,
            format!(
                "<< /Length {} >>\nstream\n{}\nendstream",
                stream.len(),
                stream
            ),
        );
    }
    let xref = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1);
    for offset in &offsets {
        let _ = writeln!(out, "{offset:010} 00000 n ");
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
        offsets.len() + 1,
        xref
    );
    out.into_bytes()
}

/// The BGRA pixel at a fraction of the raster's width and height.
pub fn pixel(bgra: &[u8], width: u32, height: u32, fx: f64, fy: f64) -> [u8; 4] {
    let x = ((f64::from(width) * fx) as usize).min(width as usize - 1);
    let y = ((f64::from(height) * fy) as usize).min(height as usize - 1);
    let at = (y * width as usize + x) * 4;
    [bgra[at], bgra[at + 1], bgra[at + 2], bgra[at + 3]]
}
