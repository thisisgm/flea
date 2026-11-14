use super::limits::*;

use super::project_part::{load, Part, Shape};
use super::{amf, xml, Model, Result, Transform, IDENTITY};
use std::collections::BTreeMap;

// Sample input: "1 0 0 0 1 0 0 0 1 10 20 30", with translations in model units.
pub(super) fn transform(value: Option<&str>, unit: f64) -> Result<Transform> {
    let Some(value) = value else {
        return Ok(IDENTITY);
    };
    let values = value
        .split_whitespace()
        .map(super::number)
        .collect::<Result<Vec<_>>>()?;
    let mut matrix: Transform = values.try_into().map_err(|_| "invalid 3MF transform")?;
    for value in &mut matrix[9..] {
        *value *= unit
    }
    Ok(matrix)
}

// Sample input: from="3D/main.model", target="Objects/one.model" resolves to "3d/objects/one.model".
fn member(from: &str, target: &str) -> Result<String> {
    if target.contains(['\\', '\0', ':', '?', '#']) {
        return Err("unsupported 3MF part URI".into());
    }
    let decoded = crate::paths::percent_decode(target);
    let mut parts: Vec<&str> = if decoded.starts_with('/') {
        Vec::new()
    } else {
        from.rsplit_once('/')
            .map(|(p, _)| p.split('/').collect())
            .unwrap_or_default()
    };
    for part in decoded.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err("3MF part URI escapes archive".into());
                }
            }
            _ => parts.push(part),
        }
    }
    Ok(parts.join("/").to_ascii_lowercase())
}

fn root(files: &BTreeMap<String, Vec<u8>>) -> Result<String> {
    if let Some((_, bytes)) = files
        .iter()
        .find(|(path, _)| path.eq_ignore_ascii_case("_rels/.rels"))
    {
        let doc = xml::parse(super::text(bytes)?)?;
        for n in &doc.nodes {
            if n.name == "Relationship" && n.attr("Type").is_some_and(|v| v.ends_with("/3dmodel")) {
                if n.attr("TargetMode") == Some("External") {
                    return Err("external 3MF model is refused".into());
                }
                return member(
                    "",
                    n.attr("Target")
                        .ok_or("3MF relationship target is missing")?,
                );
            }
        }
    }
    files
        .keys()
        .find(|n| n.eq_ignore_ascii_case("3D/3dmodel.model"))
        .map(|p| p.to_ascii_lowercase())
        .ok_or_else(|| "3MF root model is missing".into())
}

struct Instance<'a> {
    parts: &'a BTreeMap<String, Part>,
    active: Vec<(String, u32)>,
    expansions: usize,
}

impl Instance<'_> {
    fn add(
        &mut self,
        path: &str,
        id: u32,
        transform: Transform,
        plate: u32,
        model: &mut Model,
    ) -> Result<()> {
        if self.active.len() >= MAX_COMPONENT_DEPTH
            || self.active.iter().any(|(p, i)| p == path && *i == id)
        {
            return Err("cyclic or excessive 3MF components".into());
        }
        if self.expansions >= MAX_COMPONENT_EXPANSIONS {
            return Err(format!("excessive 3MF component expansions at {path}#{id}"));
        }
        self.expansions += 1;
        self.active.push((path.into(), id));
        let part = self
            .parts
            .get(path)
            .ok_or("3MF component part is missing")?;
        let object = part
            .objects
            .get(&id)
            .ok_or("3MF component object is missing")?
            .as_ref()
            .map_err(Clone::clone)?;
        match &object.shape {
            Shape::Mesh { vertices, faces } => {
                let first = model.triangles.len();
                let base = model.vertices.len() as u32;
                for &point in vertices {
                    model.vertex(amf::transformed(point, transform), None)?;
                }
                let reflected = transform[0]
                    * (transform[4] * transform[8] - transform[7] * transform[5])
                    - transform[3] * (transform[1] * transform[8] - transform[7] * transform[2])
                    + transform[6] * (transform[1] * transform[5] - transform[4] * transform[2])
                    < 0.;
                for face in faces {
                    let mut indices = face.vertices.map(|i| base + i);
                    let mut colors = face.colors;
                    if reflected {
                        indices.swap(1, 2);
                        colors.swap(1, 2);
                    }
                    model.triangle(indices, colors, face.material)?;
                }
                model.object(&object.name, plate, first, transform)?;
            }
            Shape::Components(components) => {
                for c in components {
                    let target = c
                        .path
                        .as_deref()
                        .map(|p| member(path, p))
                        .transpose()?
                        .unwrap_or_else(|| path.into());
                    self.add(
                        &target,
                        c.id,
                        amf::compose(transform, c.transform),
                        plate,
                        model,
                    )?;
                }
            }
        }
        self.active.pop();
        Ok(())
    }
}

pub(super) fn parse(bytes: &[u8], model: &mut Model, budget: &mut xml::Budget) -> Result<()> {
    let files = super::zip::unpack(bytes)?;
    let mut names = std::collections::BTreeSet::new();
    for path in files.keys() {
        if !names.insert(path.to_ascii_lowercase()) {
            return Err(format!("ambiguous 3MF part name: {path}"));
        }
    }
    let sliced = files
        .keys()
        .any(|path| path.to_ascii_lowercase().ends_with("slice_info.config"));
    let root = root(&files)?;
    let plates = super::projectmeta::read(&files, model)?;
    let mut parts = BTreeMap::new();
    let archive_storage = bytes.len() + files.values().map(|v| v.capacity()).sum::<usize>();
    // A part's XML is dropped after its compact geometry and component records are extracted.
    for (path, bytes) in &files {
        if !path.to_ascii_lowercase().ends_with(".model") {
            continue;
        }
        if parts.len() >= MAX_PARTS {
            return Err("too many 3MF model parts".into());
        }
        budget.retained(archive_storage + parts.values().map(Part::storage).sum::<usize>());
        parts.insert(
            path.to_ascii_lowercase(),
            load(bytes, model, budget, path.eq_ignore_ascii_case(&root))?,
        );
    }
    let main = parts.get(&root).ok_or("3MF root part is missing")?;
    let build = main
        .build
        .as_ref()
        .ok_or("3MF build is missing")?
        .as_ref()
        .map_err(Clone::clone)?;
    let mut instances = Instance {
        parts: &parts,
        active: Vec::new(),
        expansions: 0,
    };
    let mut occurrence = BTreeMap::new();
    for item in build {
        let id = item.id;
        let index = occurrence.entry(id).or_insert(0u32);
        let plate = plates.instances.get(&(id, *index)).copied().unwrap_or(0);
        *index += 1;
        let path = item
            .path
            .as_deref()
            .map(|p| member(&root, p))
            .transpose()?
            .unwrap_or_else(|| root.clone());
        instances.add(&path, id, item.transform, plate, model)?;
    }
    for (path, bytes) in &files {
        let lower = path.to_ascii_lowercase();
        if !lower.starts_with("metadata/") || !lower.ends_with(".png") {
            continue;
        }
        let file = lower.rsplit('/').next().unwrap_or("");
        let plate = file
            .strip_prefix("plate_")
            .and_then(|v| v.strip_suffix(".png"))
            .and_then(|v| v.parse::<u32>().ok())
            .and_then(|n| n.checked_sub(1));
        if let Some(plate) = plate {
            if plate >= MAX_PLATES as u32 {
                return Err("3MF thumbnail plate exceeds its bound".into());
            }
            while model.plates.len() <= plate as usize {
                model.plates.push(super::Plate {
                    name: format!("Plate {}", model.plates.len() + 1),
                    metadata: BTreeMap::new(),
                })
            }
        }
        if file == "thumbnail.png" || plate.is_some() {
            model.thumbnail(plate, "png", bytes.clone())?
        }
    }
    if sliced {
        super::slicer::totals(model)
    }
    super::cura::read(&files, model)?;
    Ok(())
}
