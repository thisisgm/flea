//! The code engine's raw table wire: the ask a table sends and the reply it reads, with the answer it settles on.
use crate::json::{escape, field_str, field_usize};

/// The most lines one ask names (docs/protocol.md "code"); a longer window is asked in parts.
pub const LINE_CAP: usize = 400;

/// The engine's answer for a table: waiting until the first screen is answered, then styled, plain or a refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Waiting,
    Styled,
    Plain,
    Binary,
    Large,
    Failed,
}

impl Answer {
    pub fn from_wire(state: &str) -> Self {
        match state {
            "styled" => Answer::Styled,
            "plain" => Answer::Plain,
            "binary" => Answer::Binary,
            "large" => Answer::Large,
            _ => Answer::Failed,
        }
    }

    pub fn settled(self) -> bool {
        matches!(self, Answer::Styled | Answer::Plain)
    }

    /// Binary, large and failed are the file's whole answer: nothing more is asked of it.
    pub fn refused(self) -> bool {
        matches!(self, Answer::Binary | Answer::Large | Answer::Failed)
    }
}

/// One ask of the code engine: `count` lines from line `first`, aligned to record `record`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ask {
    pub record: usize,
    pub first: usize,
    pub count: usize,
}

/// The raw ask a table sends: plain lines, its delimiter, and the record the reply starts on.
pub fn ask_line(token: u64, slot: &str, path: &str, delim: char, ask: &Ask) -> String {
    format!(
        r#"{{"c":"code","token":{},"slot":"{}","path":"{}","first":{},"count":{},"raw":true,"csv":"{}","record":{}}}"#,
        token,
        escape(slot),
        escape(path),
        ask.first,
        ask.count,
        escape(&delim.to_string()),
        ask.record
    )
}

/// One code reply as a table reads it: raw lines, each its text or empty.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub token: u64,
    pub state: Answer,
    pub first: usize,
    pub total: usize,
    pub records: usize,
    pub first_record: usize,
    pub lines: Vec<String>,
}

/// Sample input: {"t":"code","token":4,"slot":"look","state":"plain","first":1,"total":2,"lines":[[0,"a,b"],[]],"records":2,"firstRecord":1}.
pub fn parse_reply(line: &str) -> Option<Reply> {
    if field_str(line, "t").as_deref() != Some("code") {
        return None;
    }
    Some(Reply {
        token: field_usize(line, "token")? as u64,
        state: Answer::from_wire(&field_str(line, "state").unwrap_or_default()),
        first: field_usize(line, "first").unwrap_or(1).max(1),
        total: field_usize(line, "total").unwrap_or(0),
        records: field_usize(line, "records").unwrap_or(0),
        first_record: field_usize(line, "firstRecord").unwrap_or(0),
        lines: raw_lines(line),
    })
}

// "lines":[[0,"text"],[],...]: each line is empty or one run, read as its text; a malformed tail ends the list.
fn raw_lines(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Some(start) = line.find(r#""lines":["#) else {
        return out;
    };
    let bytes = line.as_bytes();
    let mut at = start + r#""lines":["#.len();
    loop {
        match bytes.get(at) {
            Some(b'[') => at += 1,
            _ => return out,
        }
        let mut text = String::new();
        loop {
            match bytes.get(at) {
                Some(b']') => {
                    at += 1;
                    break;
                }
                Some(b'"') => match crate::jsonstring::parse_string(bytes, &mut at) {
                    Ok(run) => text.push_str(&run),
                    Err(_) => return out,
                },
                Some(_) => at += 1,
                None => return out,
            }
        }
        out.push(text);
        match bytes.get(at) {
            Some(b',') => at += 1,
            _ => return out,
        }
    }
}

#[cfg(test)]
#[path = "csv_wire_tests.rs"]
mod tests;
