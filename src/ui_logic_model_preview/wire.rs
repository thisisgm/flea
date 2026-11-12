//! The `model3d` request lines a 3D preview sends and the typed replies it applies, token checked.
use crate::json::escape;
use crate::jsondoc::{self, Json};

/// The model's facts as the backend measured them; an unknown number is `None`, never zero.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub surface_cm2: Option<f64>,
    pub triangles: Option<u64>,
    pub objects: Option<u64>,
    pub plates: Option<u64>,
    pub open_edges: Option<u64>,
}

/// The slicer's normalized facts; text values are the file's own content.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Slicer {
    pub printer: Option<String>,
    pub filament: Option<String>,
    pub nozzle_mm: Vec<f64>,
    pub layer_height_mm: Option<f64>,
    pub print_time_s: Option<f64>,
    pub filament_mm: Option<f64>,
    pub weight_g: Option<f64>,
}

/// One plate's own facts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plate {
    pub name: Option<String>,
    pub facts: Facts,
    pub slicer: Slicer,
}

/// A ready reply's `data`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Data {
    pub format: String,
    pub facts: Facts,
    pub slicer: Slicer,
    pub layers: u64,
    pub segments: u64,
    pub metadata: Vec<(String, String)>,
    pub plates: Vec<Plate>,
}

/// A refusal category, independent of diagnostic wording; absent on older backends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    TooLarge,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Failure {
    pub reason: String,
    pub refusal: Option<Refusal>,
}

impl From<String> for Failure {
    fn from(reason: String) -> Self {
        Self {
            reason,
            refusal: None,
        }
    }
}

impl From<&str> for Failure {
    fn from(reason: &str) -> Self {
        reason.to_string().into()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    /// `file` is the cached model, `thumbnail` the embedded image or empty when the file has none.
    Ready {
        file: String,
        thumbnail: String,
        data: Box<Data>,
    },
    /// Diagnostic wording never selects the user-facing sentence.
    Failed(Failure),
    Cancelled,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub slot: String,
    pub token: u64,
    pub state: State,
}

/// The request for `path` in `slot`; a newer request on the same slot cancels the older one, sample output {"c":"model3d","slot":"look","token":7,"path":"/p/kit.3mf","plate":0}
pub fn request(slot: &str, token: u64, path: &str, plate: Option<usize>) -> String {
    let plate = plate.map_or(String::new(), |p| format!(",\"plate\":{p}"));
    format!(
        "{{\"c\":\"model3d\",\"slot\":\"{}\",\"token\":{token},\"path\":\"{}\"{plate}}}",
        escape(slot),
        escape(path)
    )
}

/// The explicit cancel a closing preview sends.
pub fn cancel(slot: &str, token: u64) -> String {
    format!(
        "{{\"c\":\"model3d\",\"slot\":\"{}\",\"token\":{token},\"cancel\":true}}",
        escape(slot)
    )
}

/// Parses one reply line; anything that is not a well-formed `model3d` reply is refused with a reason.
pub fn parse(line: &str) -> Result<Reply, String> {
    let doc = jsondoc::parse(line)?;
    if doc.get("t").and_then(Json::as_str) != Some("model3d") {
        return Err("not a model3d reply".into());
    }
    let slot = doc
        .get("slot")
        .and_then(Json::as_str)
        .ok_or("model3d reply without a slot")?
        .to_string();
    let token = doc
        .get("token")
        .and_then(uint)
        .ok_or("model3d reply without a token")?;
    let state = match doc.get("state").and_then(Json::as_str) {
        Some("ready") => {
            let file = doc
                .get("file")
                .and_then(Json::as_str)
                .filter(|f| f.starts_with('/'))
                .ok_or("ready reply without a model file")?;
            let data = doc
                .get("data")
                .filter(|d| d.as_object().is_some())
                .ok_or("ready reply without data")?;
            State::Ready {
                file: file.into(),
                thumbnail: doc
                    .get("thumbnail")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .into(),
                data: Box::new(parse_data(data)),
            }
        }
        Some("failed") => State::Failed(Failure {
            reason: doc
                .get("reason")
                .and_then(Json::as_str)
                .unwrap_or("")
                .into(),
            refusal: match doc.get("refusal").and_then(Json::as_str) {
                Some("too_large") => Some(Refusal::TooLarge),
                _ => None,
            },
        }),
        Some("cancelled") => State::Cancelled,
        _ => return Err("model3d reply with an unknown state".into()),
    };
    Ok(Reply { slot, token, state })
}

fn uint(v: &Json) -> Option<u64> {
    let n = v.as_f64()?;
    (n >= 0. && n.fract() == 0. && n <= u64::MAX as f64).then_some(n as u64)
}

fn num(v: Option<&Json>) -> Option<f64> {
    v.and_then(Json::as_f64).filter(|n| n.is_finite())
}

fn text(v: Option<&Json>) -> Option<String> {
    v.and_then(Json::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}

fn parse_facts(v: Option<&Json>) -> Facts {
    let Some(v) = v else { return Facts::default() };
    let dims = v
        .get("dimensions_mm")
        .and_then(Json::as_array)
        .and_then(|a| {
            let n: Vec<f64> = a.iter().filter_map(|x| num(Some(x))).collect();
            (n.len() == 3).then(|| [n[0], n[1], n[2]])
        });
    Facts {
        dimensions_mm: dims,
        volume_cm3: num(v.get("volume_cm3")),
        surface_cm2: num(v.get("surface_cm2")),
        triangles: v.get("triangles").and_then(uint),
        objects: v.get("objects").and_then(uint),
        plates: v.get("plates").and_then(uint),
        open_edges: v.get("open_edges").and_then(uint),
    }
}

fn parse_slicer(v: Option<&Json>) -> Slicer {
    let Some(v) = v else { return Slicer::default() };
    Slicer {
        printer: text(v.get("printer")),
        filament: text(v.get("filament")),
        nozzle_mm: v
            .get("nozzle_mm")
            .and_then(Json::as_array)
            .map_or(Vec::new(), |a| {
                a.iter().filter_map(|x| num(Some(x))).collect()
            }),
        layer_height_mm: num(v.get("layer_height_mm")),
        print_time_s: num(v.get("print_time_s")),
        filament_mm: num(v.get("filament_mm")),
        weight_g: num(v.get("weight_g")),
    }
}

fn parse_data(v: &Json) -> Data {
    let metadata = v
        .get("metadata")
        .and_then(Json::as_object)
        .map_or(Vec::new(), |pairs| {
            pairs
                .iter()
                .filter_map(|(k, val)| val.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        });
    let plates = v
        .get("plates")
        .and_then(Json::as_array)
        .map_or(Vec::new(), |a| {
            a.iter()
                .map(|p| Plate {
                    name: text(p.get("name")),
                    facts: parse_facts(p.get("facts")),
                    slicer: parse_slicer(p.get("slicer")),
                })
                .collect()
        });
    Data {
        format: v.get("format").and_then(Json::as_str).unwrap_or("").into(),
        facts: parse_facts(v.get("facts")),
        slicer: parse_slicer(v.get("slicer")),
        layers: v.get("layers").and_then(uint).unwrap_or(0),
        segments: v.get("segments").and_then(uint).unwrap_or(0),
        metadata,
        plates,
    }
}
