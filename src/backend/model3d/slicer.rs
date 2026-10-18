use super::Model;
use super::limits::MAX_SLICER_QUANTITY;

#[derive(Clone,Debug,Default,PartialEq)]
pub struct SlicerFacts {
    pub printer: Option<String>,
    pub filament: Option<String>,
    pub nozzle_mm: Vec<f64>,
    pub layer_height_mm: Option<f64>,
    pub print_time_s: Option<f64>,
    pub filament_mm: Option<f64>,
    pub weight_g: Option<f64>,
}

fn positive(value: &str) -> Option<f64> {
    let value = value.trim().parse::<f64>().ok()?;
    (value.is_finite() && (0. ..=MAX_SLICER_QUANTITY).contains(&value)).then_some(value)
}

// Sample input: "0.4, 0.6mm" or "[0.4;0.6]".
fn values(value: &str) -> Vec<f64> {
    value.trim_matches(['[',']','"']).split([',',';']).map(|v| {
        let v = v.trim().trim_matches('"');
        let end = v.find(|c: char| !c.is_ascii_digit() && !matches!(c,'.'|'+'|'-')).unwrap_or(v.len());
        if !matches!(v[end..].trim(),""|"mm"|"m"|"g"|"s") { return None }
        positive(&v[..end])
    }).collect::<Option<Vec<_>>>().unwrap_or_default()
}

fn sum(value: &str) -> Option<f64> {
    let values = values(value);
    (!values.is_empty()).then(|| values.iter().sum())
}

// Sample input: "123.5", "01:02:03", or "1 day 2 h 3 min 4 s"; unknown text stays unknown.
fn seconds(value: &str) -> Option<f64> {
    let value = value.trim().trim_matches('"');
    if let Some(n) = positive(value) { return Some(n) }
    if value.contains(':') {
        let parts = value.split(':').map(positive).collect::<Option<Vec<_>>>()?;
        if !(2..=3).contains(&parts.len()) || parts[1..].iter().any(|v| *v >= 60.) { return None }
        return Some(parts.iter().fold(0.,|total,n| total*60.+n));
    }
    let bytes = value.as_bytes();
    let (mut at,mut total,mut count) = (0,0.,0);
    while at < bytes.len() {
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) { at += 1 }
        if at == bytes.len() { break }
        let first = at;
        while bytes.get(at).is_some_and(|b| b.is_ascii_digit() || *b == b'.') { at += 1 }
        let n = positive(&value[first..at])?;
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) { at += 1 }
        let first = at;
        while bytes.get(at).is_some_and(u8::is_ascii_alphabetic) { at += 1 }
        let factor = match &value[first..at] {
            "d"|"day"|"days" => 86400., "h"|"hour"|"hours" => 3600.,
            "m"|"min"|"minute"|"minutes" => 60., "s"|"sec"|"second"|"seconds" => 1., _ => return None,
        };
        total += n*factor;
        count += 1;
        if total > MAX_SLICER_QUANTITY { return None }
    }
    (count > 0).then_some(total)
}

pub(super) fn facts(model: &Model, plate: Option<usize>) -> SlicerFacts {
    let lookup = |key: &str| plate.and_then(|i| model.plates.get(i)).and_then(|p| p.metadata.get(key)).or_else(|| model.metadata.get(key));
    let text = |key: &str| lookup(key).map(|v| v.trim().trim_matches('"').to_string()).filter(|v| !v.is_empty());
    let mut facts = SlicerFacts { printer: text("printer"), filament: text("filament"),
        nozzle_mm: lookup("nozzle_mm").map(|v| values(v)).unwrap_or_default(),
        layer_height_mm: lookup("layer_height_mm").and_then(|v| values(v).first().copied()),
        print_time_s: lookup("print_time").and_then(|v| seconds(v)),
        filament_mm: lookup("filament_mm").and_then(|v| sum(v)), weight_g: lookup("weight_g").and_then(|v| sum(v)) };
    if facts.filament_mm.is_none() {
        if let Some(value) = lookup("filament_m") { facts.filament_mm = sum(value).map(|v| v*1000.) }
        else if let Some(value) = lookup("filament_length") {
            let scale = if value.trim_end().ends_with("mm") { 1. } else if value.trim_end().ends_with('m') { 1000. } else { return facts };
            facts.filament_mm = sum(value).map(|v| v*scale);
        }
    }
    facts
}

impl SlicerFacts {
    pub(super) fn json(&self) -> String {
        let string = |v: &Option<String>| v.as_ref().map(|s| format!(r#""{}""#,crate::json::escape(s))).unwrap_or_else(|| "null".into());
        let number = |v: Option<f64>| v.map(|v| v.to_string()).unwrap_or_else(|| "null".into());
        format!(r#"{{"printer":{},"filament":{},"nozzle_mm":[{}],"layer_height_mm":{},"print_time_s":{},"filament_mm":{},"weight_g":{}}}"#,
            string(&self.printer),string(&self.filament),self.nozzle_mm.iter().map(f64::to_string).collect::<Vec<_>>().join(","),
            number(self.layer_height_mm),number(self.print_time_s),number(self.filament_mm),number(self.weight_g))
    }
}

pub(super) fn totals(model: &mut Model) {
    if model.plates.is_empty() { return }
    let plates: Vec<SlicerFacts> = model.plates.iter().map(|plate| {
        let local = Model { metadata: plate.metadata.clone(), ..Model::default() };
        local.slicer(None)
    }).collect();
    for (key,which) in [("print_time",0),("weight_g",1),("filament_mm",2)] {
        let values: Option<Vec<f64>> = plates.iter().map(|facts| match which { 0 => facts.print_time_s, 1 => facts.weight_g, _ => facts.filament_mm }).collect();
        if let Some(values) = values.filter(|v| !v.is_empty()) { model.metadata(key,&values.iter().sum::<f64>().to_string()) }
        else { model.metadata.remove(key); }
    }
    let mut filaments = Vec::new();
    for plate in plates {
        if let Some(filament) = plate.filament { if !filaments.contains(&filament) { filaments.push(filament) } }
    }
    if !filaments.is_empty() { model.metadata("filament",&filaments.join(", ")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_are_shared_by_both_frontends() {
        for (text,time) in [("1h 2m 3s",3723.),("2 h 08 min",7680.),("01:02:03",3723.),("08:30",510.),("123.5",123.5),("1 day 2 hours",93600.)] {
            assert_eq!(seconds(text),Some(time));
        }
        for text in ["unknown","-1","1e999","12:90:00","1 week",""] { assert_eq!(seconds(text),None) }
    }

    #[test]
    fn slicer_units_and_plate_overrides_are_normalized_without_losing_raw_metadata() {
        let mut model = Model::default();
        for (k,v) in [("printer","P1S"),("filament_length","1.23m"),("weight_g","2.5, 3.5"),("nozzle_mm","0.4, 0.6"),("print_time","2 h 08 min")] { model.metadata(k,v) }
        model.plates.push(super::super::Plate { name: "Plate 1".into(), metadata: [("print_time".into(),"100".into())].into() });
        let facts = model.slicer(None);
        assert_eq!(facts.filament_mm,Some(1230.));
        assert_eq!(facts.weight_g,Some(6.));
        assert_eq!(facts.nozzle_mm,[0.4,0.6]);
        assert_eq!(facts.print_time_s,Some(7680.));
        assert_eq!(model.slicer(Some(0)).print_time_s,Some(100.));
    }

    #[test]
    fn all_plates_and_filaments_contribute_to_project_totals() {
        let source = b"<config><plate><metadata key='prediction' value='60'/><filament type='PLA' used_g='2' used_m='1'/><filament type='PETG' used_g='3' used_m='2'/></plate><plate><metadata key='prediction' value='120'/><filament type='PLA' used_g='4' used_m='3'/></plate></config>";
        let files = [("Metadata/slice_info.config".into(),source.to_vec())].into();
        let mut model = Model::default();
        super::super::projectmeta::read(&files,&mut model).unwrap();
        totals(&mut model);
        assert_eq!(model.slicer(None).print_time_s,Some(180.));
        assert_eq!(model.slicer(None).weight_g,Some(9.));
        assert_eq!(model.slicer(None).filament_mm,Some(6000.));
        assert_eq!(model.slicer(Some(0)).weight_g,Some(5.));
        assert_eq!(model.slicer(Some(1)).print_time_s,Some(120.));
        model.plates[1].metadata.remove("print_time");
        totals(&mut model);
        assert_eq!(model.slicer(None).print_time_s,None);
    }
}
