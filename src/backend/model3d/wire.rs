// Versioned binary handoff shared by the sandbox, GPUI and TUI; never cast untrusted bytes to structs.
use super::*;
use std::io::Write;

#[path = "../../ui_logic_model_preview/model_wire.rs"]
mod reader;
#[cfg(test)]
pub use reader::read;
pub use reader::{header, MAX_WIRE};
const MAGIC: &[u8] = b"FL3D\x01\0\0\0";
const MAX_HEADER: usize = 2 * 1024 * 1024;

pub fn summary(model: &Model) -> String {
    let facts = |f: Facts| {
        format!(
            r#"{{"dimensions_mm":[{},{},{}],"volume_cm3":{},"surface_cm2":{},"triangles":{},"objects":{},"plates":{},"open_edges":{}}}"#,
            f.dimensions_mm[0],
            f.dimensions_mm[1],
            f.dimensions_mm[2],
            optional(f.volume_cm3),
            optional(f.surface_cm2),
            f.triangles,
            f.objects,
            f.plates,
            f.open_edges
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".into())
        )
    };
    let plates = model
        .plates
        .iter()
        .enumerate()
        .map(|(i, p)| {
            format!(
                r#"{{"name":"{}","facts":{},"slicer":{},"metadata":{}}}"#,
                crate::json::escape(&p.name),
                facts(model.facts(Some(i))),
                model.slicer(Some(i)).json(),
                map_json(&p.metadata)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"format":"{}","facts":{},"slicer":{},"layers":{},"segments":{},"metadata":{},"plates":[{}]}}"#,
        crate::json::escape(&model.format),
        facts(model.facts(None)),
        model.slicer(None).json(),
        model.layers.len(),
        model.segments.len(),
        map_json(&model.metadata),
        plates
    )
}

fn optional(value: Option<f64>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".into())
}

fn map_json(map: &BTreeMap<String, String>) -> String {
    format!(
        "{{{}}}",
        map.iter()
            .map(|(k, v)| format!(
                r#""{}":"{}""#,
                crate::json::escape(k),
                crate::json::escape(v)
            ))
            .collect::<Vec<_>>()
            .join(",")
    )
}

struct Writer<W> {
    out: W,
    written: usize,
}

impl<W: Write> Writer<W> {
    fn bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.written = self
            .written
            .checked_add(bytes.len())
            .filter(|&n| n <= MAX_WIRE)
            .ok_or("3D model output exceeds its bound")?;
        self.out.write_all(bytes).map_err(|e| e.to_string())
    }
    fn u32(&mut self, n: u32) -> Result<()> {
        self.bytes(&n.to_le_bytes())
    }
    fn count(&mut self, n: usize) -> Result<()> {
        self.u32(u32::try_from(n).map_err(|_| "3D wire count overflow")?)
    }
    fn string(&mut self, s: &str) -> Result<()> {
        self.count(s.len())?;
        self.bytes(s.as_bytes())
    }
    fn point(&mut self, p: Point) -> Result<()> {
        for n in p {
            self.bytes(&n.to_le_bytes())?
        }
        Ok(())
    }
    fn color(&mut self, c: Option<Color>) -> Result<()> {
        self.bytes(&[u8::from(c.is_some())])?;
        self.bytes(&c.unwrap_or([0; 4]))
    }
    fn map(&mut self, map: &BTreeMap<String, String>) -> Result<()> {
        self.count(map.len())?;
        for (k, v) in map {
            self.string(k)?;
            self.string(v)?
        }
        Ok(())
    }
}

pub fn write(model: &Model, out: impl Write) -> Result<()> {
    let mut w = Writer { out, written: 0 };
    w.bytes(MAGIC)?;
    let summary = summary(model);
    if summary.len() > MAX_HEADER {
        return Err("3D facts exceed their wire bound".into());
    }
    w.string(&summary)?;
    w.count(model.thumbnails.len())?;
    for thumbnail in &model.thumbnails {
        w.u32(thumbnail.plate.unwrap_or(u32::MAX))?;
        w.string(&thumbnail.format)?;
        w.count(thumbnail.bytes.len())?;
        w.bytes(&thumbnail.bytes)?;
    }
    w.string(&model.format)?;
    w.map(&model.metadata)?;
    w.count(model.plates.len())?;
    for plate in &model.plates {
        w.string(&plate.name)?;
        w.map(&plate.metadata)?
    }
    w.count(model.materials.len())?;
    for material in &model.materials {
        w.string(&material.name)?;
        w.color(material.color)?
    }
    w.count(model.objects.len())?;
    for object in &model.objects {
        w.string(&object.name)?;
        w.u32(object.plate)?;
        w.u32(object.first_triangle)?;
        w.u32(object.triangle_count)?;
        for n in object.transform {
            w.bytes(&n.to_le_bytes())?
        }
    }
    w.count(model.vertices.len())?;
    for vertex in &model.vertices {
        w.point(vertex.position)?;
        w.color(vertex.color)?
    }
    w.count(model.triangles.len())?;
    for triangle in &model.triangles {
        for n in triangle.vertices {
            w.u32(n)?
        }
        for c in triangle.colors {
            w.color(c)?
        }
        w.u32(triangle.material.unwrap_or(u32::MAX))?;
    }
    w.count(model.segments.len())?;
    for segment in &model.segments {
        w.point(segment.start)?;
        w.point(segment.end)?;
        w.bytes(&segment.extrusion_mm.to_le_bytes())?;
        w.bytes(&segment.feed_mm_min.to_le_bytes())?;
        w.u32(u32::from(segment.tool))?;
    }
    w.count(model.layers.len())?;
    for layer in &model.layers {
        w.bytes(&layer.z_mm.to_le_bytes())?;
        w.u32(layer.first_segment)?;
        w.u32(layer.segment_count)?
    }
    w.out.flush().map_err(|e| e.to_string())
}
