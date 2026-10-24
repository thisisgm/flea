use super::tags::known;
use crate::error::FleaError;

// Sample input: (["Custom", "Red"], "toggle", "Red") answers ["Custom"], the foreign name untouched.
pub fn apply(names: &[String], op: &str, tag: &str) -> Result<Vec<String>, FleaError> {
    if op == "clear" {
        return Ok(Vec::new());
    }
    if !matches!(op, "add" | "remove" | "toggle") {
        return Err(FleaError {
            where_: "settags".into(),
            path: op.into(),
            msg: "a tag change is add, remove, toggle or clear".into(),
        });
    }
    let tag = check_tag(tag)?;
    let present = names.iter().any(|name| name.eq_ignore_ascii_case(&tag));
    match op {
        "add" if present => Ok(names.to_vec()),
        "add" => {
            let mut out = names.to_vec();
            out.push(tag);
            Ok(out)
        }
        "remove" => Ok(names.iter().filter(|name| !name.eq_ignore_ascii_case(&tag)).cloned().collect()),
        _ if present => Ok(names.iter().filter(|name| !name.eq_ignore_ascii_case(&tag)).cloned().collect()),
        _ => {
            let mut out = names.to_vec();
            out.push(tag);
            Ok(out)
        }
    }
}

// Sample input: "red" answers "Red"; "Custom" answers "Custom"; "" and "a,b" refuse.
fn check_tag(tag: &str) -> Result<String, FleaError> {
    let trimmed = tag.trim();
    let refuse = |msg: &str| FleaError { where_: "settags".into(), path: tag.into(), msg: msg.into() };
    if trimmed.contains(',') {
        return Err(refuse("a tag cannot contain a comma"));
    }
    if trimmed.is_empty() || trimmed.contains('\0') || trimmed.chars().any(|c| c.is_control()) {
        return Err(refuse("a tag name cannot be empty or hold control characters"));
    }
    Ok(known(trimmed).map(str::to_string).unwrap_or_else(|| trimmed.to_string()))
}
