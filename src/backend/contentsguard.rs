use crate::jsondoc::{self, Json};
use std::collections::HashSet;

// Sample input: {"c":"paths","rows":[0]}; reject duplicate decoded keys before first-match handlers read them.
pub fn document(line: &str) -> Result<Json, String> {
    let document = jsondoc::parse(line)?;
    let bytes = line.as_bytes();
    let mut at = 0;
    let mut objects: Vec<HashSet<String>> = Vec::new();
    while let Some(byte) = bytes.get(at) {
        match byte {
            b'{' => {
                objects.push(HashSet::new());
                at += 1;
            }
            b'}' => {
                objects.pop();
                at += 1;
            }
            b'"' => {
                let text = crate::jsonstring::parse_string(bytes, &mut at)?;
                let mut next = at;
                while bytes.get(next).is_some_and(u8::is_ascii_whitespace) {
                    next += 1;
                }
                if bytes.get(next) == Some(&b':')
                    && !objects.last_mut().ok_or("missing object")?.insert(text)
                {
                    return Err("duplicate request key".into());
                }
            }
            _ => at += 1,
        }
    }
    Ok(document)
}
