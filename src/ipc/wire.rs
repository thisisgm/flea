use crate::json::escape;
use crate::jsondoc::{self, Json};
use std::io::{self, BufRead, Write};

pub const REQUEST_LIMIT: usize = 64 * 1024;
pub const REPLY_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Debug, PartialEq)]
pub enum Request {
    Show,
    Call {
        target: String,
        function: String,
        arguments: Vec<String>,
    },
}

#[derive(Debug, PartialEq)]
pub struct Reply {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Reply {
    pub fn value(value: impl Into<String>) -> Self {
        Self {
            code: 0,
            stdout: format!("{}\n", value.into()),
            stderr: String::new(),
        }
    }

    pub fn error(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            stdout: String::new(),
            stderr: format!("{}\n", message.into()),
        }
    }

    pub fn encode(&self) -> String {
        format!(
            "{{\"code\":{},\"stdout\":\"{}\",\"stderr\":\"{}\"}}\n",
            self.code,
            escape(&self.stdout),
            escape(&self.stderr)
        )
    }

    pub fn decode(line: &str) -> Result<Self, String> {
        let value = jsondoc::parse(line)?;
        let code = match value.get("code") {
            Some(Json::Num(code)) => code
                .parse::<u8>()
                .map(i32::from)
                .map_err(|e| e.to_string())?,
            _ => return Err("IPC reply needs an exit code".into()),
        };
        Ok(Self {
            code,
            stdout: string(&value, "stdout")?,
            stderr: string(&value, "stderr")?,
        })
    }
}

impl Request {
    pub fn encode(&self) -> String {
        match self {
            Self::Show => "{\"show\":true}\n".into(),
            Self::Call {
                target,
                function,
                arguments,
            } => {
                let mut fields = vec![target.as_str(), function.as_str()];
                fields.extend(arguments.iter().map(String::as_str));
                let fields = fields
                    .into_iter()
                    .map(|s| format!("\"{}\"", escape(s)))
                    .collect::<Vec<_>>();
                format!("{{\"call\":[{}]}}\n", fields.join(","))
            }
        }
    }

    pub fn decode(line: &str) -> Result<Self, String> {
        let value = jsondoc::parse(line)?;
        let fields = value.as_object().ok_or("IPC request must be an object")?;
        if fields.len() != 1 {
            return Err("IPC request needs exactly one operation".into());
        }
        if value.get("show").and_then(Json::as_bool) == Some(true) {
            return Ok(Self::Show);
        }
        let fields = value
            .get("call")
            .and_then(Json::as_array)
            .ok_or("IPC request needs call or show")?;
        let fields = fields
            .iter()
            .map(|s| {
                s.as_str()
                    .map(str::to_owned)
                    .ok_or("IPC arguments must be strings")
            })
            .collect::<Result<Vec<_>, _>>()?;
        if fields.len() < 2 {
            return Err("IPC call needs a target and function".into());
        }
        let mut fields = fields.into_iter();
        Ok(Self::Call {
            target: fields.next().unwrap(),
            function: fields.next().unwrap(),
            arguments: fields.collect(),
        })
    }
}

fn string(value: &Json, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Json::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("IPC reply needs {key}"))
}

pub fn read_line(input: &mut impl BufRead, limit: usize) -> io::Result<String> {
    let mut bytes = Vec::new();
    loop {
        // A signal under a receive timeout, or a stop and continue, interrupts the read and is retried.
        let chunk = match input.fill_buf() {
            Ok(chunk) => chunk,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        if chunk.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "IPC line was incomplete",
            ));
        }
        let used = chunk
            .iter()
            .position(|&b| b == b'\n')
            .map_or(chunk.len(), |i| i + 1);
        if bytes.len() + used > limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "IPC line is too long",
            ));
        }
        let complete = chunk[used - 1] == b'\n';
        bytes.extend_from_slice(&chunk[..used]);
        input.consume(used);
        if complete {
            return String::from_utf8(bytes)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
        }
    }
}

pub fn write_line(output: &mut impl Write, line: &str) -> io::Result<()> {
    output.write_all(line.as_bytes())?;
    output.flush()
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
