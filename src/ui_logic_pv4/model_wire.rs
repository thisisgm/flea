// Bounded cache-only reader shared by the sandbox writer and the frontends.
use super::*;
use std::{collections::BTreeMap, io::Read};
const MAX_FORMAT_BYTES: usize = 16;
const MAX_THUMBNAIL_FORMAT_BYTES: usize = 8;
pub const MAX_WIRE: usize = 512 * 1024 * 1024;
const MAGIC: &[u8] = b"FL3D\x01\0\0\0";
const MAX_HEADER: usize = 2 * 1024 * 1024;

struct Reader<R> {
    input: R,
    read: usize,
}

impl<R: Read> Reader<R> {
    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.read = self
            .read
            .checked_add(N)
            .filter(|&n| n <= MAX_WIRE)
            .ok_or("3D wire exceeds its bound")?;
        let mut out = [0; N];
        self.input.read_exact(&mut out).map_err(|e| e.to_string())?;
        Ok(out)
    }
    fn bytes(&mut self, n: usize) -> Result<Vec<u8>> {
        self.read = self
            .read
            .checked_add(n)
            .filter(|&n| n <= MAX_WIRE)
            .ok_or("3D wire exceeds its bound")?;
        let mut out = Vec::new();
        self.input
            .by_ref()
            .take(n as u64)
            .read_to_end(&mut out)
            .map_err(|e| e.to_string())?;
        if out.len() != n {
            return Err("truncated 3D model".into());
        }
        Ok(out)
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    fn count(&mut self, max: usize) -> Result<usize> {
        let count = self.u32()? as usize;
        if count > max {
            return Err("3D wire count exceeds its bound".into());
        }
        Ok(count)
    }
    fn string(&mut self, max: usize) -> Result<String> {
        let n = self.count(max)?;
        String::from_utf8(self.bytes(n)?).map_err(|_| "invalid 3D wire string".into())
    }
    #[allow(dead_code)]
    fn float(&mut self) -> Result<f32> {
        let n = f32::from_le_bytes(self.array()?);
        if !n.is_finite() || n.abs() > MAX_COORDINATE as f32 {
            return Err("invalid 3D wire coordinate".into());
        }
        Ok(n)
    }
    #[allow(dead_code)]
    fn point(&mut self) -> Result<Point> {
        Ok([self.float()?, self.float()?, self.float()?])
    }
    #[allow(dead_code)]
    fn color(&mut self) -> Result<Option<Color>> {
        let tag = self.array::<1>()?[0];
        let color = self.array()?;
        match tag {
            0 => Ok(None),
            1 => Ok(Some(color)),
            _ => Err("invalid 3D wire colour".into()),
        }
    }
    #[allow(dead_code)]
    fn map(&mut self) -> Result<BTreeMap<String, String>> {
        let mut map = BTreeMap::new();
        for _ in 0..self.count(MAX_METADATA_ENTRIES)? {
            let key = self.string(MAX_METADATA_KEY_BYTES)?;
            let value = self.string(MAX_METADATA_VALUE_BYTES)?;
            map.insert(key, value);
        }
        Ok(map)
    }
    // Sample input (hex): 46 4C 33 44 01 00 00 00 02 00 00 00 7B 7D 00 00 00 00 has summary {} and zero thumbnails.
    fn header(&mut self) -> Result<(String, Vec<Thumbnail>)> {
        if self.array::<8>()? != MAGIC {
            return Err("invalid 3D model signature".into());
        }
        let summary = self.string(MAX_HEADER)?;
        crate::jsondoc::parse(&summary)?;
        let mut thumbnails = Vec::new();
        for _ in 0..self.count(MAX_THUMBNAILS)? {
            let plate = self.u32()?;
            let format = self.string(MAX_THUMBNAIL_FORMAT_BYTES)?;
            let n = self.count(MAX_THUMBNAIL)?;
            thumbnails.push(Thumbnail {
                plate: (plate != u32::MAX).then_some(plate),
                format,
                bytes: self.bytes(n)?,
            });
        }
        Ok((summary, thumbnails))
    }
}

// Sample input (hex): 46 4C 33 44 01 00 00 00 02 00 00 00 7B 7D 00 00 00 00 has summary {} and zero thumbnails.
pub fn header(input: impl Read) -> Result<(String, Vec<Thumbnail>)> {
    Reader { input, read: 0 }.header()
}

// Sample input: the header example above followed by nine u32le zeros encodes an empty format, metadata map and seven empty model vectors.
#[cfg_attr(not(test), allow(dead_code))]
pub fn read(input: impl Read) -> Result<Model> {
    let mut r = Reader { input, read: 0 };
    let (_, thumbnails) = r.header()?;
    let mut model = Model {
        format: r.string(MAX_FORMAT_BYTES)?,
        metadata: r.map()?,
        thumbnails,
        ..Model::default()
    };
    for _ in 0..r.count(MAX_PLATES)? {
        model.plates.push(Plate {
            name: r.string(MAX_NAME_BYTES)?,
            metadata: r.map()?,
        })
    }
    for _ in 0..r.count(MAX_OBJECTS)? {
        model.materials.push(Material {
            name: r.string(MAX_NAME_BYTES)?,
            color: r.color()?,
        })
    }
    for _ in 0..r.count(MAX_OBJECTS)? {
        let name = r.string(MAX_NAME_BYTES)?;
        let (plate, first_triangle, triangle_count) = (r.u32()?, r.u32()?, r.u32()?);
        let mut transform = [0.; 12];
        for n in &mut transform {
            *n = f64::from_le_bytes(r.array()?);
            if !n.is_finite() {
                return Err("invalid 3D wire transform".into());
            }
        }
        if plate as usize >= model.plates.len() {
            return Err("3D object plate is out of range".into());
        }
        model.objects.push(Object {
            name,
            plate,
            first_triangle,
            triangle_count,
            transform,
        });
    }
    for _ in 0..r.count(MAX_VERTICES)? {
        let p = r.point()?;
        let c = r.color()?;
        model.vertex(p, c)?;
    }
    for _ in 0..r.count(MAX_TRIANGLES)? {
        let face = [r.u32()?, r.u32()?, r.u32()?];
        let colors = [r.color()?, r.color()?, r.color()?];
        let material = r.u32()?;
        if material != u32::MAX && material as usize >= model.materials.len() {
            return Err("3D material is out of range".into());
        }
        model.triangle(face, colors, (material != u32::MAX).then_some(material))?;
    }
    let mut next = 0usize;
    for object in &model.objects {
        if object.first_triangle as usize != next {
            return Err("invalid 3D object ranges".into());
        }
        next = next
            .checked_add(object.triangle_count as usize)
            .filter(|&n| n <= model.triangles.len())
            .ok_or("3D object range exceeds mesh")?;
    }
    if next != model.triangles.len() {
        return Err("3D objects do not cover mesh".into());
    }
    for _ in 0..r.count(MAX_SEGMENTS)? {
        let (start, end, extrusion_mm, feed_mm_min) =
            (r.point()?, r.point()?, r.float()?, r.float()?);
        let tool = u16::try_from(r.u32()?).map_err(|_| "invalid 3D tool")?;
        model.segments.push(Segment {
            start,
            end,
            extrusion_mm,
            feed_mm_min,
            tool,
        });
    }
    let mut previous = 0usize;
    for _ in 0..r.count(MAX_LAYERS)? {
        let (z_mm, first_segment, segment_count) = (r.float()?, r.u32()?, r.u32()?);
        if first_segment as usize != previous {
            return Err("invalid 3D layer range".into());
        }
        previous = previous
            .checked_add(segment_count as usize)
            .filter(|&n| n <= model.segments.len())
            .ok_or("3D layer exceeds toolpath")?;
        model.layers.push(Layer {
            z_mm,
            first_segment,
            segment_count,
        });
    }
    if previous != model.segments.len() {
        return Err("3D layers do not cover toolpath".into());
    }
    let mut tail = [0];
    if r.input.read(&mut tail).map_err(|e| e.to_string())? != 0 {
        return Err("trailing 3D wire bytes".into());
    }
    Ok(model)
}
