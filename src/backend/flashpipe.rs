// The sandboxed producer behind a piped flash: one argv per kind, the image as its stdin.
use crate::backend::flashwrite::{Fail, Job};
use flea::bytename::ExactText as _;
use std::os::raw::c_int;

extern "C" {
    fn lseek(fd: c_int, offset: i64, whence: c_int) -> i64;
}
// lseek whence: SEEK_CUR reads the shared offset without moving it.
const SEEK_CUR: c_int = 1;

// The decompressor argv for a kind; raw images are read by Flea directly and need none.
fn decompressor_argv(kind: &str, image: &std::path::Path, member: &str) -> Option<Vec<String>> {
    match kind {
        "xz" => Some(vec!["xz".to_string(), "-dc".to_string(), "-T4".to_string()]),
        "zst" => Some(vec!["zstd".to_string(), "-dc".to_string()]),
        "gz" => Some(vec!["gzip".to_string(), "-dc".to_string()]),
        "bz2" => Some(vec!["bzip2".to_string(), "-dc".to_string()]),
        "zip" => Some(vec![
            "bsdtar".to_string(),
            "-xOf".to_string(),
            image.exact_text().to_string(),
            escape_member(member),
        ]),
        _ => None,
    }
}

// bsdtar reads -xOf operands as patterns, so a member name travels with its pattern bytes escaped.
pub(crate) fn escape_member(member: &str) -> String {
    let mut out = String::with_capacity(member.len());
    for c in member.chars() {
        if matches!(c, '\\' | '[' | ']' | '*' | '?') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

// The streaming decompressor jail, with the image bound read-only for a zip and stdin otherwise.
pub fn wrap_stream(inner: &[String], image: Option<&std::path::Path>) -> Vec<String> {
    use crate::backend::sandbox::{ADDRESS_SPACE_BYTES, BWRAP, BWRAP_FLAGS, PRLIMIT};
    let mut a: Vec<String> = Vec::with_capacity(inner.len() + BWRAP_FLAGS.len() + 6);
    // prlimit stays outermost so the address-space cap still arrives, without the decoder's CPU cap.
    a.push(PRLIMIT.to_string());
    a.push(format!("--as={}", ADDRESS_SPACE_BYTES));
    a.push(BWRAP.to_string());
    for flag in BWRAP_FLAGS {
        a.push(flag.to_string());
    }
    if let Some(path) = image {
        a.push("--ro-bind".to_string());
        a.push(path.exact_text().to_string());
        a.push(path.exact_text().to_string());
    }
    a.extend_from_slice(inner);
    a
}

// The producer's jail argv: a zip binds the image read-only in the stream jail, anything else streams on stdin.
pub(crate) fn producer_jail(inner: &[String], kind: &str, image: &std::path::Path) -> Vec<String> {
    if kind == "zip" {
        crate::backend::sandbox::wrap_stream(inner, Some(image))
    } else {
        crate::backend::sandbox::wrap_stream(inner, None)
    }
}

// Starts the sandboxed producer with the image as stdin; a zip is bound read-only instead of piped.
pub(crate) fn spawn_producer(job: &Job) -> Result<(std::process::Child, Option<std::fs::File>), Fail> {
    let (kind, image, member) = (job.kind, job.image, job.member);
    #[cfg(test)]
    {
        if let Some(argv) = job.decompressor.as_ref() {
            let child = std::process::Command::new(&argv[0]).args(&argv[1..])
                .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null())
                .spawn().map_err(|e| Fail::Decompress(e.to_string()))?;
            return Ok((child, None));
        }
    }
    if !crate::backend::sandbox::available() {
        return Err(Fail::Sandbox);
    }
    let Some(inner) = decompressor_argv(kind, image, member) else { return Err(Fail::Decompress("no producer for a raw image".to_string())) };
    let full = producer_jail(&inner, kind, image);
    // The parent keeps a handle on the child's description, so lseek reads the consumed input offset without moving it.
    let input = if kind == "zip" { None } else { std::fs::File::open(image).ok() };
    let stdin = match input.as_ref().and_then(|f| f.try_clone().ok()) {
        Some(dup) => std::process::Stdio::from(dup),
        None => std::process::Stdio::null(),
    };
    let jailed = crate::backend::jail::spawn_jailed(&full, |cmd| {
        cmd.stdin(stdin);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::null());
    }).map_err(|e| Fail::Decompress(e.to_string()))?;
    Ok((jailed.child, input))
}

// The consumed input offset over the producer's shared description; written stands in where no handle was kept.
pub(crate) fn input_offset(input: Option<&std::fs::File>, written: u64) -> u64 {
    use std::os::fd::AsRawFd;
    match input {
        Some(file) => {
            let at = unsafe { lseek(file.as_raw_fd(), 0, SEEK_CUR) };
            if at < 0 { written } else { at as u64 }
        }
        None => written,
    }
}

pub(crate) fn try_kill(child: &mut std::process::Child) {
    let _ = child.kill();
}

#[cfg(test)]
#[path = "flashpipe_tests.rs"]
mod tests;
