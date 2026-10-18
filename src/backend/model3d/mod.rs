// Shared, renderer-independent 3D data; production parses only in model3dreq's thumbnail jail.
mod amf;
mod bgcode;
pub(super) mod codecs;
mod cura;
mod facts;
mod gcode;
mod mesh;
mod ply;
mod project;
mod projectmeta;
mod properties;
mod slicer;
pub use slicer::SlicerFacts;
mod xml;
mod zip;
pub mod wire;

use std::collections::BTreeMap;

pub const MAX_INPUT: usize = 128 * 1024 * 1024;
pub const MAX_EXPANDED: usize = 256 * 1024 * 1024;
pub const MAX_VERTICES: usize = 12_000_000;
pub const MAX_TRIANGLES: usize = 4_000_000;
pub const MAX_SEGMENTS: usize = 4_000_000;
pub const MAX_OBJECTS: usize = 4096;
pub const MAX_THUMBNAIL: usize = 8 * 1024 * 1024;
pub type Result<T> = std::result::Result<T, String>;
pub type Color = [u8; 4];
pub type Point = [f32; 3];
pub type Transform = [f64; 12];
pub const IDENTITY: Transform = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];

#[derive(Clone, Debug, PartialEq)]
pub struct Vertex {
    pub position: Point,
    pub color: Option<Color>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Triangle {
    pub vertices: [u32; 3],
    pub colors: [Option<Color>; 3],
    pub material: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    pub color: Option<Color>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    pub name: String,
    pub plate: u32,
    pub first_triangle: u32,
    pub triangle_count: u32,
    pub transform: Transform,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plate {
    pub name: String,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub start: Point,
    pub end: Point,
    pub extrusion_mm: f32,
    pub feed_mm_min: f32,
    pub tool: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub z_mm: f32,
    pub first_segment: u32,
    pub segment_count: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Thumbnail {
    pub plate: Option<u32>,
    pub format: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub format: String,
    pub vertices: Vec<Vertex>,
    pub triangles: Vec<Triangle>,
    pub materials: Vec<Material>,
    pub objects: Vec<Object>,
    pub plates: Vec<Plate>,
    pub segments: Vec<Segment>,
    pub layers: Vec<Layer>,
    pub thumbnails: Vec<Thumbnail>,
    pub metadata: BTreeMap<String, String>,
}

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
    match name.rsplit_once('.')?.1.to_ascii_lowercase().as_str() {
        "stl" => Some("stl"), "3mf" => Some("3mf"), "obj" => Some("obj"),
        "ply" => Some("ply"), "amf" => Some("amf"),
        "gcode" | "gco" | "g" => Some("gcode"), "bgcode" => Some("bgcode"), _ => None,
    }
}

pub fn parse(format: &str, bytes: &[u8]) -> Result<Model> {
    if bytes.is_empty() || bytes.len() > MAX_INPUT { return Err("3D input exceeds its bound".into()) }
    let mut model = Model { format: format.into(), ..Model::default() };
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
    if model.triangles.is_empty() && model.segments.is_empty() { return Err("3D file contains no geometry".into()) }
    if model.plates.is_empty() { model.plates.push(Plate { name: "Plate 1".into(), metadata: BTreeMap::new() }) }
    if model.objects.is_empty() && !model.triangles.is_empty() { model.object("Object 1", 0, 0, IDENTITY)? }
    if model.plates.len() > 128 || model.materials.iter().any(|m| m.name.len() > 1024)
        || model.plates.iter().any(|p| p.name.len() > 1024 || p.metadata.len() > 1024 || p.metadata.iter().any(|(k,v)| k.len() > 256 || v.len() > 16_384)) {
        return Err("3D names or plate metadata exceed their bound".into());
    }
    Ok(model)
}

impl Model {
    pub fn facts(&self, plate: Option<usize>) -> Facts { facts::measure(self, plate) }
    pub fn slicer(&self, plate: Option<usize>) -> SlicerFacts { slicer::facts(self,plate) }

    fn vertex(&mut self, position: Point, color: Option<Color>) -> Result<u32> {
        if self.vertices.len() >= MAX_VERTICES || position.iter().any(|n| !n.is_finite() || n.abs() > 1e9) {
            return Err("3D vertex exceeds its bound".into());
        }
        let index = self.vertices.len() as u32;
        self.vertices.push(Vertex { position, color });
        Ok(index)
    }

    fn triangle(&mut self, vertices: [u32; 3], colors: [Option<Color>; 3], material: Option<u32>) -> Result<()> {
        if self.triangles.len() >= MAX_TRIANGLES || vertices.iter().any(|&i| i as usize >= self.vertices.len()) {
            return Err("invalid or excessive 3D triangles".into());
        }
        self.triangles.push(Triangle { vertices, colors, material });
        Ok(())
    }

    fn object(&mut self, name: &str, plate: u32, first: usize, transform: Transform) -> Result<()> {
        if self.objects.len() >= MAX_OBJECTS { return Err("too many 3D objects".into()) }
        if name.len() > 1024 { return Err("3D object name exceeds its bound".into()) }
        self.objects.push(Object { name: name.into(), plate, first_triangle: first as u32,
            triangle_count: (self.triangles.len() - first) as u32, transform });
        Ok(())
    }

    fn metadata(&mut self, key: &str, value: &str) {
        if (self.metadata.len() < 1024 || self.metadata.contains_key(key.trim())) && key.len() <= 256 && value.len() <= 16_384 {
            self.metadata.insert(key.trim().into(), value.trim().into());
        }
    }

    fn thumbnail(&mut self, plate: Option<u32>, format: &str, bytes: Vec<u8>) -> Result<()> {
        if self.thumbnails.len() >= 128 || bytes.len() > MAX_THUMBNAIL { return Err("3D thumbnail exceeds its bound".into()) }
        self.thumbnails.push(Thumbnail { plate, format: format.into(), bytes });
        Ok(())
    }
}

fn number(value: &str) -> Result<f64> {
    value.parse::<f64>().ok().filter(|v| v.is_finite() && v.abs() <= 1e9).ok_or_else(|| "invalid 3D number".into())
}

fn text(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).ok().filter(|s| !s.contains('\0')).ok_or_else(|| "invalid 3D text".into())
}

fn color(value: &str) -> Result<Color> {
    let value = value.strip_prefix('#').unwrap_or(value);
    if !matches!(value.len(), 6 | 8) || !value.is_ascii() { return Err("invalid 3D colour".into()) }
    let mut out = [255; 4];
    for (i, part) in value.as_bytes().chunks_exact(2).enumerate() {
        out[i] = u8::from_str_radix(std::str::from_utf8(part).unwrap(), 16).map_err(|_| "invalid 3D colour")?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(super) mod testdata;

#[cfg(test)]
mod review_probes;
#[cfg(test)]
mod r1_tests;

#[cfg(test)]
mod r2_review_probes;
#[cfg(test)]
mod r2_tests;

#[cfg(test)]
mod r3_review_probes;
#[cfg(test)]
mod r3_tests;
