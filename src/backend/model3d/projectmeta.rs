use super::limits::*;

use super::{xml, Model, Result};
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

// Sample input: ; printer_model = MK4\n; model printing time: 1h; total estimated time: 1h 5m.
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

pub(super) fn metadata(doc: &xml::Document, node: usize) -> BTreeMap<String,String> {
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
    let mut sliced = std::collections::BTreeSet::new();
    let mut entries: Vec<_> = files.iter().collect();
    entries.sort_by_key(|(path,_)| {
        let path = path.to_ascii_lowercase();
        if path.ends_with("model_settings.config") { 0 } else if path.ends_with("slice_info.config") { 2 } else { 1 }
    });
    for (path,bytes) in entries {
        let lower = path.to_ascii_lowercase();
        if !lower.starts_with("metadata/") || !matches!(lower.rsplit('.').next(),Some("config"|"json"|"xml")) { continue }
        let identity = lower.ends_with("model_settings.config") || lower.ends_with("slice_info.config");
        if bytes.len() > MAX_METADATA_BYTES { return Err(format!("3MF metadata {path}: exceeds its byte bound")) }
        let source = match super::text(bytes) {
            Ok(source) => source,
            Err(reason) if identity => return Err(format!("3MF metadata {path}: {reason}")),
            Err(reason) => { eprintln!("flea: optional 3MF metadata {path}: {reason}"); continue },
        };
        if identity && !source.trim_start().starts_with('<') { return Err(format!("3MF metadata {path}: expected plate XML")) }
        if source.trim_start().starts_with('<') {
            let doc = match xml::parse(source) {
                Ok(doc) => doc,
                Err(reason) if identity || reason.contains("bound") => return Err(format!("3MF metadata {path}: {reason}")),
                Err(reason) => { eprintln!("flea: optional 3MF metadata {path}: {reason}"); continue },
            };
            for n in &doc.nodes {
                if n.name == "metadata" {
                    if let Some(key) = n.attr("key").or_else(|| n.attr("name")) { record(model,key,n.attr("value").unwrap_or(n.text.trim())) }
                }
            }
            super::projectplates::read(&doc,&lower,model,&mut assigned,&mut sliced).map_err(|reason| format!("3MF metadata {path}: {reason}"))?;
        } else if source.trim_start().starts_with('{') {
            let json = match crate::jsondoc::parse(source) {
                Ok(json) => json,
                Err(reason) if reason.starts_with("nested past ") => return Err(format!("3MF metadata {path}: {reason}")),
                Err(reason) => { eprintln!("flea: optional 3MF metadata {path}: {reason}"); continue },
            };
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
