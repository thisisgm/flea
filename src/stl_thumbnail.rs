use crate::backend::{model3d, regfile};
use flea::ui_logic_pv4::{
    camera,
    color::Palette,
    raster::{Prepared, Raster, Style},
    scene::Scene,
};
use std::{
    ffi::OsString,
    fs::OpenOptions,
    io::{BufWriter, Read},
    path::PathBuf,
};

const PALETTE: Palette = Palette {
    foreground: [0.65, 0.68, 0.73],
    ground: [0.1, 0.1, 0.12],
    muted: [0.4, 0.4, 0.45],
    frame: [0.5, 0.5, 0.52],
};

/// Render a supported STL input to the sandbox-provided thumbnail file.
pub fn run(args: impl Iterator<Item = OsString>) -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = args.collect();
    let [input, output, size] = args.as_slice() else {
        return Err("expected input, output and thumbnail size".into());
    };
    let input = PathBuf::from(input);
    let output = PathBuf::from(output);
    let size: u32 = size.to_str().ok_or("invalid thumbnail size")?.parse()?;
    if !input.is_absolute() || !output.is_absolute() || !(1..=512).contains(&size) {
        return Err("invalid path or thumbnail size".into());
    }

    let (source, metadata) = regfile::open_if_regular_with_meta(&input, 0)?;
    if metadata.len() > model3d::MAX_INPUT as u64 {
        return Err("3D input exceeds 128 MiB".into());
    }
    let mut bytes = Vec::new();
    source
        .take(model3d::MAX_INPUT as u64 + 1)
        .read_to_end(&mut bytes)?;
    let model = model3d::parse("stl", &bytes)?;
    drop(bytes);
    let scene = Scene {
        positions: model
            .vertices
            .into_iter()
            .map(|vertex| vertex.position)
            .collect(),
        triangles: model
            .triangles
            .into_iter()
            .map(|face| face.vertices)
            .collect(),
        ..Scene::default()
    };
    scene.check()?;

    // Look from +X/-Y/+Z, matching the quadrant of the local F3D thumbnailer.
    // The STL parser emits only positions used by a face.
    let view = camera::frame(
        &scene.positions,
        -camera::ISO_AZ,
        camera::iso_el(),
        size as f32,
        size as f32,
        camera::THUMB_FILL,
        None,
    );
    let prepared = Prepared::new(&scene, PALETTE);
    let mut raster = Raster::default();
    raster.resize(size as usize, size as usize, 2);
    raster.render(
        &scene,
        &prepared,
        0,
        &view,
        &Style {
            palette: PALETTE,
            wire: None,
            layer: None,
            plate: None,
            ground: None,
        },
    );
    let mut rgba = Vec::new();
    raster.rgba_into(&mut rgba);
    if !rgba.chunks_exact(4).any(|pixel| pixel[3] > 0) {
        return Err("the model produced an empty thumbnail".into());
    }

    let file = OpenOptions::new().write(true).truncate(true).open(output)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&rgba)?;
    writer.finish()?;
    Ok(())
}
