use crate::jsonstring::parse_string;

const MAX_BYTES: usize = 1024 * 1024;
const MAX_STRING: usize = 8192;
const MAX_MEMBERS: usize = 100_000;
const MAX_TOKENS: usize = 524_288;
const MAX_DEPTH: usize = 32;
const MAX_NUMBER: usize = 128;
const CHECK_INTERVAL: usize = 1024;
pub(super) const INTERRUPTED: &str = "Tailscale status decode was cancelled or timed out.";
const INVALID: &str = "Tailscale status could not be read.";
const LIMITED: &str = "Tailscale status exceeded its decode limits.";

// Visit direct members without building ignored objects or scanning preceding keys for duplicates.
// Sample input: {"BackendState":"Running","Peer":{}}
pub(super) fn object<'a>(input: &'a str, keep_going: &mut dyn FnMut() -> bool,
    mut visit: impl FnMut(String, &'a str, &mut dyn FnMut() -> bool) -> Result<(), &'static str>,
) -> Result<(), &'static str> {
    if input.len() > MAX_BYTES { return Err(LIMITED); }
    let mut reader = Reader { input, at: 0, tokens: 0, keep_going };
    reader.check()?;
    reader.expect(b'{')?;
    if !reader.take(b'}') {
        let mut count = 0;
        loop {
            reader.check()?;
            count += 1;
            if count > MAX_MEMBERS { return Err(LIMITED); }
            let key = reader.string()?;
            reader.expect(b':')?;
            reader.space();
            let start = reader.at;
            reader.value(1)?;
            visit(key, &input[start..reader.at], reader.keep_going)?;
            if reader.take(b'}') { break; }
            reader.expect(b',')?;
        }
    }
    reader.space();
    if reader.at != input.len() { return Err(INVALID); }
    reader.check()
}

// Sample input: "nas.tail.ts.net."
pub(super) fn text(input: &str) -> Option<String> {
    parse_string(input.as_bytes(), &mut 0).ok()
}

struct Reader<'a, 'p> {
    input: &'a str,
    at: usize,
    tokens: usize,
    keep_going: &'p mut dyn FnMut() -> bool,
}

impl Reader<'_, '_> {
    fn check(&mut self) -> Result<(), &'static str> {
        if !(self.keep_going)() { return Err(INTERRUPTED); }
        self.tokens += 1;
        if self.tokens > MAX_TOKENS { return Err(LIMITED); }
        Ok(())
    }

    fn space(&mut self) {
        while self.input.as_bytes().get(self.at).is_some_and(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n')) { self.at += 1; }
    }

    fn take(&mut self, byte: u8) -> bool {
        self.space();
        if self.input.as_bytes().get(self.at) == Some(&byte) { self.at += 1; true } else { false }
    }

    fn expect(&mut self, byte: u8) -> Result<(), &'static str> {
        if self.take(byte) { Ok(()) } else { Err(INVALID) }
    }

    // Sample input: "caf\u00e9"
    fn string(&mut self) -> Result<String, &'static str> {
        self.check()?;
        self.space();
        let start = self.at;
        if self.input.as_bytes().get(self.at) != Some(&b'"') { return Err(INVALID); }
        self.at += 1;
        while let Some(&byte) = self.input.as_bytes().get(self.at) {
            if self.at - start > MAX_STRING { return Err(LIMITED); }
            if (self.at - start).is_multiple_of(CHECK_INTERVAL) { self.check()?; }
            self.at += 1;
            if byte == b'\\' { self.at += 1; }
            else if byte == b'"' {
                let mut at = 0;
                return parse_string(&self.input.as_bytes()[start..self.at], &mut at).map_err(|_| INVALID);
            }
        }
        Err(INVALID)
    }

    // Sample input: [true,{"Online":false},1.25e2]
    fn value(&mut self, depth: usize) -> Result<(), &'static str> {
        self.check()?;
        if depth >= MAX_DEPTH { return Err(LIMITED); }
        self.space();
        match self.input.as_bytes().get(self.at) {
            Some(b'"') => { self.string()?; }
            Some(b'{') => self.container(depth, true)?,
            Some(b'[') => self.container(depth, false)?,
            Some(b't') => self.literal("true")?,
            Some(b'f') => self.literal("false")?,
            Some(b'n') => self.literal("null")?,
            Some(b'-' | b'0'..=b'9') => {
                let start = self.at;
                while self.input.as_bytes().get(self.at).is_some_and(|b| matches!(b, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')) {
                    self.at += 1;
                    if self.at - start > MAX_NUMBER { return Err(LIMITED); }
                }
                crate::jsondoc::parse(&self.input[start..self.at]).map_err(|_| INVALID)?;
            }
            _ => return Err(INVALID),
        }
        Ok(())
    }

    // Sample input: {"Online":true,"DNSName":"nas.tail.ts.net."}
    fn container(&mut self, depth: usize, object: bool) -> Result<(), &'static str> {
        self.at += 1;
        let end = if object { b'}' } else { b']' };
        if self.take(end) { return Ok(()); }
        let mut members = 0;
        loop {
            members += 1;
            if members > MAX_MEMBERS { return Err(LIMITED); }
            if object { self.string()?; self.expect(b':')?; }
            self.value(depth + 1)?;
            if self.take(end) { return Ok(()); }
            self.expect(b',')?;
        }
    }

    // Sample input: true
    fn literal(&mut self, literal: &str) -> Result<(), &'static str> {
        if !self.input[self.at..].starts_with(literal) { return Err(INVALID); }
        self.at += literal.len();
        Ok(())
    }
}
