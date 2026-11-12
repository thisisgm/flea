use super::{amf, xml, Model, Point, Result, Transform, Triangle};
use std::collections::BTreeMap;

pub(super) struct Part {
    pub objects: BTreeMap<u32, Result<Object>>,
    pub build: Option<Result<Vec<Component>>>,
}

impl Part {
    pub fn storage(&self) -> usize {
        self.objects.len() * std::mem::size_of::<(u32, Result<Object>)>() * 2
            + self
                .objects
                .values()
                .filter_map(|o| o.as_ref().ok())
                .map(|o| {
                    o.name.capacity()
                        + match &o.shape {
                            Shape::Mesh { vertices, faces } => {
                                vertices.capacity() * std::mem::size_of::<Point>()
                                    + faces.capacity() * std::mem::size_of::<Triangle>()
                            }
                            Shape::Components(c) => components_storage(c),
                        }
                })
                .sum::<usize>()
            + self
                .build
                .as_ref()
                .and_then(|b| b.as_ref().ok())
                .map_or(0, components_storage)
    }
}

fn components_storage(components: &Vec<Component>) -> usize {
    components.capacity() * std::mem::size_of::<Component>()
        + components
            .iter()
            .filter_map(|c| c.path.as_ref())
            .map(String::capacity)
            .sum::<usize>()
}

pub(super) struct Object {
    pub name: String,
    pub shape: Shape,
}

pub(super) enum Shape {
    Mesh {
        vertices: Vec<Point>,
        faces: Vec<Triangle>,
    },
    Components(Vec<Component>),
}

pub(super) struct Component {
    pub path: Option<String>,
    pub id: u32,
    pub transform: Transform,
}

// Sample input: <component objectid="1" path="/3D/Objects/a.model" transform="1 0 0 0 1 0 0 0 1 10 0 0"/>.
fn component(n: &xml::Node, unit: f64) -> Result<Component> {
    Ok(Component {
        path: n.attr("path").map(str::to_string),
        id: n.index("objectid")?,
        transform: super::project::transform(n.attr("transform"), unit)?,
    })
}

fn property(
    properties: &BTreeMap<u32, Vec<u32>>,
    pid: Option<u32>,
    index: u32,
) -> Result<Option<u32>> {
    pid.map(|id| {
        properties
            .get(&id)
            .and_then(|v| v.get(index as usize))
            .copied()
            .ok_or_else(|| "unsupported or missing 3MF material property".into())
    })
    .transpose()
}

fn optional_index(n: &xml::Node, name: &str) -> Result<Option<u32>> {
    n.attr(name)
        .map(|v| v.parse().map_err(|_| "invalid 3MF property index".into()))
        .transpose()
}

fn release(n: &mut xml::Node) {
    n.attrs = Vec::new();
    n.name = String::new();
    n.text = String::new();
}

// Sample input: <build><item objectid="2"/></build>, or <components><component objectid="1"/></components>.
fn components(
    doc: &mut xml::Document,
    root: usize,
    tag: &str,
    unit: f64,
) -> Result<Vec<Component>> {
    let mut out = Vec::new();
    for offset in 0..doc.nodes[root].children.len() {
        let index = doc.nodes[root].children[offset];
        if doc.nodes[index].name != tag {
            continue;
        }
        let value = component(&doc.nodes[index], unit)?;
        release(&mut doc.nodes[index]);
        out.push(value);
    }
    Ok(out)
}

// Sample input: <object id="1" name="Cube" pid="5" pindex="0"><mesh><vertices><vertex x="0" y="0" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2" p1="1"/></triangles></mesh></object>.
fn object(
    doc: &mut xml::Document,
    i: usize,
    id: u32,
    unit: f64,
    properties: &BTreeMap<u32, Vec<u32>>,
    model: &Model,
) -> Result<Object> {
    let n = &doc.nodes[i];
    let name = n
        .attr("name")
        .map(str::to_string)
        .unwrap_or_else(|| format!("Object {id}"));
    let shape = if let Some(mesh) = doc.child(i, "mesh") {
        let object_pid = optional_index(n, "pid")?;
        let object_p = optional_index(n, "pindex")?.unwrap_or(0);
        let vroot = doc
            .child(mesh, "vertices")
            .ok_or("3MF vertices are missing")?;
        let troot = doc
            .child(mesh, "triangles")
            .ok_or("3MF triangles are missing")?;
        let mut vertices = Vec::with_capacity(doc.children(vroot, "vertex").count());
        for offset in 0..doc.nodes[vroot].children.len() {
            let index = doc.nodes[vroot].children[offset];
            let v = &doc.nodes[index];
            if v.name != "vertex" {
                continue;
            }
            vertices.push(
                [
                    v.number("x")? * unit,
                    v.number("y")? * unit,
                    v.number("z")? * unit,
                ]
                .map(|v| v as f32),
            );
            release(&mut doc.nodes[index]);
        }
        let mut faces = Vec::with_capacity(doc.children(troot, "triangle").count());
        for offset in 0..doc.nodes[troot].children.len() {
            let index = doc.nodes[troot].children[offset];
            let t = &doc.nodes[index];
            if t.name != "triangle" {
                continue;
            }
            let mut face = [0; 3];
            for (j, key) in ["v1", "v2", "v3"].iter().enumerate() {
                face[j] = t.index(key)?;
                if face[j] as usize >= vertices.len() {
                    return Err("3MF triangle index is out of range".into());
                }
            }
            let pid = optional_index(t, "pid")?.or(object_pid);
            let p1 = optional_index(t, "p1")?.unwrap_or(object_p);
            let indices = [
                p1,
                optional_index(t, "p2")?.unwrap_or(p1),
                optional_index(t, "p3")?.unwrap_or(p1),
            ];
            let mut colors = [None; 3];
            for j in 0..3 {
                colors[j] = property(properties, pid, indices[j])?
                    .and_then(|i| model.materials[i as usize].color)
            }
            faces.push(Triangle {
                vertices: face,
                colors,
                material: property(properties, pid, p1)?,
            });
            release(&mut doc.nodes[index]);
        }
        Shape::Mesh { vertices, faces }
    } else if let Some(root) = doc.child(i, "components") {
        Shape::Components(components(doc, root, "component", unit)?)
    } else {
        return Err("3MF object contains no mesh or components".into());
    };
    Ok(Object { name, shape })
}

// Sample input: <model unit="millimeter"><resources><basematerials id="5"><base name="PLA" displaycolor="#FF0000"/></basematerials><object id="1">...</object></resources><build><item objectid="1"/></build></model>.
pub(super) fn load(
    bytes: &[u8],
    model: &mut Model,
    budget: &mut xml::Budget,
    main: bool,
) -> Result<Part> {
    budget.begin_document();
    let mut doc = xml::parse_bounded(super::text(bytes)?, budget)?;
    if doc.nodes[doc.root].name != "model" {
        return Err("invalid 3MF model root".into());
    }
    let unit = amf::scale(doc.nodes[doc.root].attr("unit").unwrap_or("millimeter"))?;
    for i in doc.children(doc.root, "metadata") {
        let n = &doc.nodes[i];
        if let Some(key) = n.attr("name") {
            super::projectmeta::record(model, key, &n.text)
        }
    }
    let resources = doc
        .child(doc.root, "resources")
        .ok_or("3MF resources are missing")?;
    let mut objects = BTreeMap::new();
    let mut properties = BTreeMap::new();
    let mut ids = std::collections::BTreeSet::new();
    for &i in &doc.nodes[resources].children {
        let n = &doc.nodes[i];
        let Some(id) = n.attr("id") else { continue };
        let id = id.parse::<u32>().map_err(|_| "invalid 3MF resource id")?;
        if !ids.insert(id) {
            return Err("duplicate 3MF resource id".into());
        }
        if n.name == "object" {
            objects.insert(id, i);
            continue;
        }
        if !matches!(n.name.as_str(), "basematerials" | "colorgroup") {
            continue;
        }
        let mut entries = Vec::new();
        for &c in &n.children {
            let c = &doc.nodes[c];
            if !matches!(c.name.as_str(), "base" | "color") {
                continue;
            }
            if model.materials.len() >= super::MAX_OBJECTS {
                return Err("too many 3MF materials".into());
            }
            entries.push(model.materials.len() as u32);
            model.materials.push(super::Material {
                name: c.attr("name").unwrap_or("").into(),
                color: c
                    .attr("displaycolor")
                    .or_else(|| c.attr("color"))
                    .map(super::color)
                    .transpose()?,
            });
        }
        properties.insert(id, entries);
    }
    super::properties::resolve(&doc, resources, &mut properties, model)?;
    let build = if main {
        doc.child(doc.root, "build")
            .map(|b| components(&mut doc, b, "item", unit))
    } else {
        None
    };
    // Invalid unused mesh resources were not interpreted by the original reader.
    let objects = objects
        .into_iter()
        .map(|(id, i)| (id, object(&mut doc, i, id, unit, &properties, model)))
        .collect();
    drop(doc);
    Ok(Part { objects, build })
}
