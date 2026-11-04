//! The filter request src/backend/proto.rs reads and the `filtered` answer src/backend/filter.rs writes, checked before anything uses them.
use flea::{json::escape, jsondoc::Json};

/// One answer, still unchecked against the request in force; `exact` is None for an empty query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    pub id: u64,
    pub query: String,
    pub listing: usize,
    pub total: usize,
    pub exact: Option<Vec<usize>>,
    // Close rows ride a `close` member the Filter040 producer adds; a reply without it has none.
    pub close: Vec<usize>,
}

/// What one line is to the filter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Other,
    /// An answer to no request in force, refused by its id before a row is read.
    Stale,
    Answer(Answer),
}

/// The request line for one query against the listing in force.
pub fn request(id: u64, query: &str) -> String {
    format!(
        "{{\"c\":\"filter\",\"query\":\"{}\",\"id\":{id}}}\n",
        escape(query)
    )
}

/// None for a line that is not a filter answer; an answer whose rows break the protocol is an error, never half used.
pub fn parse(line: &str) -> Result<Option<Answer>, String> {
    Ok(match scan(line, None)? {
        Line::Answer(answer) => Some(answer),
        _ => None,
    })
}

/// One line against the requests in flight, `oldest` the oldest id awaited or None with nothing asked.
pub fn read(line: &str, oldest: Option<u64>) -> Result<Line, String> {
    scan(line, Some(oldest))
}

// The members of one flat object as raw byte spans, found without decoding a value.
struct Members<'a> {
    line: &'a [u8],
    spans: [Option<(usize, usize)>; KEYS.len()],
}

const KEYS: [&str; 8] = ["t", "id", "query", "n", "rows", "close", "listing", "ms"];

fn space(line: &[u8], mut at: usize) -> usize {
    while at < line.len() && matches!(line[at], b' ' | b'\t' | b'\n' | b'\r') {
        at += 1;
    }
    at
}

// The end of the string opening at `at`, stepping over escapes.
fn string_end(line: &[u8], mut at: usize) -> Result<usize, String> {
    at += 1;
    while at < line.len() {
        match line[at] {
            b'\\' => at += 2,
            b'"' => return Ok(at + 1),
            _ => at += 1,
        }
    }
    Err("a string never closes".into())
}

// The end of one value: a string, a bracketed array or object, or a bare number or word.
fn value_end(line: &[u8], at: usize) -> Result<usize, String> {
    match line.get(at) {
        Some(b'"') => string_end(line, at),
        Some(b'[' | b'{') => {
            let (mut depth, mut i) = (0usize, at);
            while i < line.len() {
                match line[i] {
                    b'"' => i = string_end(line, i)? - 1,
                    b'[' | b'{' => depth += 1,
                    b']' | b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(i + 1);
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            Err("an array never closes".into())
        }
        Some(_) => {
            let end = (at..line.len())
                .find(|&i| matches!(line[i], b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r'))
                .unwrap_or(line.len());
            if end == at {
                Err("a member has no value".into())
            } else {
                Ok(end)
            }
        }
        None => Err("a member has no value".into()),
    }
}

impl<'a> Members<'a> {
    fn of(text: &'a str) -> Result<Self, String> {
        let line = text.as_bytes();
        let mut members = Self {
            line,
            spans: [None; KEYS.len()],
        };
        let mut at = space(line, 0);
        if line.get(at) != Some(&b'{') {
            return Err("an answer is one object".into());
        }
        at = space(line, at + 1);
        if line.get(at) == Some(&b'}') {
            return members.end(at + 1);
        }
        loop {
            if line.get(at) != Some(&b'"') {
                return Err("a member name must be a string".into());
            }
            let key_end = string_end(line, at)?;
            let key = &line[at + 1..key_end - 1];
            at = space(line, key_end);
            if line.get(at) != Some(&b':') {
                return Err("a member name needs a colon".into());
            }
            let start = space(line, at + 1);
            let end = value_end(line, start)?;
            if let Some(slot) = KEYS.iter().position(|name| name.as_bytes() == key) {
                if members.spans[slot].replace((start, end)).is_some() {
                    return Err("a member is named twice".into());
                }
            }
            at = space(line, end);
            match line.get(at) {
                Some(b',') => at = space(line, at + 1),
                Some(b'}') => return members.end(at + 1),
                _ => return Err("members must be separated by commas".into()),
            }
        }
    }

    fn end(self, at: usize) -> Result<Self, String> {
        if space(self.line, at) == self.line.len() {
            Ok(self)
        } else {
            Err("an answer ends at its object".into())
        }
    }

    fn raw(&self, key: &str) -> Option<&'a [u8]> {
        let slot = KEYS.iter().position(|name| *name == key)?;
        self.spans[slot].map(|(start, end)| &self.line[start..end])
    }

    fn uint(&self, key: &str) -> Result<u64, String> {
        let raw = self
            .raw(key)
            .filter(|raw| raw.iter().all(u8::is_ascii_digit));
        let text = raw
            .and_then(|raw| std::str::from_utf8(raw).ok())
            .ok_or_else(|| format!("{key} must be an unsigned integer"))?;
        text.parse()
            .map_err(|_| format!("{key} must be an unsigned integer"))
    }

    fn index(&self, key: &str) -> Result<usize, String> {
        usize::try_from(self.uint(key)?).map_err(|_| format!("{key} must fit an index"))
    }

    fn text(&self, key: &str) -> Result<String, String> {
        // The member's span ends on ASCII bytes, so it is whole UTF-8 and only this value is decoded.
        let raw = self
            .raw(key)
            .filter(|raw| raw.first() == Some(&b'"'))
            .and_then(|raw| std::str::from_utf8(raw).ok());
        match raw.map(flea::jsondoc::parse) {
            Some(Ok(Json::Str(text))) => Ok(text),
            _ => Err(format!("{key} must be a string")),
        }
    }

    // Row indexes read straight from the bytes into one list: digits only, each under the listing's count.
    fn rows(&self, key: &str, total: usize) -> Result<Option<Vec<usize>>, String> {
        let Some(raw) = self.raw(key) else {
            return Ok(None);
        };
        if raw == b"null" {
            return Ok(None);
        }
        let body = raw
            .strip_prefix(b"[")
            .and_then(|raw| raw.strip_suffix(b"]"))
            .ok_or_else(|| format!("{key} must be an array"))?;
        let mut out = Vec::with_capacity(body.iter().filter(|&&b| b == b',').count() + 1);
        if space(body, 0) == body.len() {
            return Ok(Some(Vec::new()));
        }
        for item in body.split(|&b| b == b',') {
            let (from, to) = (
                space(item, 0),
                item.iter()
                    .rposition(|b| !b.is_ascii_whitespace())
                    .map_or(0, |at| at + 1),
            );
            let digits = item
                .get(from..to)
                .filter(|digits| !digits.is_empty() && digits.iter().all(u8::is_ascii_digit));
            let digits = digits.ok_or_else(|| format!("{key} holds a row that is not an index"))?;
            let row = digits.iter().try_fold(0usize, |row, &d| {
                row.checked_mul(10)?.checked_add(usize::from(d - b'0'))
            });
            match row {
                Some(row) if row < total => out.push(row),
                _ => return Err(format!("{key} names a row past the listing's {total}")),
            }
        }
        Ok(Some(out))
    }
}

fn scan(line: &str, gate: Option<Option<u64>>) -> Result<Line, String> {
    if !line.contains("\"filtered\"") {
        return Ok(Line::Other);
    }
    let members = Members::of(line)?;
    if members.raw("t") != Some(b"\"filtered\"") {
        return Ok(Line::Other);
    }
    let id = members.uint("id")?;
    if gate.is_some_and(|live| live.is_none_or(|oldest| id < oldest)) {
        return Ok(Line::Stale);
    }
    let query = members.text("query")?;
    let total = members.index("n")?;
    let listing = members.index("listing")?;
    let exact = match members.raw("rows") {
        None => return Err("rows must be an array or null".into()),
        Some(_) => members.rows("rows", total)?,
    };
    if exact
        .as_ref()
        .is_some_and(|exact| exact.windows(2).any(|pair| pair[0] >= pair[1]))
    {
        // Exact rows keep the backend's order, strictly ascending, which is what keeps folders first.
        return Err("rows must ascend without repeats".into());
    }
    let close = members.rows("close", total)?.unwrap_or_default();
    let mut seen = vec![0u64; if close.is_empty() { 0 } else { total / 64 + 1 }];
    for &row in &close {
        let (word, bit) = (row / 64, 1u64 << (row % 64));
        if seen[word] & bit != 0 {
            return Err("close must not repeat a row".into());
        }
        seen[word] |= bit;
    }
    if !close.is_empty() && exact.as_ref().is_none_or(|exact| !exact.is_empty()) {
        return Err("close rows only stand beside an empty exact list".into());
    }
    if query.is_empty() != exact.is_none() {
        return Err("an empty query answers null rows and only it does".into());
    }
    Ok(Line::Answer(Answer {
        id,
        query,
        listing,
        total,
        exact,
        close,
    }))
}
