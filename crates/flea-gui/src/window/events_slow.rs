//! A backend slow-operation notice becomes the status line's sentence.

// Sample input: {"t":"slow","op":"rename","msg":"The rename will finish on its own."}
pub(crate) fn slow_sentence(line: &str) -> Option<String> {
    if !line.contains("\"slow\"") {
        return None;
    }
    let document = flea::jsondoc::parse(line).ok()?;
    (document.get("t")?.as_str()? == "slow")
        .then(|| document.get("msg")?.as_str().map(str::to_owned))
        .flatten()
        .filter(|sentence| !sentence.is_empty())
}
