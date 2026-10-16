use super::files;
use crate::jsondoc::Json;
use std::process::{Command, Stdio};

#[derive(Clone)]
pub struct Setting { pub original: String, pub applied: String }
pub fn call(action: &str, value: Option<&str>) -> Result<String, String> {
    let mut command = Command::new("timeout");
    command.args(["3", "gsettings", action, "org.gnome.desktop.interface", "font-name"]);
    if let Some(value) = value { command.arg(value); }
    let output = command.stdin(Stdio::null()).output().map_err(|e| e.to_string())?;
    if !output.status.success() { return Err(format!("The desktop font preference could not be updated: {}", String::from_utf8_lossy(&output.stderr).trim())); }
    String::from_utf8(output.stdout).map(|s| s.trim().to_string()).map_err(|e| e.to_string())
}
pub fn prepare(family: &str) -> Option<Setting> {
    if !files::executable("gsettings") || call("writable", None).ok()?.as_str() != "true" { return None; }
    let original = call("get", None).ok()?;
    let text = decode(&original).ok()?;
    Some(Setting { original, applied: quote(&format!("{} {}", family, size(&text))) })
}
pub fn size(text: &str) -> &str {
    text.split_whitespace().last().filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit() || b == b'.') && s.parse::<f64>().is_ok()).unwrap_or("11")
}
pub fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}
// GVariant string syntax returned by gsettings; also accepts JSON strings in older journals.
pub fn decode(text: &str) -> Result<String, String> {
    if text.starts_with('"') {
        return crate::jsondoc::parse(text)?.as_str().map(str::to_string).ok_or("Invalid desktop font preference.".into());
    }
    let inner = text.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')).ok_or("Invalid desktop font preference.")?;
    let mut chars = inner.chars();
    let mut out = String::new();
    while let Some(ch) = chars.next() {
        out.push(if ch == '\\' { match chars.next().ok_or("Invalid desktop font escape.")? {
            'n' => '\n', 'r' => '\r', 't' => '\t', '\'' => '\'', '"' => '"', '\\' => '\\', _ => return Err("Unsupported desktop font escape.".into()),
        }} else { ch });
    }
    Ok(out)
}
impl Setting {
    pub fn json(&self) -> Json { Json::Obj(vec![("original".into(), Json::Str(self.original.clone())), ("applied".into(), Json::Str(self.applied.clone()))]) }
    pub fn parse(value: &Json) -> Result<Option<Self>, String> {
        if value == &Json::Null { return Ok(None); }
        let field = |name| value.get(name).and_then(Json::as_str).map(str::to_string).ok_or_else(|| "Invalid font preference journal.".to_string());
        Ok(Some(Self { original: field("original")?, applied: field("applied")? }))
    }
}
