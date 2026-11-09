// Shared, renderer-independent 3D data; production parses only in model3dreq's thumbnail jail.
mod amf;
mod limits;
use limits::*;
mod bgcode;
pub(crate) mod codecs;
mod cura;
mod facts;
mod gcode;
mod mesh;
mod ply;
mod project;
mod projectmeta;
mod projectplates;
mod properties;
mod slicer;
pub use slicer::SlicerFacts;
pub mod wire;
mod xml;
mod zip;

use std::collections::BTreeMap;

#[path = "../../ui_logic_pv4/model_wire_types.rs"]
mod cache_types;
pub use cache_types::*;
pub const MAX_INPUT: usize = 128 * 1024 * 1024;
pub const MAX_EXPANDED: usize = 256 * 1024 * 1024;
pub const IDENTITY: Transform = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    pub dimensions_mm: [f64; 3],
    pub volume_cm3: Option<f64>,
    pub surface_cm2: Option<f64>,
    pub triangles: usize,
    pub objects: usize,
    pub plates: usize,
    pub open_edges: Option<usize>,
}

pub fn supported(name: &str) -> Option<&'static str> {
    match flea::bytename::display(name)
        .rsplit_once('.')?
        .1
        .to_ascii_lowercase()
        .as_str()
    {
        "stl" => Some("stl"),
        "3mf" => Some("3mf"),
        "obj" => Some("obj"),
        "ply" => Some("ply"),
        "amf" => Some("amf"),
        "gcode" | "gco" | "g" => Some("gcode"),
        "bgcode" => Some("bgcode"),
        _ => None,
    }
}

pub(super) fn embedded_thumbnail_supported(name: &str) -> bool {
    matches!(supported(name), Some("3mf" | "gcode" | "bgcode"))
}

// Sample input: format="obj", bytes=b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n".
pub fn parse(format: &str, bytes: &[u8]) -> Result<Model> {
    if bytes.is_empty() || bytes.len() > MAX_INPUT {
        return Err("3D input exceeds its bound".into());
    }
    let mut model = Model {
        format: format.into(),
        ..Model::default()
    };
    match format {
        "stl" => mesh::stl(bytes, &mut model)?,
        "obj" => mesh::obj(bytes, &mut model)?,
        "ply" => ply::parse(bytes, &mut model)?,
        "amf" => amf::parse(bytes, &mut model)?,
        "3mf" => project::parse(bytes, &mut model)?,
        "gcode" => gcode::parse(bytes, &mut model)?,
        "bgcode" => bgcode::parse(bytes, &mut model)?,
        _ => return Err("unsupported 3D format".into()),
    }
    if model.triangles.is_empty() && model.segments.is_empty() {
        return Err("3D file contains no geometry".into());
    }
    if model.plates.is_empty() {
        model.plates.push(Plate {
            name: "Plate 1".into(),
            metadata: BTreeMap::new(),
        })
    }
    if model.objects.is_empty() && !model.triangles.is_empty() {
        model.object("Object 1", 0, 0, IDENTITY)?
    }
    if model.plates.len() > MAX_PLATES
        || model
            .materials
            .iter()
            .any(|m| m.name.len() > MAX_NAME_BYTES)
        || model.plates.iter().any(|p| {
            p.name.len() > MAX_NAME_BYTES
                || p.metadata.len() > MAX_METADATA_ENTRIES
                || p.metadata.iter().any(|(k, v)| {
                    k.len() > MAX_METADATA_KEY_BYTES || v.len() > MAX_METADATA_VALUE_BYTES
                })
        })
    {
        return Err("3D names or plate metadata exceed their bound".into());
    }
    Ok(model)
}

impl Model {
    pub fn facts(&self, plate: Option<usize>) -> Facts {
        facts::measure(self, plate)
    }
    pub fn slicer(&self, plate: Option<usize>) -> SlicerFacts {
        slicer::facts(self, plate)
    }

    fn object(&mut self, name: &str, plate: u32, first: usize, transform: Transform) -> Result<()> {
        if self.objects.len() >= MAX_OBJECTS {
            return Err("too many 3D objects".into());
        }
        if name.len() > MAX_NAME_BYTES {
            return Err("3D object name exceeds its bound".into());
        }
        self.objects.push(Object {
            name: name.into(),
            plate,
            first_triangle: first as u32,
            triangle_count: (self.triangles.len() - first) as u32,
            transform,
        });
        Ok(())
    }

    fn metadata(&mut self, key: &str, value: &str) {
        if (self.metadata.len() < MAX_METADATA_ENTRIES || self.metadata.contains_key(key.trim()))
            && key.len() <= MAX_METADATA_KEY_BYTES
            && value.len() <= MAX_METADATA_VALUE_BYTES
        {
            self.metadata.insert(key.trim().into(), value.trim().into());
        }
    }

    fn thumbnail(&mut self, plate: Option<u32>, format: &str, bytes: Vec<u8>) -> Result<()> {
        if self.thumbnails.len() >= MAX_THUMBNAILS || bytes.len() > MAX_THUMBNAIL {
            return Err("3D thumbnail exceeds its bound".into());
        }
        self.thumbnails.push(Thumbnail {
            plate,
            format: format.into(),
            bytes,
        });
        Ok(())
    }
}

fn number(value: &str) -> Result<f64> {
    value
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && v.abs() <= MAX_COORDINATE)
        .ok_or_else(|| "invalid 3D number".into())
}

fn text(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes)
        .ok()
        .filter(|s| !s.contains('\0'))
        .ok_or_else(|| "invalid 3D text".into())
}

fn color(value: &str) -> Result<Color> {
    let value = value.strip_prefix('#').unwrap_or(value);
    if !matches!(value.len(), 6 | 8) || !value.is_ascii() {
        return Err("invalid 3D colour".into());
    }
    let mut out = [255; 4];
    for (i, part) in value.as_bytes().chunks_exact(2).enumerate() {
        out[i] = u8::from_str_radix(std::str::from_utf8(part).unwrap(), 16)
            .map_err(|_| "invalid 3D colour")?;
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod testdata;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod r1_tests;
#[cfg(test)]
mod review_probes;

#[cfg(test)]
mod r2_review_probes;
#[cfg(test)]
mod r2_tests;

#[cfg(test)]
mod r3_review_probes;
#[cfg(test)]
mod r3_tests;

#[cfg(test)]
mod regression_tests;
