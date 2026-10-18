use super::limits::*;

use super::{xml, Material, Model, Result};
use std::collections::BTreeMap;

fn name_part<'a>(parts: &mut Vec<&'a str>, length: &mut usize, part: &'a str) -> Result<()> {
    let separator = if parts.is_empty() { 0 } else { 3 };
    *length = length.checked_add(separator).and_then(|n| n.checked_add(part.len()))
        .filter(|&n| n <= MAX_NAME_BYTES).ok_or("3D material name exceeds its bound")?;
    parts.push(part);
    Ok(())
}

// Resolve composite and multi-property groups after the base palettes, independent of resource order.
pub(super) fn resolve(doc: &xml::Document, resources: usize, properties: &mut BTreeMap<u32,Vec<u32>>, model: &mut Model) -> Result<()> {
    let mut kinds = BTreeMap::new();
    for &i in &doc.nodes[resources].children {
        let n = &doc.nodes[i];
        if n.attr("id").is_some() { kinds.insert(n.index("id")?,n.name.as_str()); }
    }
    let mut pending = Vec::new();
    for &i in &doc.nodes[resources].children {
        let n = &doc.nodes[i];
        if !matches!(n.name.as_str(),"compositematerials"|"multiproperties") { continue }
        let ids = if n.name == "compositematerials" { vec![n.index("matid")?] }
            else { n.attr("pids").ok_or("3MF multi-property ids missing")?.split_whitespace().map(|v| v.parse::<u32>().map_err(|_| "invalid 3MF property id".into())).collect::<Result<Vec<_>>>()? };
        if ids.is_empty() || ids.len() > MAX_MATERIAL_GROUP { return Err("3MF material group exceeds its bound".into()) }
        for id in &ids {
            let kind = kinds.get(id).copied();
            let allowed = if n.name == "compositematerials" { kind == Some("basematerials") }
                else { matches!(kind,Some("basematerials"|"colorgroup"|"compositematerials")) };
            if !allowed { return Err("unsupported or missing 3MF material resource reference".into()) }
        }
        pending.push((i,ids));
    }
    while !pending.is_empty() {
        let before = pending.len();
        let mut next = Vec::new();
        for (i,ids) in pending {
            let n = &doc.nodes[i];
            let id = n.index("id")?;
            if ids.iter().any(|id| !properties.contains_key(id)) { next.push((i,ids)); continue }
            let mut entries = Vec::new();
            for &c in &n.children {
                let c = &doc.nodes[c];
                let mut name = Vec::new();
                let mut name_length = 0;
                let mut color = None;
                if n.name == "compositematerials" {
                    if c.name != "composite" { continue }
                    let weights = c.attr("values").ok_or("3MF material weights missing")?.split_whitespace().map(super::number).collect::<Result<Vec<_>>>()?;
                    let base = &properties[&ids[0]];
                    if weights.len() != base.len() || weights.iter().any(|v| *v < 0.) { return Err("invalid 3MF material weights".into()) }
                    let total = weights.iter().sum::<f64>();
                    if total <= 0. || (total-1.).abs() > 0.001 { return Err("3MF material weights do not sum to one".into()) }
                    let mut mixed = [0.;4];
                    let mut known = false;
                    for (&m,&weight) in base.iter().zip(&weights) {
                        let material = &model.materials[m as usize];
                        if weight == 0. { continue }
                        name_part(&mut name,&mut name_length,&material.name)?;
                        if let Some(c) = material.color { known = true; for channel in 0..4 { mixed[channel] += f64::from(c[channel])*weight/total } }
                    }
                    if known { color = Some(mixed.map(|v| v.round().clamp(0.,255.) as u8)) }
                } else {
                    if c.name != "multi" { continue }
                    let indices = c.attr("pindices").ok_or("3MF multi-property indices missing")?.split_whitespace()
                        .map(|v| v.parse::<usize>().map_err(|_| "invalid 3MF property index".into())).collect::<Result<Vec<_>>>()?;
                    if indices.len() != ids.len() { return Err("3MF multi-property size mismatch".into()) }
                    for (&group,&index) in ids.iter().zip(&indices) {
                        let material = properties[&group].get(index).and_then(|&i| model.materials.get(i as usize)).ok_or("3MF multi-property index is out of range")?;
                        name_part(&mut name,&mut name_length,&material.name)?;
                        color = material.color.or(color);
                    }
                }
                if model.materials.len() >= super::MAX_OBJECTS { return Err("too many 3MF material combinations".into()) }
                let name = name.join(" + ");
                entries.push(model.materials.len() as u32);
                model.materials.push(Material { name, color });
            }
            properties.insert(id,entries);
        }
        if next.len() == before { return Err("cyclic or missing 3MF material groups".into()) }
        pending = next;
    }
    Ok(())
}
