//! The bounded backend cache handoff, without original-file parsers or decoders.
#[path = "backend/model3d/limits.rs"]
#[allow(dead_code)]
mod limits;
use limits::*;
#[path = "ui_logic_model_preview/model_wire_types.rs"]
mod cache_types;
use cache_types::*;
#[path = "ui_logic_model_preview/model_wire.rs"]
mod reader;
use std::sync::atomic::{AtomicBool, Ordering};

struct Checked<'a, R> {
    input: R,
    cancel: &'a AtomicBool,
}

impl<R: std::io::Read> std::io::Read for Checked<'_, R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::other("model render cancelled"));
        }
        self.input.read(bytes)
    }
}

fn decode(input: impl std::io::Read, cancel: &AtomicBool) -> Result<Model> {
    reader::read(std::io::BufReader::new(Checked { input, cancel }))
}

/// Reads only a regular cached FL3D file, never an original STL, project or toolpath.
pub fn read(path: &std::path::Path) -> Result<crate::ui_logic_model_preview::scene::Scene> {
    read_cancellable(path, &AtomicBool::new(false))
}

/// The TUI's retired render can stop while the shared cache decoder is reading.
pub fn read_cancellable(
    path: &std::path::Path,
    cancel: &AtomicBool,
) -> Result<crate::ui_logic_model_preview::scene::Scene> {
    use crate::ui_logic_model_preview::scene::{bed_from_metadata, Layer, Scene, Segment};
    let (file, metadata) =
        crate::backend::regfile::open_if_regular_with_meta(path, crate::oflags::O_NOFOLLOW)
            .map_err(|error| error.to_string())?;
    if metadata.len() > reader::MAX_WIRE as u64 {
        return Err("3D wire exceeds its bound".into());
    }
    let model = decode(file, cancel)?;
    let mut scene = Scene {
        positions: model.vertices.iter().map(|v| v.position).collect(),
        bed: bed_from_metadata(model.metadata.iter().map(|(k, v)| (k.as_str(), v.as_str()))),
        segments: model
            .segments
            .iter()
            .map(|s| Segment {
                a: s.start,
                b: s.end,
                extrude: s.extrusion_mm > 0.,
            })
            .collect(),
        layers: model
            .layers
            .iter()
            .map(|l| Layer {
                segments: l.first_segment..l.first_segment + l.segment_count,
                z: l.z_mm,
            })
            .collect(),
        ..Scene::default()
    };
    for plate in 0..model.plates.len() {
        let first = scene.triangles.len() as u32;
        for object in model
            .objects
            .iter()
            .filter(|object| object.plate as usize == plate)
        {
            for t in &model.triangles[object.first_triangle as usize
                ..(object.first_triangle + object.triangle_count) as usize]
            {
                let material = t.material.and_then(|i| model.materials[i as usize].color);
                let color = t
                    .colors
                    .iter()
                    .copied()
                    .flatten()
                    .next()
                    .or(material)
                    .or_else(|| {
                        t.vertices
                            .iter()
                            .find_map(|&i| model.vertices[i as usize].color)
                    });
                scene.triangles.push(t.vertices);
                scene.colors.push(color.map(|[r, g, b, _]| [r, g, b]));
            }
        }
        scene.plates.push(first..scene.triangles.len() as u32);
    }
    scene.check().map_err(str::to_owned)?;
    Ok(scene)
}

#[cfg(test)]
#[path = "preview_model_gap64_tests.rs"]
mod gap64_tests;

/// Reads bounded metadata from the same cache handoff without constructing a scene.
pub fn header(path: &std::path::Path) -> std::result::Result<String, String> {
    let (file, metadata) =
        crate::backend::regfile::open_if_regular_with_meta(path, crate::oflags::O_NOFOLLOW)
            .map_err(|error| error.to_string())?;
    if metadata.len() > reader::MAX_WIRE as u64 {
        return Err("3D wire exceeds its bound".into());
    }
    reader::header(file).map(|(summary, _)| summary)
}
