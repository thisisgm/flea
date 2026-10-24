// The backend's meta and compare replies as typed preview state; a malformed line is refused, never guessed.
use super::photo::{Photo, Ratio};
use crate::jsondoc::{self, Json};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Raw {
    pub file: String,
    pub w: u32,
    pub h: u32,
    pub orient: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meta {
    pub row: usize,
    pub token: Option<usize>,
    pub w: u32,
    pub h: u32,
    pub orient: u32,
    pub duration_ms: i64,
    pub sample_rate: f64,
    pub entries: Option<u64>,
    pub unpacked: u64,
    pub archive_failed: bool,
    pub names: Vec<Entry>,
    pub lines: Option<u64>,
    pub partial: bool,
    pub lines_failed: bool,
    pub frames: u32,
    pub photo: Option<Photo>,
    pub target: String,
    pub target_dir: bool,
    pub raw: Option<Raw>,
    pub owner: String,
}

fn number(doc: &Json, key: &str) -> f64 { doc.get(key).and_then(Json::as_f64).unwrap_or(0.0) }
fn whole(doc: &Json, key: &str) -> u32 { number(doc, key).max(0.0) as u32 }
fn text(doc: &Json, key: &str) -> String { doc.get(key).and_then(Json::as_str).unwrap_or("").into() }
fn flag(doc: &Json, key: &str) -> bool { doc.get(key).and_then(Json::as_bool).unwrap_or(false) }

fn ratio(doc: &Json, key: &str) -> Ratio {
    let pair = doc.get(key)?.as_array()?;
    match pair {
        [n, d] => Some((n.as_f64()?, d.as_f64()?)),
        _ => None,
    }
}

fn photo(doc: &Json) -> Option<Photo> {
    doc.as_object()?;
    Some(Photo {
        make: text(doc, "make"), model: text(doc, "model"), lens: text(doc, "lens"),
        exp: ratio(doc, "exp"), fnum: ratio(doc, "fnum"), iso: whole(doc, "iso"), focal: ratio(doc, "focal"),
        taken: text(doc, "taken"), w: whole(doc, "w"), h: whole(doc, "h"),
    })
}

// Sample input: src/backend/metareq.rs meta_reply; only a "t":"meta" object with a row is a meta reply.
pub fn parse(line: &str) -> Option<Meta> {
    let doc = jsondoc::parse(line).ok()?;
    if doc.get("t").and_then(Json::as_str) != Some("meta") {
        return None;
    }
    let row = doc.get("row").and_then(Json::as_f64).filter(|r| *r >= 0.0)? as usize;
    let names = doc.get("names").and_then(Json::as_array).unwrap_or(&[]).iter()
        .map(|e| Entry { name: text(e, "n"), is_dir: flag(e, "d") }).collect();
    let raw = doc.get("raw").filter(|r| r.as_object().is_some()).map(|r| Raw {
        file: text(r, "file"), w: whole(r, "w"), h: whole(r, "h"), orient: whole(r, "orient"),
    });
    Some(Meta {
        row,
        token: doc.get("token").and_then(Json::as_f64).map(|t| t as usize),
        w: whole(&doc, "w"), h: whole(&doc, "h"), orient: whole(&doc, "orient").max(1),
        duration_ms: number(&doc, "ms") as i64, sample_rate: number(&doc, "rate"),
        entries: doc.get("entries").and_then(Json::as_f64).map(|e| e as u64),
        unpacked: number(&doc, "unpacked") as u64, archive_failed: flag(&doc, "afailed"), names,
        lines: doc.get("lines").and_then(Json::as_f64).map(|l| l as u64),
        partial: flag(&doc, "partial"), lines_failed: flag(&doc, "lfailed"), frames: whole(&doc, "frames"),
        photo: doc.get("photo").and_then(photo), target: text(&doc, "target"), target_dir: flag(&doc, "targetdir"),
        raw, owner: text(&doc, "owner"),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompareState {
    Diff(String),
    Identical,
    Binary,
    Over,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareReply {
    pub token: usize,
    pub state: CompareState,
}

// Sample input: {"t":"compare","token":3,"state":"diff","patch":"--- /a\n..."}; an unknown state is a failure.
pub fn parse_compare(line: &str) -> Option<CompareReply> {
    let doc = jsondoc::parse(line).ok()?;
    if doc.get("t").and_then(Json::as_str) != Some("compare") {
        return None;
    }
    let token = doc.get("token").and_then(Json::as_f64)? as usize;
    let state = match doc.get("state").and_then(Json::as_str).unwrap_or("") {
        "diff" => CompareState::Diff(text(&doc, "patch")),
        "identical" => CompareState::Identical,
        "binary" => CompareState::Binary,
        "over" => CompareState::Over,
        _ => CompareState::Failed,
    };
    Some(CompareReply { token, state })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_meta_reply_reads_every_field_metareq_writes() {
        let line = r#"{"t":"meta","row":4,"w":0,"h":0,"orient":1,"ms":72000,"rate":44100,"entries":214,"unpacked":3400,"afailed":false,"names":[{"n":"ui","d":true}],"lines":9000,"partial":true,"lfailed":false,"frames":24,"photo":{"make":"Canon","exp":[1,250],"iso":400,"w":6000,"h":4000},"target":"/x","targetdir":true,"raw":{"file":"/c/r.jpg","w":1620,"h":1080,"orient":6},"owner":"gm","token":9}"#;
        let m = parse(line).expect("meta");
        assert_eq!((m.row, m.token, m.duration_ms, m.entries, m.lines, m.partial, m.frames), (4, Some(9), 72000, Some(214), Some(9000), true, 24));
        assert_eq!(m.names, vec![Entry { name: "ui".into(), is_dir: true }]);
        let p = m.photo.expect("photo");
        assert_eq!((p.make.as_str(), p.exp, p.iso, p.w, p.h), ("Canon", Some((1.0, 250.0)), 400, 6000, 4000));
        assert_eq!(m.raw.map(|r| (r.file, r.orient)), Some(("/c/r.jpg".into(), 6)));
        assert!(m.target_dir && m.owner == "gm");
    }

    #[test]
    fn malformed_and_foreign_lines_are_refused() {
        assert_eq!(parse("{\"t\":\"meta\""), None);
        assert_eq!(parse(r#"{"t":"rows","row":1}"#), None);
        assert_eq!(parse(r#"{"t":"meta"}"#), None);
        assert_eq!(parse(r#"{"t":"meta","row":-1}"#), None);
        assert!(parse(r#"{"t":"meta","row":2,"photo":null,"raw":null}"#).is_some_and(|m| m.photo.is_none() && m.raw.is_none()));
        assert_eq!(parse_compare(r#"{"t":"compare","state":"diff"}"#), None);
        assert_eq!(parse_compare("not json"), None);
        assert_eq!(parse_compare(r#"{"t":"compare","token":3,"state":"exploded"}"#).map(|r| r.state), Some(CompareState::Failed));
        assert_eq!(parse_compare(r#"{"t":"compare","token":3,"state":"diff","patch":"--- a\n"}"#).map(|r| r.state),
            Some(CompareState::Diff("--- a\n".into())));
    }
}
