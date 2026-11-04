use super::{limits::*, projectmeta::metadata, xml, Model, Plate, Result};
use std::collections::{BTreeMap, BTreeSet};

const MILLIMETRES_PER_METRE: f64 = 1000.;

// Sample input: metadata keys plater_id="2" or index="2" name the same one-based plate identity.
fn identity(info: &BTreeMap<String,String>) -> Result<Option<usize>> {
    let number = |key| info.get(key).map(|v| v.parse::<usize>().ok().filter(|n| (1..=MAX_PLATES).contains(n)).ok_or_else(|| "invalid 3MF plate identity".to_string())).transpose();
    let index = number("index")?;
    let plater = number("plater_id")?;
    if index.zip(plater).is_some_and(|(a,b)| a != b) { return Err("conflicting 3MF plate identities".into()) }
    Ok(index.or(plater))
}

// Sample input: <plate><metadata key="plater_id" value="2"/><metadata key="plater_name" value="Part"/></plate>.
fn plate_metadata(doc: &xml::Document, node: usize) -> Result<BTreeMap<String,String>> {
    let mut keys = BTreeSet::new();
    for i in doc.children(node,"metadata") {
        let n = &doc.nodes[i];
        if let Some(key) = n.attr("key").or_else(|| n.attr("name")) {
            if matches!(key,"index"|"plater_id") && !keys.insert(key) { return Err("duplicate 3MF plate identity field".into()) }
        }
    }
    Ok(metadata(doc,node))
}

// Sample input: model_settings.config has <plate><metadata key="plater_id" value="2"/></plate>; slice_info.config names it with key="index" value="2".
pub(super) fn read(doc: &xml::Document, path: &str, model: &mut Model, assigned: &mut BTreeMap<(u32,u32),u32>, sliced: &mut BTreeSet<usize>) -> Result<()> {
    if path.ends_with("model_settings.config") {
        for i in doc.nodes.iter().enumerate().filter(|(_,n)| n.name == "plate").map(|(i,_)| i) {
            let index = model.plates.len();
            if index >= MAX_PLATES { return Err("too many build plates".into()) }
            let info = plate_metadata(doc,i)?;
            if let Some(id) = identity(&info)? {
                for plate in &model.plates {
                    if identity(&plate.metadata)? == Some(id) { return Err("duplicate 3MF plate identity".into()) }
                }
            }
            let name = info.get("plater_name").filter(|v| !v.is_empty()).cloned().unwrap_or_else(|| format!("Plate {}",index+1));
            model.plates.push(Plate { name, metadata: info });
            for instance in doc.children(i,"model_instance") {
                let info = metadata(doc,instance);
                let object = info.get("object_id").ok_or("plate object id is missing")?.parse::<u32>().map_err(|_| "invalid plate object id")?;
                let instance = info.get("instance_id").map(String::as_str).unwrap_or("0").parse::<u32>().map_err(|_| "invalid plate instance id")?;
                if assigned.len() >= super::MAX_OBJECTS { return Err("too many plate instances".into()) }
                if assigned.insert((object,instance),index as u32).is_some() { return Err("duplicate plate instance".into()) }
            }
        }
    }
    if path.ends_with("slice_info.config") {
        for (ordinal,i) in doc.nodes.iter().enumerate().filter(|(_,n)| n.name == "plate").map(|(i,_)| i).enumerate() {
            if ordinal >= MAX_PLATES { return Err("too many slice plates".into()) }
            let info = plate_metadata(doc,i)?;
            let index = if let Some(id) = identity(&info)? {
                let ids = model.plates.iter().map(|p| identity(&p.metadata)).collect::<Result<Vec<_>>>()?;
                match ids.iter().position(|&n| n == Some(id)) {
                    Some(index) => index,
                    None if ids.iter().any(Option::is_some) && model.plates.iter().any(|p| p.metadata.contains_key("plater_id")) => return Err("slice references a missing 3MF plate identity".into()),
                    None => id-1,
                }
            } else { ordinal };
            if !sliced.insert(index) { return Err("duplicate 3MF slice plate identity".into()) }
            while model.plates.len() <= index { model.plates.push(Plate { name: format!("Plate {}",model.plates.len()+1), metadata: BTreeMap::new() }) }
            model.plates[index].metadata.extend(info);
            filament(doc,i,&mut model.plates[index]);
        }
    }
    Ok(())
}

// Sample input: <plate><filament type="PLA" used_g="3.5" used_m="2"/></plate>.
fn filament(doc: &xml::Document, i: usize, plate: &mut Plate) {
    let mut types = Vec::new();
    let (mut weight,mut length) = (Some(0.),Some(0.));
    let mut count = 0;
    for f in doc.children(i,"filament") {
        let n = &doc.nodes[f];
        count += 1;
        if let Some(t) = n.attr("type") { if !types.contains(&t) { types.push(t) } }
        let quantity = |key| n.attr(key).and_then(|v| super::number(v).ok()).filter(|v| *v >= 0.);
        weight = weight.zip(quantity("used_g")).map(|(a,b)| a+b);
        length = length.zip(quantity("used_m")).map(|(a,b)| a+b*MILLIMETRES_PER_METRE);
    }
    if !types.is_empty() { plate.metadata.insert("filament".into(),types.join(", ")); }
    if count > 0 {
        if let Some(weight) = weight { plate.metadata.entry("weight_g".into()).or_insert_with(|| weight.to_string()); }
        if let Some(length) = length { plate.metadata.entry("filament_mm".into()).or_insert_with(|| length.to_string()); }
    }
}
