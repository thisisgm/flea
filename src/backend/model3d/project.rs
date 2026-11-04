use super::limits::*;

use super::{amf, xml, Model, Result, Transform, IDENTITY};
use std::collections::BTreeMap;

struct Part {
    doc: xml::Document,
    unit: f64,
    objects: BTreeMap<u32,usize>,
    properties: BTreeMap<u32,Vec<u32>>,
}

// Sample input: "1 0 0 0 1 0 0 0 1 10 20 30", with translations in model units.
fn transform(value: Option<&str>, unit: f64) -> Result<Transform> {
    let Some(value) = value else { return Ok(IDENTITY) };
    let values = value.split_whitespace().map(super::number).collect::<Result<Vec<_>>>()?;
    let mut matrix: Transform = values.try_into().map_err(|_| "invalid 3MF transform")?;
    for value in &mut matrix[9..] { *value *= unit }
    Ok(matrix)
}

// Sample input: from="3D/main.model", target="Objects/one.model" resolves to "3d/objects/one.model".
fn member(from: &str, target: &str) -> Result<String> {
    if target.contains(['\\','\0',':','?','#']) { return Err("unsupported 3MF part URI".into()) }
    let decoded = crate::paths::percent_decode(target);
    let mut parts: Vec<&str> = if decoded.starts_with('/') { Vec::new() } else { from.rsplit_once('/').map(|(p,_)| p.split('/').collect()).unwrap_or_default() };
    for part in decoded.split('/') {
        match part {
            ""|"." => {},
            ".." => { if parts.pop().is_none() { return Err("3MF part URI escapes archive".into()) } },
            _ => parts.push(part),
        }
    }
    Ok(parts.join("/").to_ascii_lowercase())
}

fn root(files: &BTreeMap<String,Vec<u8>>) -> Result<String> {
    if let Some((_,bytes)) = files.iter().find(|(path,_)| path.eq_ignore_ascii_case("_rels/.rels")) {
        let doc = xml::parse(super::text(bytes)?)?;
        for n in &doc.nodes {
            if n.name == "Relationship" && n.attr("Type").is_some_and(|v| v.ends_with("/3dmodel")) {
                if n.attr("TargetMode") == Some("External") { return Err("external 3MF model is refused".into()) }
                return member("",n.attr("Target").ok_or("3MF relationship target is missing")?);
            }
        }
    }
    files.keys().find(|n| n.eq_ignore_ascii_case("3D/3dmodel.model")).map(|p| p.to_ascii_lowercase()).ok_or_else(|| "3MF root model is missing".into())
}

fn load(bytes: &[u8], model: &mut Model) -> Result<Part> {
    let doc = xml::parse(super::text(bytes)?)?;
    if doc.nodes[doc.root].name != "model" { return Err("invalid 3MF model root".into()) }
    let unit = amf::scale(doc.nodes[doc.root].attr("unit").unwrap_or("millimeter"))?;
    for i in doc.children(doc.root,"metadata") {
        let n = &doc.nodes[i];
        if let Some(key) = n.attr("name") { super::projectmeta::record(model,key,&n.text) }
    }
    let resources = doc.child(doc.root,"resources").ok_or("3MF resources are missing")?;
    let mut objects = BTreeMap::new();
    let mut properties = BTreeMap::new();
    let mut ids = std::collections::BTreeSet::new();
    for &i in &doc.nodes[resources].children {
        let n = &doc.nodes[i];
        let Some(id) = n.attr("id") else { continue };
        let id = id.parse::<u32>().map_err(|_| "invalid 3MF resource id")?;
        if !ids.insert(id) { return Err("duplicate 3MF resource id".into()) }
        if n.name == "object" { objects.insert(id,i); continue }
        if !matches!(n.name.as_str(),"basematerials"|"colorgroup") { continue }
        let mut entries = Vec::new();
        for &c in &n.children {
            let c = &doc.nodes[c];
            if !matches!(c.name.as_str(),"base"|"color") { continue }
            if model.materials.len() >= super::MAX_OBJECTS { return Err("too many 3MF materials".into()) }
            entries.push(model.materials.len() as u32);
            model.materials.push(super::Material { name: c.attr("name").unwrap_or("").into(), color: c.attr("displaycolor").or_else(|| c.attr("color")).map(super::color).transpose()? });
        }
        properties.insert(id,entries);
    }
    super::properties::resolve(&doc,resources,&mut properties,model)?;
    Ok(Part { doc, unit, objects, properties })
}

fn property(part: &Part, pid: Option<u32>, index: u32) -> Result<Option<u32>> {
    pid.map(|id| part.properties.get(&id).and_then(|v| v.get(index as usize)).copied().ok_or_else(|| "unsupported or missing 3MF material property".into())).transpose()
}

fn optional_index(n: &xml::Node, name: &str) -> Result<Option<u32>> {
    n.attr(name).map(|v| v.parse().map_err(|_| "invalid 3MF property index".into())).transpose()
}

struct Instance<'a> {
    parts: &'a BTreeMap<String,Part>,
    active: Vec<(String,u32)>,
    expansions: usize,
}

impl Instance<'_> {
    fn add(&mut self, path: &str, id: u32, transform: Transform, plate: u32, model: &mut Model) -> Result<()> {
        if self.active.len() >= MAX_COMPONENT_DEPTH || self.active.iter().any(|(p,i)| p == path && *i == id) { return Err("cyclic or excessive 3MF components".into()) }
        if self.expansions >= MAX_COMPONENT_EXPANSIONS { return Err(format!("excessive 3MF component expansions at {path}#{id}")) }
        self.expansions += 1;
        self.active.push((path.into(),id));
        let part = self.parts.get(path).ok_or("3MF component part is missing")?;
        let i = *part.objects.get(&id).ok_or("3MF component object is missing")?;
        let n = &part.doc.nodes[i];
        let name = n.attr("name").map(str::to_string).unwrap_or_else(|| format!("Object {id}"));
        if let Some(mesh) = part.doc.child(i,"mesh") {
            let first = model.triangles.len();
            let vertices = part.doc.child(mesh,"vertices").ok_or("3MF vertices are missing")?;
            let mut indices = Vec::new();
            for v in part.doc.children(vertices,"vertex") {
                let v = &part.doc.nodes[v];
                let point = [v.number("x")?*part.unit,v.number("y")?*part.unit,v.number("z")?*part.unit].map(|v| v as f32);
                indices.push(model.vertex(amf::transformed(point,transform),None)?);
            }
            let triangles = part.doc.child(mesh,"triangles").ok_or("3MF triangles are missing")?;
            let object_pid = optional_index(n,"pid")?;
            let object_p = optional_index(n,"pindex")?.unwrap_or(0);
            for t in part.doc.children(triangles,"triangle") {
                let t = &part.doc.nodes[t];
                let mut face = [0;3];
                for (j,key) in ["v1","v2","v3"].iter().enumerate() { face[j] = *indices.get(t.index(key)? as usize).ok_or("3MF triangle index is out of range")? }
                let pid = optional_index(t,"pid")?.or(object_pid);
                let p1 = optional_index(t,"p1")?.unwrap_or(object_p);
                let properties = [p1,optional_index(t,"p2")?.unwrap_or(p1),optional_index(t,"p3")?.unwrap_or(p1)];
                let mut colors = [None;3];
                for j in 0..3 { colors[j] = property(part,pid,properties[j])?.and_then(|i| model.materials[i as usize].color) }
                let determinant = transform[0]*(transform[4]*transform[8]-transform[7]*transform[5])
                    - transform[3]*(transform[1]*transform[8]-transform[7]*transform[2])
                    + transform[6]*(transform[1]*transform[5]-transform[4]*transform[2]);
                if determinant < 0. { face.swap(1,2); colors.swap(1,2); }
                model.triangle(face,colors,property(part,pid,p1)?)?;
            }
            model.object(&name,plate,first,transform)?;
        } else if let Some(components) = part.doc.child(i,"components") {
            for c in part.doc.children(components,"component") {
                let c = &part.doc.nodes[c];
                let target = c.attr("path").map(|target| member(path,target)).transpose()?.unwrap_or_else(|| path.into());
                let matrix = amf::compose(transform,super::project::transform(c.attr("transform"),part.unit)?);
                self.add(&target,c.index("objectid")?,matrix,plate,model)?;
            }
        } else { return Err("3MF object contains no mesh or components".into()) }
        self.active.pop();
        Ok(())
    }
}

pub(super) fn parse(bytes: &[u8], model: &mut Model) -> Result<()> {
    let files = super::zip::unpack(bytes)?;
    let mut names = std::collections::BTreeSet::new();
    for path in files.keys() {
        if !names.insert(path.to_ascii_lowercase()) { return Err(format!("ambiguous 3MF part name: {path}")) }
    }
    let sliced = files.keys().any(|path| path.to_ascii_lowercase().ends_with("slice_info.config"));
    let root = root(&files)?;
    let plates = super::projectmeta::read(&files,model)?;
    let mut parts = BTreeMap::new();
    for (path,bytes) in &files {
        if !path.to_ascii_lowercase().ends_with(".model") { continue }
        if parts.len() >= MAX_PARTS { return Err("too many 3MF model parts".into()) }
        parts.insert(path.to_ascii_lowercase(),load(bytes,model)?);
    }
    let main = parts.get(&root).ok_or("3MF root part is missing")?;
    let build = main.doc.child(main.doc.root,"build").ok_or("3MF build is missing")?;
    let mut instances = Instance { parts: &parts, active: Vec::new(), expansions: 0 };
    let mut occurrence = BTreeMap::new();
    for item in main.doc.children(build,"item") {
        let item = &main.doc.nodes[item];
        let id = item.index("objectid")?;
        let index = occurrence.entry(id).or_insert(0u32);
        let plate = plates.instances.get(&(id,*index)).copied().unwrap_or(0);
        *index += 1;
        let path = item.attr("path").map(|target| member(&root,target)).transpose()?.unwrap_or_else(|| root.clone());
        instances.add(&path,id,transform(item.attr("transform"),main.unit)?,plate,model)?;
    }
    for (path,bytes) in &files {
        let lower = path.to_ascii_lowercase();
        if !lower.starts_with("metadata/") || !lower.ends_with(".png") { continue }
        let file = lower.rsplit('/').next().unwrap_or("");
        let plate = file.strip_prefix("plate_").and_then(|v| v.strip_suffix(".png")).and_then(|v| v.parse::<u32>().ok()).and_then(|n| n.checked_sub(1));
        if let Some(plate) = plate {
            if plate >= MAX_PLATES as u32 { return Err("3MF thumbnail plate exceeds its bound".into()) }
            while model.plates.len() <= plate as usize { model.plates.push(super::Plate { name: format!("Plate {}",model.plates.len()+1), metadata: BTreeMap::new() }) }
        }
        if file == "thumbnail.png" || plate.is_some() { model.thumbnail(plate,"png",bytes.clone())? }
    }
    if sliced { super::slicer::totals(model) }
    super::cura::read(&files,model)?;
    Ok(())
}
