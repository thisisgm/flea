use crate::jsondoc::{self, Json};
use std::collections::HashMap;

pub(super) const MAX_BYTES: usize = 1024 * 1024;
pub(super) const MAX_ENTRIES: usize = 16_384;

// A single forward cursor and hashed object insertion keep wide and duplicate-key inputs linear.
// Sample input: {"label":"Open %1","count":{"one":"%1 item","other":"%1 items"}}.
pub(super) fn parse(text: &str) -> Result<Json, String> {
    if text.len() > MAX_BYTES {
        return Err(format!("document exceeds {MAX_BYTES} bytes"));
    }
    let mut reader = Reader { text, at: 0, entries: 0 };
    reader.space();
    let value = reader.value(0)?;
    reader.space();
    if reader.at != text.len() {
        return Err(reader.error("trailing text"));
    }
    Ok(value)
}

struct Reader<'a> {
    text: &'a str,
    at: usize,
    entries: usize,
}

impl Reader<'_> {
    fn error(&self, message: &str) -> String {
        format!("{message} at byte {}", self.at)
    }

    fn peek(&self) -> Option<u8> { self.text.as_bytes().get(self.at).copied() }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) { self.at += 1; }
    }

    fn consume(&mut self, byte: u8) -> bool {
        if self.peek() != Some(byte) { return false; }
        self.at += 1;
        true
    }

    fn entry(&mut self) -> Result<(), String> {
        if self.entries == MAX_ENTRIES {
            return Err(self.error(&format!("document exceeds {MAX_ENTRIES} entries")));
        }
        self.entries += 1;
        Ok(())
    }

    // Sample input: {"key":"value"}, [1,true], "text", null or -1.5e2 is one JSON value.
    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth >= jsondoc::MAX_DEPTH {
            return Err(self.error("document exceeds nesting limit"));
        }
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Json::Str),
            Some(b't' | b'f' | b'n' | b'-' | b'0'..=b'9') => {
                let start = self.at;
                while let Some(byte) = self.peek() {
                    if matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b',' | b'}' | b']') { break; }
                    self.at += 1;
                }
                // Only one scalar reaches jsondoc, never an object or an unconsumed input suffix.
                jsondoc::parse(&self.text[start..self.at])
            }
            _ => Err(self.error("expected a JSON value")),
        }
    }

    // Sample input: "Open \"%1\"" includes JSON quotes and escaped interior quotes.
    fn string(&mut self) -> Result<String, String> {
        let start = self.at;
        if !self.consume(b'"') { return Err(self.error("expected a string")); }
        let mut escaped = false;
        while let Some(byte) = self.peek() {
            self.at += 1;
            match byte {
                b'"' => {
                    if escaped {
                        let mut cursor = start;
                        return crate::jsonstring::parse_string(self.text.as_bytes(), &mut cursor);
                    }
                    return Ok(self.text[start + 1..self.at - 1].to_owned());
                }
                b'\\' => {
                    escaped = true;
                    if self.peek().is_none() { break; }
                    self.at += 1;
                }
                0..=31 => return Err(self.error("unescaped control character")),
                _ => {}
            }
        }
        Err(self.error("unterminated string"))
    }

    // Sample input: {"name":"Flea","forms":{"other":"%1 items"}} is a JSON object.
    fn object(&mut self, depth: usize) -> Result<Json, String> {
        self.at += 1;
        self.space();
        let mut values = HashMap::new();
        if self.consume(b'}') { return Ok(Json::Obj(Vec::new())); }
        loop {
            // Count every member, including duplicates and members inside nested objects.
            self.entry()?;
            let key = self.string()?;
            self.space();
            if !self.consume(b':') { return Err(self.error("expected a colon")); }
            self.space();
            values.insert(key, self.value(depth + 1)?);
            self.space();
            if self.consume(b'}') { return Ok(Json::Obj(values.into_iter().collect())); }
            if !self.consume(b',') { return Err(self.error("expected a comma or closing brace")); }
            self.space();
        }
    }

    // Sample input: [null,true,12,"text",{"key":"value"}] is an ordered JSON array.
    fn array(&mut self, depth: usize) -> Result<Json, String> {
        self.at += 1;
        self.space();
        let mut values = Vec::new();
        if self.consume(b']') { return Ok(Json::Arr(values)); }
        loop {
            self.entry()?;
            values.push(self.value(depth + 1)?);
            self.space();
            if self.consume(b']') { return Ok(Json::Arr(values)); }
            if !self.consume(b',') { return Err(self.error("expected a comma or closing bracket")); }
            self.space();
        }
    }
}
