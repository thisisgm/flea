use super::limits::*;
use super::{Model, Result};

const MAX_HEADER_BYTES: usize = 65_536;
const MAX_ELEMENTS: usize = 32;
const MAX_PROPERTIES: usize = 64;
const MAX_LIST_VALUES: usize = 1024;

#[derive(Clone,Copy)]
enum Type { I8, U8, I16, U16, I32, U32, F32, F64 }

impl Type {
    // Sample input: "float" or "uint8" selects one PLY scalar representation.
    fn parse(name: &str) -> Result<Self> {
        match name {
            "char"|"int8" => Ok(Self::I8), "uchar"|"uint8" => Ok(Self::U8),
            "short"|"int16" => Ok(Self::I16), "ushort"|"uint16" => Ok(Self::U16),
            "int"|"int32" => Ok(Self::I32), "uint"|"uint32" => Ok(Self::U32),
            "float"|"float32" => Ok(Self::F32), "double"|"float64" => Ok(Self::F64),
            _ => Err("unsupported PLY scalar".into()),
        }
    }

    fn integer(self) -> bool { !matches!(self,Self::F32|Self::F64) }

    fn read(self, data: &mut Data) -> Result<f64> {
        let value = if let Some(words) = data.words.as_mut() {
            let word = words.next().ok_or("truncated PLY data")?;
            let n = super::number(word)?;
            let (lo,hi) = match self {
                Self::I8 => (-128.,127.), Self::U8 => (0.,255.), Self::I16 => (-32768.,32767.),
                Self::U16 => (0.,65535.), Self::I32 => (i32::MIN as f64,i32::MAX as f64),
                Self::U32 => (0.,u32::MAX as f64), _ => (-MAX_COORDINATE,MAX_COORDINATE),
            };
            if n < lo || n > hi || (self.integer() && n.fract() != 0.) { return Err("PLY scalar is out of range".into()) }
            n
        } else {
            let n = match self { Self::I8|Self::U8 => 1, Self::I16|Self::U16 => 2, Self::I32|Self::U32|Self::F32 => 4, Self::F64 => 8 };
            let b = data.bytes.get(data.at..data.at+n).ok_or("truncated binary PLY")?;
            data.at += n;
            let mut raw = [0;8];
            if data.big { raw[8-n..].copy_from_slice(b) } else { raw[..n].copy_from_slice(b) }
            let raw = if data.big { u64::from_be_bytes(raw) } else { u64::from_le_bytes(raw) };
            match self {
                Self::I8 => raw as i8 as f64, Self::U8 => raw as u8 as f64,
                Self::I16 => raw as i16 as f64, Self::U16 => raw as u16 as f64,
                Self::I32 => raw as i32 as f64, Self::U32 => raw as u32 as f64,
                Self::F32 => f32::from_bits(raw as u32) as f64, Self::F64 => f64::from_bits(raw),
            }
        };
        if !value.is_finite() { return Err("non-finite PLY scalar".into()) }
        Ok(value)
    }
}

struct Property { name: String, scalar: Type, count: Option<Type> }
struct Element { name: String, count: usize, properties: Vec<Property> }
struct Data<'a> { bytes: &'a [u8], at: usize, big: bool, words: Option<std::str::SplitWhitespace<'a>> }

// Sample input: ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\nproperty float y\nproperty float z\nend_header\n0 0 0\n.
pub(super) fn parse(bytes: &[u8], model: &mut Model) -> Result<()> {
    let mut end = 0;
    let mut elements: Vec<Element> = Vec::new();
    let mut format = None;
    let mut total = 0usize;
    for line in bytes.split_inclusive(|b| *b == b'\n') {
        end += line.len();
        if end > MAX_HEADER_BYTES { return Err("PLY header exceeds its bound".into()) }
        let line = super::text(line)?.trim();
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["ply"] if end == 4 || end == 5 => {},
            ["format",name,"1.0"] if format.is_none() => format = Some(*name),
            ["element",name,count] => {
                let count = count.parse::<usize>().map_err(|_| "invalid PLY element count")?;
                total = total.checked_add(count).filter(|&n| n <= super::MAX_VERTICES+super::MAX_TRIANGLES).ok_or("PLY element count exceeds its bound")?;
                if elements.len() >= MAX_ELEMENTS { return Err("too many PLY elements".into()) }
                elements.push(Element { name: name.to_string(), count, properties: Vec::new() });
            }
            ["property",scalar,name] | ["property","list",_,scalar,name] => {
                let element = elements.last_mut().ok_or("PLY property has no element")?;
                if element.properties.len() >= MAX_PROPERTIES || element.properties.iter().any(|p| p.name == *name) { return Err("invalid PLY properties".into()) }
                let count = if words.len() == 5 { Some(Type::parse(words[2])?) } else { None };
                if count.is_some_and(|t| !t.integer()) { return Err("PLY list count must be integer".into()) }
                element.properties.push(Property { name: name.to_string(), scalar: Type::parse(scalar)?, count });
            }
            ["end_header"] => break,
            ["comment",..] | ["obj_info",..] => {},
            _ => return Err("invalid PLY header".into()),
        }
    }
    if !bytes[..end].ends_with(b"end_header\n") && !bytes[..end].ends_with(b"end_header\r\n") { return Err("truncated PLY header".into()) }
    let format = format.ok_or("PLY format is missing")?;
    if !matches!(format,"ascii"|"binary_little_endian"|"binary_big_endian") { return Err("unsupported PLY format".into()) }
    let payload = &bytes[end..];
    let mut data = Data { bytes: payload, at: 0, big: format == "binary_big_endian", words: if format == "ascii" { Some(super::text(payload)?.split_whitespace()) } else { None } };
    let mut faces = Vec::new();
    let mut face_indices = 0usize;
    for element in elements {
        for _ in 0..element.count {
            let mut xyz = [None;3];
            let mut rgba = [None;4];
            let mut indices = None;
            for property in &element.properties {
                if let Some(count) = property.count {
                    let n = count.read(&mut data)?;
                    if !(0. ..=MAX_LIST_VALUES as f64).contains(&n) { return Err("PLY list exceeds its bound".into()) }
                    let face = element.name == "face" && matches!(property.name.as_str(),"vertex_indices"|"vertex_index");
                    let mut values = Vec::new();
                    for _ in 0..n as usize {
                        let v = property.scalar.read(&mut data)?;
                        if face {
                            if v < 0. || v.fract() != 0. || v > super::MAX_VERTICES as f64 { return Err("invalid PLY face index".into()) }
                            values.push(v as u32);
                        }
                    }
                    if face { indices = Some(values) }
                } else {
                    let v = property.scalar.read(&mut data)?;
                    match property.name.as_str() {
                        "x" => xyz[0] = Some(v as f32), "y" => xyz[1] = Some(v as f32), "z" => xyz[2] = Some(v as f32),
                        "red"|"green"|"blue"|"alpha" => {
                            let index = match property.name.as_str() { "red" => 0, "green" => 1, "blue" => 2, _ => 3 };
                            rgba[index] = Some((if property.scalar.integer() { v } else { v*255. }).clamp(0.,255.).round() as u8);
                        }
                        _ => {},
                    }
                }
            }
            if element.name == "vertex" {
                let point = [xyz[0].ok_or("PLY x missing")?,xyz[1].ok_or("PLY y missing")?,xyz[2].ok_or("PLY z missing")?];
                let color = match rgba { [Some(r),Some(g),Some(b),a] => Some([r,g,b,a.unwrap_or(255)]), _ => None };
                model.vertex(point,color)?;
            } else if element.name == "face" {
                let face = indices.ok_or("PLY face has no vertex indices")?;
                face_indices = face_indices.checked_add(face.len()).filter(|&n| n <= super::MAX_TRIANGLES*3).ok_or("PLY faces exceed their bound")?;
                faces.push(face);
            }
        }
    }
    for face in faces { super::mesh::polygon(model,&face,None)? }
    if data.words.as_mut().is_some_and(|w| w.next().is_some()) || (data.words.is_none() && data.at != payload.len()) { return Err("trailing PLY data".into()) }
    Ok(())
}
