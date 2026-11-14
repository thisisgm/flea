//! 3D model and toolpath previews, shared by the window and the terminal: camera, motion, rasterizer, facts and wire.
pub mod camera;
pub mod color;
pub mod control;
#[cfg(test)]
mod control_tests;
pub mod facts;
#[cfg(test)]
mod facts_tests;
#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod locale_tests;
pub mod motion;
pub mod place;
pub mod raster;
#[cfg(test)]
mod raster_tests;
#[cfg(test)]
mod refusal_tests;
pub mod scene;
#[cfg(test)]
mod tests;
pub mod vec;
pub mod wire;

use color::Palette;
use raster::{Prepared, Raster, Style};
use scene::Scene;

/// Flea's own isometric picture of plate `plate`, `w` by `h` RGBA on a transparent ground, for a file without an embedded one, `None` for a scene that fails its check; it draws in the two shades of the foreground role, so it follows the palette it is given.
pub fn render_thumbnail(
    scene: &Scene,
    plate: usize,
    w: usize,
    h: usize,
    palette: Palette,
) -> Option<Vec<u8>> {
    scene.check().ok()?;
    let view = camera::thumbnail(scene, plate, w as f32, h as f32)?;
    let prepared = Prepared::new(scene, palette);
    let mut raster = Raster::default();
    raster.resize(w, h, 2);
    let style = Style {
        palette,
        wire: None,
        layer: None,
        plate: None,
        ground: None,
    };
    raster.render(scene, &prepared, plate, &view, &style);
    let mut out = Vec::with_capacity(w * h * 4);
    raster.rgba_into(&mut out);
    Some(out)
}
