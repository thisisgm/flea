//! ASCII punctuation written as numeric references, so no emphasis, link or autolink forms inside converted text.

/// The span a code chip pads with on each side; the renderer reads it as a fixed gap, and user text never holds it.
pub const CHIP_PAD_MARK: &str = "<span style=\"font-size:chippad\">";
/// The whole pad: the mark, one no-break space and the span's close.
pub const CHIP_PAD: &str = "<span style=\"font-size:chippad\">&nbsp;</span>";

/// True for the ASCII punctuation CommonMark lets a backslash escape.
pub fn is_ascii_punct(code: u32) -> bool {
    matches!(code, 33..=47 | 58..=64 | 91..=96 | 123..=126)
}

/// The text with every ASCII punctuation character written as its numeric reference.
pub fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    push_escaped(&mut out, text);
    out
}

/// Appends the escaped text to out.
pub fn push_escaped(out: &mut String, text: &str) {
    for c in text.chars() {
        if is_ascii_punct(c as u32) {
            push_reference(out, c as u32);
        } else {
            out.push(c);
        }
    }
}

/// Appends `&#N;` for one code.
pub fn push_reference(out: &mut String, code: u32) {
    out.push_str("&#");
    out.push_str(&code.to_string());
    out.push(';');
}
