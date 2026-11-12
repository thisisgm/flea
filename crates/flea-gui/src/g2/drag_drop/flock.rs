//! The mouse drag borrows Carry's approved flock geometry and gives the platform the same image.
use super::Drags;
use crate::{
    g2::file_clipboard::{carry, flock::FlockElement},
    list::Palette,
    row::marks::Mark,
    window::pane::PaneHost,
};
use gpui::{
    div, prelude::*, px, size, App, AppContext, Context, DevicePixels, ExternalDragImage,
    ExternalDragPayload, Render, SvgSize, TextRun, Window,
};
use std::rc::Rc;

pub struct Flock {
    paint: Option<FlockElement>,
    image: Option<ExternalDragImage>,
    /// The family the platform image's name was drawn in.
    #[cfg(test)]
    pub(crate) family: gpui::SharedString,
}

impl Flock {
    pub(super) fn native(&self, payload: ExternalDragPayload) -> ExternalDragPayload {
        let Some(image) = &self.image else {
            return payload;
        };
        payload
            .clone()
            .with_image(
                image.rgba().clone(),
                image.width(),
                image.height(),
                image.scale(),
                image.hotspot(),
            )
            .unwrap_or(payload)
    }
}

impl Render for Flock {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let paint = self.paint.clone().map(|mut paint| {
            paint.at = window.mouse_position();
            paint
        });
        div()
            .debug_selector(|| "file-drag-flock".into())
            .children(paint)
    }
}

pub(super) fn captured(
    host: &PaneHost,
    rows: &[usize],
    window: &Window,
    cx: &mut App,
) -> gpui::Entity<Flock> {
    let lang_marks = host.list.borrow().view.lang_marks;
    let flock = carry::Flock {
        marks: rows
            .iter()
            .take(carry::LAYERS.len())
            .filter_map(|&i| host.pane.held.row(i))
            .map(|row| Mark::for_facts(row, lang_marks))
            .collect(),
        name: rows
            .first()
            .and_then(|&i| host.pane.held.row(i))
            .map_or_else(String::new, |row| row.name.clone()),
    };
    pictured(host, flock, window, cx)
}

/// The drag's in-window picture and platform image of a flock, drawn in the pane's own typeface.
pub(super) fn pictured(
    host: &PaneHost,
    flock: carry::Flock,
    window: &Window,
    cx: &mut App,
) -> gpui::Entity<Flock> {
    let flock = Rc::new(flock);
    let list = host.list.borrow();
    let theme = cx
        .try_global::<crate::palette::Theme>()
        .copied()
        .unwrap_or_default();
    let palette = Palette::inherited(&window.text_style(), theme);
    let font = list.typeface.font.clone();
    let image = raster(&flock, &font, palette, window, cx);
    let paint = Some(FlockElement::new(
        flock,
        window.mouse_position(),
        palette.background,
        palette.foreground,
    ));
    cx.new(|_| Flock {
        paint,
        image,
        #[cfg(test)]
        family: font.family,
    })
}

pub fn flock(cx: &mut App) -> gpui::Entity<Flock> {
    let active = cx.try_global::<Drags>().and_then(|drags| {
        let (key, _) = drags.lifts.iter().max_by_key(|(_, lift)| lift.serial)?;
        drags.flocks.get(key).cloned()
    });
    active.unwrap_or_else(|| {
        cx.new(|_| Flock {
            paint: None,
            image: None,
            #[cfg(test)]
            family: Default::default(),
        })
    })
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn ink(color: gpui::Hsla) -> String {
    let rgb = color.to_rgb();
    format!(
        "#{:02x}{:02x}{:02x}",
        (rgb.r * 255.).round() as u8,
        (rgb.g * 255.).round() as u8,
        (rgb.b * 255.).round() as u8
    )
}

fn raster(
    flock: &carry::Flock,
    font: &gpui::Font,
    palette: Palette,
    window: &Window,
    cx: &App,
) -> Option<ExternalDragImage> {
    let name: gpui::SharedString = flock.name.clone().into();
    // The listing's own face: the SVG font database cannot resolve GPUI's default family and falls back to a symbol font.
    let line = window.text_system().shape_line(
        name.clone(),
        px(carry::NAME_SIZE),
        &[TextRun {
            len: name.len(),
            font: font.clone(),
            color: palette.foreground,
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        None,
    );
    let scale = window.scale_factor().ceil().clamp(1., 8.) as u32;
    let width = (f32::from(line.width).ceil() as u32 + 64).clamp(64, 1024 / scale);
    let height = 60;
    let fg = ink(palette.foreground);
    let ground = ink(palette.background);
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" color="{fg}">"#
    );
    for (mark, (dx, dy, alpha)) in flock.marks.iter().zip(carry::LAYERS).rev() {
        let (x, y) = (carry::OFFSET.0 + dx, carry::OFFSET.1 + dy);
        svg.push_str(&format!(r#"<g transform="translate({x} {y})" opacity="{}"><rect width="19" height="19" fill="{ground}"/><svg width="19" height="19" viewBox="0 0 24 24">{}</svg></g>"#, alpha * carry::OPACITY, std::str::from_utf8(mark.svg().1).ok()?));
    }
    svg.push_str(&format!(r#"<text x="47" y="42" font-family="&apos;{}&apos;, monospace" font-size="14" fill="{fg}" opacity="0.8">{}</text></svg>"#, xml(&font.family), xml(&name)));
    let renderer = cx.svg_renderer();
    let text_system = window.text_system();
    let mut ids = Vec::new();
    let mut fonts = Vec::new();
    for run in &line.runs {
        if !ids.contains(&run.font_id) {
            ids.push(run.font_id);
            fonts.push(text_system.font_data(run.font_id)?);
        }
    }
    let parsed = renderer.parse_svg_with_fonts(svg.as_bytes(), fonts).ok()?;
    let image = renderer
        .render_parsed(
            &parsed,
            SvgSize::ExactSize(size(
                DevicePixels((width * scale) as i32),
                DevicePixels((height * scale) as i32),
            )),
        )
        .ok()?;
    let mut rgba = image.as_bytes(0)?.to_vec();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    ExternalDragImage::new(rgba.into(), width * scale, height * scale, scale, (0, 0)).ok()
}

#[cfg(test)]
#[path = "first_drag_tests.rs"]
mod first_drag_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::g2::drag_drop::mounted_tests::{open, Root};
    #[test]
    fn vf_look_review_native_name_pixels_use_the_listing_font() {
        let mut cx = crate::list::bench::real_cx();
        let root = Root::new("flock-pixels");
        let (handle, _) = open(&mut cx, root.src());
        handle.update(&mut cx, |shell, window, cx| {
            let font = shell.active_typeface().font;
            let palette = Palette::inherited(&window.text_style(), crate::palette::Theme::default());
            let flock = carry::Flock { marks: vec![], name: "alpha.txt".into() };
            let image = raster(&flock, &font, palette, window, cx).unwrap();
            let width = image.width() / image.scale();
            let height = image.height() / image.scale();
            let color = ink(palette.foreground);
            let reference = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}"><text x="47" y="42" font-family="&apos;{}&apos;, monospace" font-size="14" fill="{color}" opacity="0.8">alpha.txt</text></svg>"#,
                xml(&font.family)
            );
            let renderer = cx.svg_renderer();
            let parsed = renderer.parse_svg(reference.as_bytes()).unwrap();
            let expected = renderer.render_parsed(&parsed, SvgSize::ExactSize(size(
                DevicePixels(image.width() as i32), DevicePixels(image.height() as i32)
            ))).unwrap();
            let mut expected = expected.as_bytes(0).unwrap().to_vec();
            for pixel in expected.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
            assert!(expected.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0));
            assert_eq!(image.rgba().as_ref(), expected.as_slice(),
                "the native image must contain the listing font's rendered glyphs");
        }).unwrap();
    }
}
