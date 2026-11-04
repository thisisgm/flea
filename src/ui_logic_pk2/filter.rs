//! Chip/hidden request fields; the backend applies both.
use super::request::FilterDef;
use crate::json::escape;

// The caller chip as the backend reads it.
pub fn filter_request(filter: Option<&FilterDef>) -> (Vec<String>, Vec<String>) {
    match filter {
        None => (Vec::new(), Vec::new()),
        Some(f) => (f.globs.clone(), f.mimes.clone()),
    }
}

fn string_array(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| format!("\"{}\"", escape(item))).collect();
    format!("[{}]", quoted.join(","))
}

// Picker list-request fields: no chip sends empty arrays.
pub fn list_fields(filter: Option<&FilterDef>, show_hidden: bool) -> String {
    let (globs, mimes) = filter_request(filter);
    format!(
        r#""pickerGlobs":{},"pickerMimes":{},"hidden":{}"#,
        string_array(&globs),
        string_array(&mimes),
        show_hidden
    )
}
