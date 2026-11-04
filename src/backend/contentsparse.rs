use crate::jsondoc::{self, Json};

#[derive(Debug)]
pub(super) struct Hit {
    pub path: Vec<u8>,
    pub line_no: usize,
    pub text: String,
}

pub(super) enum Record {
    Hit(Hit),
    End { path: Vec<u8>, binary: bool },
    Summary(usize),
    Begin,
}

fn bytes(value: &Json) -> Result<Vec<u8>, String> {
    match (value.get("text"), value.get("bytes")) {
        (Some(Json::Str(text)), None) => Ok(text.as_bytes().to_vec()),
        (None, Some(Json::Str(text))) => base64(text),
        _ => Err("invalid ripgrep byte string".into()),
    }
}

fn relative(value: &Json) -> Result<Vec<u8>, String> {
    let path = bytes(value)?;
    let path = path.strip_prefix(b"./").unwrap_or(&path);
    if path.is_empty()
        || path.starts_with(b"/")
        || path.contains(&0)
        || path
            .split(|b| *b == b'/')
            .any(|part| part.is_empty() || part == b"." || part == b"..")
    {
        return Err("ripgrep returned a path outside its scope".into());
    }
    Ok(path.to_vec())
}

// Sample input: {"type":"begin","data":{"path":{"text":"./notes.txt"}}}
pub(super) fn parse(line: &[u8]) -> Result<Record, String> {
    let document =
        jsondoc::parse(std::str::from_utf8(line).map_err(|_| "ripgrep JSON was not UTF-8")?)?;
    let kind = document
        .get("type")
        .and_then(Json::as_str)
        .ok_or("missing ripgrep record type")?;
    if kind == "summary" {
        let searches = document
            .get("data")
            .and_then(|data| data.get("stats"))
            .and_then(|stats| stats.get("searches"));
        return match searches {
            Some(Json::Num(number)) => number
                .parse()
                .map(Record::Summary)
                .map_err(|_| "invalid ripgrep search count".into()),
            _ => Err("missing ripgrep search count".into()),
        };
    }
    let data = document.get("data").ok_or("missing ripgrep record data")?;
    match kind {
        "begin" => Ok(Record::Begin),
        "end" => Ok(Record::End {
            path: relative(data.get("path").ok_or("missing ripgrep path")?)?,
            binary: !matches!(data.get("binary_offset"), None | Some(Json::Null)),
        }),
        "match" => {
            let path = relative(data.get("path").ok_or("missing ripgrep path")?)?;
            let line_no = match data.get("line_number") {
                Some(Json::Num(number)) => number.parse::<usize>().ok().filter(|n| *n > 0),
                _ => None,
            }
            .ok_or("invalid ripgrep line number")?;
            let mut line = bytes(data.get("lines").ok_or("missing ripgrep matching line")?)?;
            if line.last() == Some(&b'\n') {
                line.pop();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
            }
            if line.contains(&0) {
                return Ok(Record::End { path, binary: true });
            }
            Ok(Record::Hit(Hit {
                path,
                line_no,
                text: String::from_utf8_lossy(&line).into_owned(),
            }))
        }
        _ => Err("unexpected ripgrep record type".into()),
    }
}

fn base64(text: &str) -> Result<Vec<u8>, String> {
    if text.len() % 4 != 0 {
        return Err("invalid ripgrep base64".into());
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let digit = |b| match b {
        b'A'..=b'Z' => Ok(b - b'A'),
        b'a'..=b'z' => Ok(b - b'a' + 26),
        b'0'..=b'9' => Ok(b - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err("invalid ripgrep base64".to_owned()),
    };
    let chunks = text.as_bytes().chunks_exact(4);
    let count = chunks.len();
    for (index, chunk) in chunks.enumerate() {
        let a = digit(chunk[0])?;
        let b = digit(chunk[1])?;
        out.push(a << 2 | b >> 4);
        if chunk[2] == b'=' {
            if chunk[3] != b'=' || b & 15 != 0 || index + 1 != count {
                return Err("invalid ripgrep base64 padding".into());
            }
            continue;
        }
        let c = digit(chunk[2])?;
        out.push(b << 4 | c >> 2);
        if chunk[3] == b'=' {
            if c & 3 != 0 || index + 1 != count {
                return Err("invalid ripgrep base64 padding".into());
            }
        } else {
            out.push(c << 6 | digit(chunk[3])?);
        }
    }
    Ok(out)
}
