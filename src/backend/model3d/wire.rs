// Versioned binary handoff shared by the sandbox, GPUI and TUI; never cast untrusted bytes to structs.
use super::*;
use std::io::{Read,Write};

const MAX_FORMAT_BYTES: usize = 16;
const MAX_THUMBNAIL_FORMAT_BYTES: usize = 8;

pub const MAX_WIRE: usize = 512*1024*1024;
const MAGIC: &[u8] = b"FL3D\x01\0\0\0";
const MAX_HEADER: usize = 2*1024*1024;

pub fn summary(model: &Model) -> String {
    let facts = |f: Facts| format!(r#"{{"dimensions_mm":[{},{},{}],"volume_cm3":{},"surface_cm2":{},"triangles":{},"objects":{},"plates":{},"open_edges":{}}}"#,
        f.dimensions_mm[0],f.dimensions_mm[1],f.dimensions_mm[2],optional(f.volume_cm3),optional(f.surface_cm2),f.triangles,f.objects,f.plates,
        f.open_edges.map(|v| v.to_string()).unwrap_or_else(|| "null".into()));
    let plates = model.plates.iter().enumerate().map(|(i,p)| format!(r#"{{"name":"{}","facts":{},"slicer":{},"metadata":{}}}"#,
        crate::json::escape(&p.name),facts(model.facts(Some(i))),model.slicer(Some(i)).json(),map_json(&p.metadata))).collect::<Vec<_>>().join(",");
    format!(r#"{{"format":"{}","facts":{},"slicer":{},"layers":{},"segments":{},"metadata":{},"plates":[{}]}}"#,
        crate::json::escape(&model.format),facts(model.facts(None)),model.slicer(None).json(),model.layers.len(),model.segments.len(),map_json(&model.metadata),plates)
}

fn optional(value: Option<f64>) -> String { value.map(|v| v.to_string()).unwrap_or_else(|| "null".into()) }

fn map_json(map: &BTreeMap<String,String>) -> String {
    format!("{{{}}}",map.iter().map(|(k,v)| format!(r#""{}":"{}""#,crate::json::escape(k),crate::json::escape(v))).collect::<Vec<_>>().join(","))
}

struct Writer<W> { out: W, written: usize }

impl<W: Write> Writer<W> {
    fn bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.written = self.written.checked_add(bytes.len()).filter(|&n| n <= MAX_WIRE).ok_or("3D model output exceeds its bound")?;
        self.out.write_all(bytes).map_err(|e| e.to_string())
    }
    fn u32(&mut self, n: u32) -> Result<()> { self.bytes(&n.to_le_bytes()) }
    fn count(&mut self, n: usize) -> Result<()> { self.u32(u32::try_from(n).map_err(|_| "3D wire count overflow")?) }
    fn string(&mut self, s: &str) -> Result<()> { self.count(s.len())?; self.bytes(s.as_bytes()) }
    fn point(&mut self, p: Point) -> Result<()> { for n in p { self.bytes(&n.to_le_bytes())? } Ok(()) }
    fn color(&mut self, c: Option<Color>) -> Result<()> { self.bytes(&[u8::from(c.is_some())])?; self.bytes(&c.unwrap_or([0;4])) }
    fn map(&mut self, map: &BTreeMap<String,String>) -> Result<()> {
        self.count(map.len())?;
        for (k,v) in map { self.string(k)?; self.string(v)? }
        Ok(())
    }
}

pub fn write(model: &Model, out: impl Write) -> Result<()> {
    let mut w = Writer { out, written: 0 };
    w.bytes(MAGIC)?;
    let summary = summary(model);
    if summary.len() > MAX_HEADER { return Err("3D facts exceed their wire bound".into()) }
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
    for plate in &model.plates { w.string(&plate.name)?; w.map(&plate.metadata)? }
    w.count(model.materials.len())?;
    for material in &model.materials { w.string(&material.name)?; w.color(material.color)? }
    w.count(model.objects.len())?;
    for object in &model.objects {
        w.string(&object.name)?; w.u32(object.plate)?; w.u32(object.first_triangle)?; w.u32(object.triangle_count)?;
        for n in object.transform { w.bytes(&n.to_le_bytes())? }
    }
    w.count(model.vertices.len())?;
    for vertex in &model.vertices { w.point(vertex.position)?; w.color(vertex.color)? }
    w.count(model.triangles.len())?;
    for triangle in &model.triangles {
        for n in triangle.vertices { w.u32(n)? }
        for c in triangle.colors { w.color(c)? }
        w.u32(triangle.material.unwrap_or(u32::MAX))?;
    }
    w.count(model.segments.len())?;
    for segment in &model.segments {
        w.point(segment.start)?; w.point(segment.end)?;
        w.bytes(&segment.extrusion_mm.to_le_bytes())?; w.bytes(&segment.feed_mm_min.to_le_bytes())?;
        w.u32(u32::from(segment.tool))?;
    }
    w.count(model.layers.len())?;
    for layer in &model.layers { w.bytes(&layer.z_mm.to_le_bytes())?; w.u32(layer.first_segment)?; w.u32(layer.segment_count)? }
    w.out.flush().map_err(|e| e.to_string())
}

struct Reader<R> { input: R, read: usize }

impl<R: Read> Reader<R> {
    fn array<const N: usize>(&mut self) -> Result<[u8;N]> {
        self.read = self.read.checked_add(N).filter(|&n| n <= MAX_WIRE).ok_or("3D wire exceeds its bound")?;
        let mut out = [0;N];
        self.input.read_exact(&mut out).map_err(|e| e.to_string())?;
        Ok(out)
    }
    fn bytes(&mut self, n: usize) -> Result<Vec<u8>> {
        self.read = self.read.checked_add(n).filter(|&n| n <= MAX_WIRE).ok_or("3D wire exceeds its bound")?;
        let mut out = Vec::new();
        self.input.by_ref().take(n as u64).read_to_end(&mut out).map_err(|e| e.to_string())?;
        if out.len() != n { return Err("truncated 3D model".into()) }
        Ok(out)
    }
    fn u32(&mut self) -> Result<u32> { Ok(u32::from_le_bytes(self.array()?)) }
    fn count(&mut self, max: usize) -> Result<usize> {
        let count = self.u32()? as usize;
        if count > max { return Err("3D wire count exceeds its bound".into()) }
        Ok(count)
    }
    fn string(&mut self, max: usize) -> Result<String> { let n = self.count(max)?; String::from_utf8(self.bytes(n)?).map_err(|_| "invalid 3D wire string".into()) }
    #[allow(dead_code)] // Consumed by the GPUI and TUI model reader once rendering lands.
    fn float(&mut self) -> Result<f32> {
        let n = f32::from_le_bytes(self.array()?);
        if !n.is_finite() || n.abs() > MAX_COORDINATE as f32 { return Err("invalid 3D wire coordinate".into()) }
        Ok(n)
    }
    #[allow(dead_code)]
    fn point(&mut self) -> Result<Point> { Ok([self.float()?,self.float()?,self.float()?]) }
    #[allow(dead_code)]
    fn color(&mut self) -> Result<Option<Color>> {
        let tag = self.array::<1>()?[0];
        let color = self.array()?;
        match tag { 0 => Ok(None), 1 => Ok(Some(color)), _ => Err("invalid 3D wire colour".into()) }
    }
    #[allow(dead_code)]
    fn map(&mut self) -> Result<BTreeMap<String,String>> {
        let mut map = BTreeMap::new();
        for _ in 0..self.count(MAX_METADATA_ENTRIES)? { let key = self.string(MAX_METADATA_KEY_BYTES)?; let value = self.string(MAX_METADATA_VALUE_BYTES)?; map.insert(key,value); }
        Ok(map)
    }
    // Sample input (hex): 46 4C 33 44 01 00 00 00 02 00 00 00 7B 7D 00 00 00 00 has summary {} and zero thumbnails.
    fn header(&mut self) -> Result<(String,Vec<Thumbnail>)> {
        if self.array::<8>()? != MAGIC { return Err("invalid 3D model signature".into()) }
        let summary = self.string(MAX_HEADER)?;
        crate::jsondoc::parse(&summary)?;
        let mut thumbnails = Vec::new();
        for _ in 0..self.count(MAX_THUMBNAILS)? {
            let plate = self.u32()?;
            let format = self.string(MAX_THUMBNAIL_FORMAT_BYTES)?;
            let n = self.count(MAX_THUMBNAIL)?;
            thumbnails.push(Thumbnail { plate: (plate != u32::MAX).then_some(plate), format, bytes: self.bytes(n)? });
        }
        Ok((summary,thumbnails))
    }
}

// Sample input (hex): 46 4C 33 44 01 00 00 00 02 00 00 00 7B 7D 00 00 00 00 has summary {} and zero thumbnails.
pub fn header(input: impl Read) -> Result<(String,Vec<Thumbnail>)> { Reader { input, read: 0 }.header() }

// Sample input: the header example above followed by nine u32le zeros encodes an empty format, metadata map and seven empty model vectors.
#[cfg_attr(not(test), allow(dead_code))]
pub fn read(input: impl Read) -> Result<Model> {
    let mut r = Reader { input, read: 0 };
    let (_,thumbnails) = r.header()?;
    let mut model = Model { format: r.string(MAX_FORMAT_BYTES)?, metadata: r.map()?, thumbnails, ..Model::default() };
    for _ in 0..r.count(MAX_PLATES)? { model.plates.push(Plate { name: r.string(MAX_NAME_BYTES)?, metadata: r.map()? }) }
    for _ in 0..r.count(MAX_OBJECTS)? { model.materials.push(Material { name: r.string(MAX_NAME_BYTES)?, color: r.color()? }) }
    for _ in 0..r.count(MAX_OBJECTS)? {
        let name = r.string(MAX_NAME_BYTES)?;
        let (plate,first_triangle,triangle_count) = (r.u32()?,r.u32()?,r.u32()?);
        let mut transform = [0.;12];
        for n in &mut transform { *n = f64::from_le_bytes(r.array()?); if !n.is_finite() { return Err("invalid 3D wire transform".into()) } }
        if plate as usize >= model.plates.len() { return Err("3D object plate is out of range".into()) }
        model.objects.push(Object { name, plate, first_triangle, triangle_count, transform });
    }
    for _ in 0..r.count(MAX_VERTICES)? { let p = r.point()?; let c = r.color()?; model.vertex(p,c)?; }
    for _ in 0..r.count(MAX_TRIANGLES)? {
        let face = [r.u32()?,r.u32()?,r.u32()?];
        let colors = [r.color()?,r.color()?,r.color()?];
        let material = r.u32()?;
        if material != u32::MAX && material as usize >= model.materials.len() { return Err("3D material is out of range".into()) }
        model.triangle(face,colors,(material != u32::MAX).then_some(material))?;
    }
    let mut next = 0usize;
    for object in &model.objects {
        if object.first_triangle as usize != next { return Err("invalid 3D object ranges".into()) }
        next = next.checked_add(object.triangle_count as usize).filter(|&n| n <= model.triangles.len()).ok_or("3D object range exceeds mesh")?;
    }
    if next != model.triangles.len() { return Err("3D objects do not cover mesh".into()) }
    for _ in 0..r.count(MAX_SEGMENTS)? {
        let (start,end,extrusion_mm,feed_mm_min) = (r.point()?,r.point()?,r.float()?,r.float()?);
        let tool = u16::try_from(r.u32()?).map_err(|_| "invalid 3D tool")?;
        model.segments.push(Segment { start, end, extrusion_mm, feed_mm_min, tool });
    }
    let mut previous = 0usize;
    for _ in 0..r.count(MAX_LAYERS)? {
        let (z_mm,first_segment,segment_count) = (r.float()?,r.u32()?,r.u32()?);
        if first_segment as usize != previous { return Err("invalid 3D layer range".into()) }
        previous = previous.checked_add(segment_count as usize).filter(|&n| n <= model.segments.len()).ok_or("3D layer exceeds toolpath")?;
        model.layers.push(Layer { z_mm, first_segment, segment_count });
    }
    if previous != model.segments.len() { return Err("3D layers do not cover toolpath".into()) }
    let mut tail = [0];
    if r.input.read(&mut tail).map_err(|e| e.to_string())? != 0 { return Err("trailing 3D wire bytes".into()) }
    Ok(model)
}
