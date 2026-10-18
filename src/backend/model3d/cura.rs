use super::limits::*;
use super::{Model,Result};
use std::collections::{BTreeMap,BTreeSet};

const MAX_PROFILE_DEPTH: usize = 32;

#[derive(Default)]
struct Config { general: BTreeMap<String,String>, metadata: BTreeMap<String,String>, values: BTreeMap<String,String>, containers: Vec<(usize,String)> }

// Sample input: [general]\nname = Ender\n[values]\nlayer_height = 0.2\n.
fn ini(source: &str) -> Config {
    let mut config = Config::default();
    let mut section = "";
    for line in source.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') { section = &line[1..line.len()-1]; continue }
        let Some((key,value)) = line.split_once('=') else { continue };
        let (key,value) = (key.trim(),value.trim());
        if key.len() > MAX_METADATA_KEY_BYTES || value.len() > MAX_METADATA_VALUE_BYTES { continue }
        let table = match section {
            "general" => &mut config.general, "metadata" => &mut config.metadata, "values" => &mut config.values,
            "containers" => {
                if let Ok(order) = key.parse::<usize>() { config.containers.push((order,value.into())) }
                continue;
            }
            _ => continue,
        };
        if table.len() < MAX_METADATA_ENTRIES { table.insert(key.into(),value.into()); }
    }
    config.containers.sort_unstable();
    config
}

fn default_values(value: &crate::jsondoc::Json, out: &mut BTreeMap<String,String>) {
    let Some(entries) = value.as_object() else { return };
    for (key,value) in entries {
        if let Some(default) = value.get("default_value") {
            let literal = default.as_str().map(str::to_string).or_else(|| default.as_f64().map(|n| n.to_string()));
            if let Some(literal) = literal { if out.len() < MAX_METADATA_ENTRIES && literal.len() <= MAX_METADATA_VALUE_BYTES { out.insert(key.clone(),literal); } }
        }
        if let Some(children) = value.get("children") { default_values(children,out) }
    }
}

fn resolve<'a>(id: &'a str, key: &str, configs: &'a BTreeMap<String,Config>, active: &mut BTreeSet<String>) -> Option<&'a str> {
    if active.len() >= MAX_PROFILE_DEPTH || !active.insert(id.into()) { return None }
    let result = configs.get(id).and_then(|config| {
        config.values.get(key).map(String::as_str).or_else(|| config.containers.iter().find_map(|(_,id)| resolve(id,key,configs,active)))
    });
    active.remove(id);
    result
}

pub(super) fn read(files: &BTreeMap<String,Vec<u8>>, model: &mut Model) -> Result<()> {
    let mut configs = BTreeMap::new();
    for (path,bytes) in files {
        let Some(path) = path.strip_prefix("Cura/") else { continue };
        if ![".global.cfg",".extruder.cfg",".inst.cfg",".def.json",".xml.fdm_material"].iter().any(|suffix| path.ends_with(suffix)) && path != "user-settings.json" { continue }
        if bytes.len() > MAX_METADATA_BYTES { return Err("Cura profile exceeds its bound".into()) }
        let mut id = None;
        let mut config = Config::default();
        for suffix in [".global.cfg",".extruder.cfg",".inst.cfg"] {
            if let Some(name) = path.strip_suffix(suffix) { id = Some(name.to_string()); config = ini(super::text(bytes)?); break }
        }
        if let Some(name) = path.strip_suffix(".def.json") {
            id = Some(name.into());
            let doc = crate::jsondoc::parse(super::text(bytes)?)?;
            if let Some(settings) = doc.get("settings") { default_values(settings,&mut config.values) }
            if let Some(parent) = doc.get("inherits").and_then(|v| v.as_str()) { config.containers.push((0,parent.into())) }
        }
        if let Some(name) = path.strip_suffix(".xml.fdm_material") {
            id = Some(name.into());
            let doc = super::xml::parse(super::text(bytes)?)?;
            let metadata = doc.child(doc.root,"metadata");
            let name = metadata.and_then(|i| doc.child(i,"name"));
            if let Some(name) = name {
                let field = |key| doc.child(name,key).map(|i| doc.nodes[i].text.trim());
                let label = field("label").map(str::to_string).unwrap_or_else(|| [field("brand"),field("material")].into_iter().flatten().collect::<Vec<_>>().join(" "));
                if !label.is_empty() { config.values.insert("material_name".into(),label); }
            }
        }
        if let Some(id) = id { if configs.insert(id,config).is_some() { return Err("duplicate Cura profile id".into()) } }
    }
    let mut nozzles = Vec::new();
    let mut filaments = Vec::new();
    let has_extruders = configs.values().any(|c| c.metadata.get("type").map(String::as_str) == Some("extruder_train"));
    for (id,config) in &configs {
        let kind = config.metadata.get("type").map(String::as_str);
        if kind == Some("machine") {
            if let Some(name) = config.general.get("name") { super::projectmeta::record(model,"machine_name",name) }
        }
        if !matches!(kind,Some("machine"|"extruder_train")) { continue }
        if kind == Some("machine") {
            if let Some(value) = resolve(id,"layer_height",&configs,&mut BTreeSet::new()) { super::projectmeta::record(model,"layer_height",value) }
            if !config.general.contains_key("name") {
                if let Some(value) = resolve(id,"machine_name",&configs,&mut BTreeSet::new()) { super::projectmeta::record(model,"machine_name",value) }
            }
        }
        if kind == Some("extruder_train") || !has_extruders {
            if let Some(value) = resolve(id,"machine_nozzle_size",&configs,&mut BTreeSet::new()) {
                if !nozzles.contains(&value) { nozzles.push(value) }
            }
            if let Some(value) = resolve(id,"material_name",&configs,&mut BTreeSet::new()).or_else(|| resolve(id,"material_type",&configs,&mut BTreeSet::new())) {
                if !filaments.contains(&value) { filaments.push(value) }
            }
        }
    }
    if !nozzles.is_empty() { model.metadata("nozzle_mm",&nozzles.join(", ")) }
    if !filaments.is_empty() { model.metadata("filament",&filaments.join(", ")) }
    if let Some(bytes) = files.get("Cura/user-settings.json") {
        let json = crate::jsondoc::parse(super::text(bytes)?)?;
        if let Some(groups) = json.as_object() {
            for (_,group) in groups {
                if let Some(values) = group.as_object() {
                    for (key,value) in values {
                        if let Some(value) = value.as_str().map(str::to_string).or_else(|| value.as_f64().map(|n| n.to_string())) { super::projectmeta::record(model,key,&value) }
                    }
                }
            }
        }
    }
    if !configs.is_empty() { model.metadata("slicer","Cura") }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cura_workspace_container_order_and_material_names_are_read_without_evaluation() {
        let files = [
            ("Cura/machine.global.cfg".into(),b"[general]\nname = Ender-3\n[metadata]\ntype = machine\n[containers]\n0 = user\n1 = quality\n2 = definition\n".to_vec()),
            ("Cura/extruder.extruder.cfg".into(),b"[metadata]\ntype = extruder_train\n[containers]\n0 = pla\n1 = definition\n".to_vec()),
            ("Cura/user.inst.cfg".into(),b"[values]\nlayer_height = 0.12\n".to_vec()),
            ("Cura/quality.inst.cfg".into(),b"[values]\nlayer_height = 0.2\n".to_vec()),
            ("Cura/definition.def.json".into(),b"{\"settings\":{\"machine_nozzle_size\":{\"default_value\":0.4},\"layer_height\":{\"default_value\":0.3},\"machine_name\":{\"default_value\":\"Generic\"}}}".to_vec()),
            ("Cura/pla.xml.fdm_material".into(),b"<fdmmaterial><metadata><name><brand>Generic</brand><material>PLA</material></name></metadata></fdmmaterial>".to_vec()),
        ].into();
        let mut model = Model::default();
        read(&files,&mut model).unwrap();
        let facts = model.slicer(None);
        assert_eq!(facts.printer.as_deref(),Some("Ender-3"));
        assert_eq!(facts.filament.as_deref(),Some("Generic PLA"));
        assert_eq!(facts.layer_height_mm,Some(0.12));
        assert_eq!(facts.nozzle_mm,[0.4]);
    }
}
