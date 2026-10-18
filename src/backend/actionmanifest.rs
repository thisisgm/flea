// The action format is five unique keys with basic strings and string arrays, bounded before parsing.
use std::collections::HashMap;

pub(crate) const MAX_MANIFEST: usize = 16 * 1024;
const MAX_ARRAY_VALUES: usize = 128;

#[derive(Clone)]
pub(crate) struct Manifest {
    pub name: String,
    pub mime: Vec<String>,
    pub selection: String,
    pub run: Vec<String>,
    pub mode: String,
}

enum Value {
    Text(String),
    List(Vec<String>),
}

// Sample input: name="Inspect"\nmime=["text/*"]\nselection="one"\nrun=["cat","{path}"]\nmode="background"
pub(crate) fn parse(text: &str) -> Result<Manifest, String> {
    if text.len() > MAX_MANIFEST {
        return Err("manifest exceeds 16384 bytes".into());
    }
    let mut reader = Reader { text, at: 0 };
    let mut fields = HashMap::new();
    while reader.space() {
        let start = reader.at;
        while reader.byte().is_some_and(|b| b.is_ascii_alphabetic()) {
            reader.at += 1;
        }
        let key = &text[start..reader.at];
        if !["name", "mime", "selection", "run", "mode"].contains(&key) {
            return Err("unknown manifest key".into());
        }
        reader.horizontal();
        reader.expect(b'=')?;
        reader.horizontal();
        let value = if reader.byte() == Some(b'[') {
            reader.at += 1;
            let mut values = Vec::new();
            reader.space();
            while reader.byte() != Some(b']') {
                values.push(reader.string()?);
                if values.len() > MAX_ARRAY_VALUES {
                    return Err("too many array values".into());
                }
                reader.space();
                if reader.byte() == Some(b']') {
                    break;
                }
                reader.expect(b',')?;
                reader.space();
            }
            reader.expect(b']')?;
            Value::List(values)
        } else {
            Value::Text(reader.string()?)
        };
        if fields.insert(key, value).is_some() {
            return Err("duplicate manifest key".into());
        }
        reader.horizontal();
        if reader.byte() == Some(b'#') {
            reader.comment();
        }
        if reader.byte().is_some_and(|b| b != b'\n' && b != b'\r') {
            return Err("unexpected text after value".into());
        }
    }
    let mut string = |key| match fields.remove(key) {
        Some(Value::Text(value)) if !value.is_empty() => Ok(value),
        _ => Err(format!("{key} must be a nonempty string")),
    };
    let (name, selection, mode) = (string("name")?, string("selection")?, string("mode")?);
    if !["one", "many"].contains(&selection.as_str()) {
        return Err("selection must be one or many".into());
    }
    if !["background", "terminal", "preview"].contains(&mode.as_str()) {
        return Err("unknown action mode".into());
    }
    let mut list = |key| match fields.remove(key) {
        Some(Value::List(value))
            if !value.is_empty() && value.iter().all(|s| !s.is_empty() && !s.contains('\0')) =>
        {
            Ok(value)
        }
        _ => Err(format!("{key} must be a nonempty string array")),
    };
    let (mime, run) = (list("mime")?, list("run")?);
    if mime.iter().any(|p| {
        !p.contains('/')
            || p.chars().any(|c| c.is_whitespace() || c.is_control())
            || p.contains(['[', ']', '\\'])
    }) {
        return Err("MIME globs accept literal characters, * and ?".into());
    }
    if run[0].contains(['{', '}']) || run[0].starts_with('-') {
        return Err("the program must be literal".into());
    }
    for arg in &run {
        let mut rest = arg.as_str();
        while let Some(at) = rest.find(['{', '}']) {
            rest = &rest[at..];
            let Some(end) = rest.find('}') else {
                return Err("unclosed placeholder".into());
            };
            if !["{path}", "{dir}", "{name}", "{stem}", "{ext}"].contains(&&rest[..=end]) {
                return Err("unknown placeholder".into());
            }
            rest = &rest[end + 1..];
        }
    }
    Ok(Manifest {
        name,
        mime,
        selection,
        run,
        mode,
    })
}

struct Reader<'a> {
    text: &'a str,
    at: usize,
}
impl Reader<'_> {
    fn byte(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }
    fn horizontal(&mut self) {
        while matches!(self.byte(), Some(b' ' | b'\t')) {
            self.at += 1;
        }
    }
    fn comment(&mut self) {
        while self.byte().is_some_and(|b| b != b'\n') {
            self.at += 1;
        }
    }
    fn space(&mut self) -> bool {
        loop {
            while self
                .byte()
                .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
            {
                self.at += 1;
            }
            if self.byte() == Some(b'#') {
                self.comment();
            } else {
                return self.byte().is_some();
            }
        }
    }
    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.byte() != Some(byte) {
            return Err(format!("expected {}", byte as char));
        }
        self.at += 1;
        Ok(())
    }
    fn string(&mut self) -> Result<String, String> {
        let start = self.at;
        self.expect(b'"')?;
        while let Some(byte) = self.byte() {
            self.at += 1;
            match byte {
                b'\\' => {
                    self.at += usize::from(self.byte().is_some());
                }
                b'"' => {
                    return crate::jsondoc::parse(&self.text[start..self.at])
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_string))
                        .ok_or_else(|| "invalid string escape".into());
                }
                0..=8 | 10..=31 | 127 => return Err("control character in string".into()),
                _ => {}
            }
        }
        Err("unclosed string".into())
    }
}

// Linear wildcard matching with one remembered star, with no recursion or exponential backtracking.
pub(crate) fn matches(pattern: &str, value: &str) -> bool {
    let (p, v) = (pattern.as_bytes(), value.as_bytes());
    let (mut pi, mut vi, mut star, mut retry) = (0, 0, None, 0);
    while vi < v.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == v[vi]) {
            pi += 1;
            vi += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            pi += 1;
            retry = vi;
        } else if let Some(at) = star {
            retry += 1;
            vi = retry;
            pi = at + 1;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    const VALID: &str = r#"name = "Resize to 1600 px"
mime = ["image/*"]
selection = "many"
run = ["magick", "{path}", "-resize",
       "1600x1600>", "{dir}/{stem}-1600.{ext}"]
mode = "background"
"#;

    #[test]
    fn jobs_manifest_accepts_the_specimen_and_rejects_malformed_shapes_whole() {
        let manifest = parse(VALID).unwrap();
        assert_eq!(manifest.run.len(), 5);
        assert_eq!(manifest.name, "Resize to 1600 px");
        for malformed in [
            VALID.replace("many", "zero"),
            VALID.replace("background", "shell"),
            VALID.replace("{path}", "{unknown}"),
            VALID.replace("{path}", "{path"),
            VALID.replace("name =", "other ="),
            format!("{VALID}\nname=\"duplicate\""),
            VALID.replace("\nmime", " mime"),
            VALID.replace("\nmode", " trailing\nmode"),
            VALID.replace("[\"image/*\"]", "\"image/*\""),
            VALID.replace("\"magick\"", "\"{path}\""),
            VALID.replace("mode = \"background\"", ""),
            "x".repeat(MAX_MANIFEST + 1),
        ] {
            assert!(parse(&malformed).is_err(), "{malformed}");
        }
    }

    #[test]
    fn manifest_keys_require_separate_lines() {
        let text = VALID.replace("\nmime", " mime");
        assert_eq!(
            parse(&text).err().as_deref(),
            Some("unexpected text after value")
        );
    }

    #[test]
    fn jobs_mime_globs_match_the_entire_mime_with_bounded_wildcards() {
        for (pattern, mime, result) in [
            ("image/*", "image/jpeg", true),
            ("image/jp?g", "image/jpeg", true),
            ("text/*", "image/jpeg", false),
            ("image/jpeg", "image/jpeg-extra", false),
            ("*/*", "application/octet-stream", true),
        ] {
            assert_eq!(matches(pattern, mime), result);
        }
    }
}
