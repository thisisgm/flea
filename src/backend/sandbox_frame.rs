use crate::backend::cgroup::Events;
use crate::backend::child::Ran;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::Duration;

pub const RESULT_BYTES: usize = 33;
const MAX_ARGS: usize = 4096;
const MAX_ARG_BYTES: usize = 16 * 1024 * 1024;

pub struct Request {
    pub argv: Vec<String>,
    pub timeout: Duration,
    pub exec_error: Option<String>,
}

pub fn write_request(out: &mut impl Write, argv: &[String], timeout: Duration, exec_error: Option<&Path>) -> io::Result<()> {
    if argv.is_empty() || argv.len() > MAX_ARGS {
        return Err(io::Error::other("invalid broker argument count"));
    }
    write_u32(out, argv.len())?;
    let timeout_ms: u64 = timeout.as_millis().try_into().map_err(io::Error::other)?;
    out.write_all(&timeout_ms.to_le_bytes())?;
    let error = exec_error.map(|path| path.to_string_lossy()).unwrap_or_default();
    write_bytes(out, error.as_bytes())?;
    for arg in argv {
        write_bytes(out, arg.as_bytes())?;
    }
    Ok(())
}

pub fn read_request(input: &mut impl Read) -> io::Result<Option<Request>> {
    let count = match read_u32_or_eof(input)? {
        Some(value) if value > 0 && value <= MAX_ARGS => value,
        Some(_) => return Err(io::Error::other("invalid broker argument count")),
        None => return Ok(None),
    };
    let timeout = Duration::from_millis(read_u64(input)?);
    let exec_error = read_bytes(input)?;
    let exec_error = if exec_error.is_empty() { None } else { Some(String::from_utf8(exec_error).map_err(io::Error::other)?) };
    let mut argv = Vec::with_capacity(count);
    for _ in 0..count {
        let bytes = read_bytes(input)?;
        argv.push(String::from_utf8(bytes).map_err(io::Error::other)?);
    }
    Ok(Some(Request { argv, timeout, exec_error }))
}

pub fn write_result(out: &mut impl Write, ran: Ran, events: &Events) -> io::Result<()> {
    out.write_all(&[match ran {
        Ran::Succeeded => 0,
        Ran::Failed => 1,
        Ran::NotStarted => 2,
    }])?;
    for value in [events.max, events.oom, events.oom_kill, events.peak] {
        out.write_all(&value.to_le_bytes())?;
    }
    Ok(())
}

pub fn read_result(input: &mut impl Read) -> io::Result<(Ran, Events)> {
    let ran = match read_u8(input)? {
        0 => Ran::Succeeded,
        1 => Ran::Failed,
        2 => Ran::NotStarted,
        _ => return Err(io::Error::other("invalid broker result")),
    };
    let events = Events { max: read_u64(input)?, oom: read_u64(input)?, oom_kill: read_u64(input)?, peak: read_u64(input)? };
    Ok((ran, events))
}

fn write_u32(out: &mut impl Write, value: usize) -> io::Result<()> {
    let value: u32 = value.try_into().map_err(io::Error::other)?;
    out.write_all(&value.to_le_bytes())
}

fn write_bytes(out: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    if bytes.len() > MAX_ARG_BYTES {
        return Err(io::Error::other("broker argument is too large"));
    }
    write_u32(out, bytes.len())?;
    out.write_all(bytes)
}

fn read_bytes(input: &mut impl Read) -> io::Result<Vec<u8>> {
    let len = read_u32(input)?;
    if len > MAX_ARG_BYTES {
        return Err(io::Error::other("broker argument is too large"));
    }
    let mut bytes = vec![0; len];
    input.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn read_u8(input: &mut impl Read) -> io::Result<u8> {
    let mut bytes = [0; 1];
    input.read_exact(&mut bytes)?;
    Ok(bytes[0])
}

fn read_u32(input: &mut impl Read) -> io::Result<usize> {
    let mut bytes = [0; 4];
    input.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes) as usize)
}

fn read_u32_or_eof(input: &mut impl Read) -> io::Result<Option<usize>> {
    let mut bytes = [0; 4];
    let mut read = 0;
    while read < bytes.len() {
        match input.read(&mut bytes[read..])? {
            0 if read == 0 => return Ok(None),
            0 => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "partial broker frame")),
            count => read += count,
        }
    }
    Ok(Some(u32::from_le_bytes(bytes) as usize))
}

fn read_u64(input: &mut impl Read) -> io::Result<u64> {
    let mut bytes = [0; 8];
    input.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn request_frames_preserve_spaces_and_newlines() {
        let argv = ["program".to_string(), "two words".to_string(), "line\nbreak".to_string()];
        let mut bytes = Vec::new();
        write_request(&mut bytes, &argv, Duration::from_secs(20), Some(Path::new("/tmp/error file"))).unwrap();
        let got = read_request(&mut bytes.as_slice()).unwrap().unwrap();
        assert_eq!(got.argv, argv);
        assert_eq!(got.timeout, Duration::from_secs(20));
        assert_eq!(got.exec_error.as_deref(), Some("/tmp/error file"));
    }

    #[test]
    fn result_frames_round_trip_all_classes_and_accounting() {
        for ran in [Ran::Succeeded, Ran::Failed, Ran::NotStarted] {
            let events = Events { max: 1, oom: 2, oom_kill: 3, peak: 4 };
            let mut bytes = Vec::new();
            write_result(&mut bytes, ran, &events).unwrap();
            assert_eq!(bytes.len(), RESULT_BYTES);
            assert_eq!(read_result(&mut bytes.as_slice()).unwrap(), (ran, events));
        }
    }
}
