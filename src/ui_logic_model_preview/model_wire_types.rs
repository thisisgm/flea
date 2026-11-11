// Shared cache data types; original-file parsers stay in the backend sandbox.
use super::MAX_COORDINATE;
use std::collections::BTreeMap;

pub const MAX_VERTICES: usize = 12_000_000;
pub const MAX_TRIANGLES: usize = 4_000_000;
pub const MAX_SEGMENTS: usize = 4_000_000;
pub const MAX_OBJECTS: usize = 4096;
pub const MAX_THUMBNAIL: usize = 8 * 1024 * 1024;
pub type Result<T> = std::result::Result<T, String>;
pub type Color = [u8; 4];
pub type Point = [f32; 3];
pub type Transform = [f64; 12];

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

impl Model {
    pub(super) fn vertex(&mut self, position: Point, color: Option<Color>) -> Result<u32> {
        if self.vertices.len() >= MAX_VERTICES
            || position
                .iter()
                .any(|n| !n.is_finite() || n.abs() > MAX_COORDINATE as f32)
        {
            return Err("3D vertex exceeds its bound".into());
        }
        let index = self.vertices.len() as u32;
        self.vertices.push(Vertex { position, color });
        Ok(index)
    }

    pub(super) fn triangle(
        &mut self,
        vertices: [u32; 3],
        colors: [Option<Color>; 3],
        material: Option<u32>,
    ) -> Result<()> {
        if self.triangles.len() >= MAX_TRIANGLES
            || vertices.iter().any(|&i| i as usize >= self.vertices.len())
        {
            return Err("invalid or excessive 3D triangles".into());
        }
        self.triangles.push(Triangle {
            vertices,
            colors,
            material,
        });
        Ok(())
    }
}
