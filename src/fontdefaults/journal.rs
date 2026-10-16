use super::settings::Setting;
use crate::jsondoc::Json;
use std::path::{Path, PathBuf};

pub struct Entry { pub path: PathBuf, pub original: Option<String>, pub applied: String, pub mode: u32 }
pub struct Journal { pub family: String, pub files: Vec<Entry>, pub setting: Option<Setting> }
fn field<'a>(value: &'a Json, name: &str) -> Result<&'a Json, String> { value.get(name).ok_or("Invalid font preference journal.".into()) }
fn string(value: &Json) -> Result<String, String> { value.as_str().map(str::to_string).ok_or("Invalid font preference journal.".into()) }
fn optional(value: &Json) -> Result<Option<String>, String> { if value == &Json::Null { Ok(None) } else { string(value).map(Some) } }
fn opt_json(value: &Option<String>) -> Json { value.as_ref().map(|s| Json::Str(s.clone())).unwrap_or(Json::Null) }
impl Journal {
    pub fn read(path: &Path) -> Result<Option<Self>, String> {
        // Three backed-up preference files can exceed the per-file 1 MiB input bound.
        let Some(text) = read_state(path)? else { return Ok(None); };
        let value = crate::jsondoc::parse(&text)?;
        let mut files = Vec::new();
        for entry in field(&value, "files")?.as_array().ok_or("Invalid font preference journal.")? {
            let path = PathBuf::from(string(field(entry, "path")?)?);
            if !path.is_absolute() || path.starts_with("/usr") || path.starts_with("/etc") { return Err("Invalid preference backup location.".into()); }
            let mode = field(entry, "mode")?.as_f64().filter(|n| n.fract() == 0.0 && (0.0..=4095.0).contains(n)).ok_or("Invalid preference permissions.")? as u32;
            if files.iter().any(|e: &Entry| e.path == path) || files.len() >= 3 { return Err("Invalid preference backup files.".into()); }
            files.push(Entry { path, original: optional(field(entry, "original")?)?, applied: string(field(entry, "applied")?)?, mode });
        }
        if files.is_empty() { return Err("The preference backup is empty.".into()); }
        let setting = Setting::parse(value.get("setting").unwrap_or(&Json::Null))?;
        Ok(Some(Self { family: string(field(&value, "family")?)?, files, setting }))
    }
    pub fn json(&self) -> String {
        let files = self.files.iter().map(|entry| Json::Obj(vec![
            ("path".into(), Json::Str(entry.path.to_string_lossy().into_owned())), ("original".into(), opt_json(&entry.original)),
            ("applied".into(), Json::Str(entry.applied.clone())), ("mode".into(), Json::Num(entry.mode.to_string()))])).collect();
        crate::jsondoc::render(&Json::Obj(vec![("family".into(), Json::Str(self.family.clone())), ("files".into(), Json::Arr(files)),
            ("setting".into(), self.setting.as_ref().map(Setting::json).unwrap_or(Json::Null))]))
    }
}
fn read_state(path: &Path) -> Result<Option<String>, String> {
    use std::fs::OpenOptions;
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = match OpenOptions::new().read(true).custom_flags(0x800 | 0x20000).open(path) {
        Ok(file) => file, Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None), Err(e) => return Err(e.to_string()),
    };
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > 16 * 1024 * 1024 { return Err("Invalid font preference journal size.".into()); }
    let mut text = String::new();
    file.take(16 * 1024 * 1024 + 1).read_to_string(&mut text).map_err(|e| e.to_string())?;
    if text.len() > 16 * 1024 * 1024 { return Err("Font preference journal exceeds 16 MiB.".into()); }
    Ok(Some(text))
}
