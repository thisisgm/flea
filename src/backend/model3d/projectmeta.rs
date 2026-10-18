use super::{xml, Model, Plate, Result};
use std::collections::BTreeMap;

pub(super) fn record(model: &mut Model, key: &str, value: &str) {
    model.metadata(key,value);
    let key_lower = key.to_ascii_lowercase();
    let canonical = match key_lower.strip_prefix("cura:").unwrap_or(&key_lower) {
        "printer_model"|"printer_model_id"|"printer_settings_id"|"machine_name"|"machine" => "printer",
        "filament_type"|"material"|"material_type"|"material_name" => "filament",
        "nozzle_diameter"|"machine_nozzle_size" => "nozzle_mm",
        "layer_height"|"layerheight"|"layer height" => "layer_height_mm",
        "time"|"prediction"|"estimated_time"|"estimated_print_time"|"estimated printing time (normal mode)"|"total estimated time" => "print_time",
        "filament used [mm]"|"filament_used_mm"|"total filament length [mm]" => "filament_mm",
        "filament used"|"filament_length" => "filament_length",
        "filament used [g]"|"total filament used [g]"|"filament_used_g"|"weight"|"weight_g"|"total filament weight [g]" => "weight_g",
        "application"|"producer"|"generator" => "slicer",
        _ => return,
    };
    model.metadata(canonical,value);
}

pub(super) fn pairs(model: &mut Model, source: &str) {
    for line in source.lines() {
        let line = line.trim().trim_start_matches(';').trim();
        if let Some((key,value)) = line.split_once('=').or_else(|| line.split_once(':')) {
            record(model,key.trim(),value.trim());
            if key.trim().eq_ignore_ascii_case("model printing time") {
                for field in value.split(';').skip(1) {
                    if let Some((key,value)) = field.split_once(':') {
                        if key.trim().eq_ignore_ascii_case("total estimated time") { record(model,key.trim(),value.trim()) }
                    }
                }
            }
        }
        for slicer in ["BambuStudio","Bambu Studio","PrusaSlicer","OrcaSlicer","Cura"] {
            if line.contains(slicer) { model.metadata("slicer",slicer) }
        }
    }
}

fn metadata(doc: &xml::Document, node: usize) -> BTreeMap<String,String> {
    let mut model = Model::default();
    for i in doc.children(node,"metadata") {
        let n = &doc.nodes[i];
        if let Some(key) = n.attr("key").or_else(|| n.attr("name")) { record(&mut model,key,n.attr("value").unwrap_or(n.text.trim())) }
    }
    model.metadata
}

pub(super) struct Plates {
    pub instances: BTreeMap<(u32,u32),u32>,
}

pub(super) fn read(files: &BTreeMap<String,Vec<u8>>, model: &mut Model) -> Result<Plates> {
    let mut assigned = BTreeMap::new();
    for (path,bytes) in files {
        if !path.starts_with("Metadata/") || !matches!(path.rsplit('.').next(),Some("config"|"json"|"xml")) { continue }
        if bytes.len() > 8*1024*1024 { return Err("slicer metadata exceeds its bound".into()) }
        let Ok(source) = super::text(bytes) else { continue };
        if source.trim_start().starts_with('<') {
            let doc = xml::parse(source)?;
            for n in &doc.nodes {
                if n.name == "metadata" {
                    if let Some(key) = n.attr("key").or_else(|| n.attr("name")) { record(model,key,n.attr("value").unwrap_or(n.text.trim())) }
                }
            }
            if path.ends_with("model_settings.config") {
                for (index,i) in doc.nodes.iter().enumerate().filter(|(_,n)| n.name == "plate").map(|(i,_)| i).enumerate() {
                    if index >= 128 { return Err("too many build plates".into()) }
                    let info = metadata(&doc,i);
                    let name = info.get("plater_name").filter(|v| !v.is_empty()).cloned().unwrap_or_else(|| format!("Plate {}",index+1));
                    model.plates.push(Plate { name, metadata: info });
                    for instance in doc.children(i,"model_instance") {
                        let info = metadata(&doc,instance);
                        let object = info.get("object_id").ok_or("plate object id is missing")?.parse::<u32>().map_err(|_| "invalid plate object id")?;
                        let instance = info.get("instance_id").map(String::as_str).unwrap_or("0").parse::<u32>().map_err(|_| "invalid plate instance id")?;
                        if assigned.len() >= super::MAX_OBJECTS { return Err("too many plate instances".into()) }
                        if assigned.insert((object,instance),index as u32).is_some() { return Err("duplicate plate instance".into()) }
                    }
                }
            }
            if path.ends_with("slice_info.config") {
                for (index,i) in doc.nodes.iter().enumerate().filter(|(_,n)| n.name == "plate").map(|(i,_)| i).enumerate() {
                    let info = metadata(&doc,i);
                    if index >= 128 { return Err("too many slice plates".into()) }
                    if model.plates.len() <= index { model.plates.push(Plate { name: format!("Plate {}",index+1), metadata: BTreeMap::new() }) }
                    model.plates[index].metadata.extend(info);
                    let mut types = Vec::new();
                    let (mut weight,mut length) = (Some(0.),Some(0.));
                    let mut count = 0;
                    for f in doc.children(i,"filament") {
                        let n = &doc.nodes[f];
                        count += 1;
                        if let Some(t) = n.attr("type") { if !types.contains(&t) { types.push(t) } }
                        let quantity = |key| n.attr(key).and_then(|v| super::number(v).ok()).filter(|v| *v >= 0.);
                        weight = weight.zip(quantity("used_g")).map(|(a,b)| a+b);
                        length = length.zip(quantity("used_m")).map(|(a,b)| a+b*1000.);
                    }
                    let plate = &mut model.plates[index];
                    if !types.is_empty() { plate.metadata.insert("filament".into(),types.join(", ")); }
                    if count > 0 {
                        if let Some(weight) = weight { plate.metadata.entry("weight_g".into()).or_insert_with(|| weight.to_string()); }
                        if let Some(length) = length { plate.metadata.entry("filament_mm".into()).or_insert_with(|| length.to_string()); }
                    }
                }
            }
        } else if source.trim_start().starts_with('{') {
            let json = crate::jsondoc::parse(source)?;
            if let Some(entries) = json.as_object() {
                for (key,value) in entries {
                    let value = match value {
                        crate::jsondoc::Json::Str(s)|crate::jsondoc::Json::Num(s) => Some(s.clone()),
                        crate::jsondoc::Json::Arr(values) => Some(values.iter().filter_map(|v| v.as_str().map(str::to_string).or_else(|| v.as_f64().map(|v| v.to_string()))).collect::<Vec<_>>().join(", ")),
                        _ => None,
                    };
                    if let Some(value) = value { record(model,key,&value) }
                }
            }
        } else { pairs(model,source) }
    }
    Ok(Plates { instances: assigned })
}
