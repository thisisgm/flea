// The nonces a journal stamps on the manifests it records, so a backend reattaches only its own.
use super::Journal;

// Four bytes of randomness read once per journal, with no new crate.
const NONCE_BYTES: usize = 4;
// The starttime field index after the comm in /proc/self/stat.
const START_INDEX: usize = 19;
// A fixed mix when neither urandom nor /proc answers, so the fallback still differs by pid.
const FALLBACK_XOR: u32 = 0x9e3779b9;

// One random read per journal; the fallback mixes pid with process start time.
pub(super) fn nonce_high_once() -> u32 {
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let mut buf = [0u8; NONCE_BYTES];
        use std::io::Read;
        if file.read_exact(&mut buf).is_ok() {
            return u32::from_le_bytes(buf);
        }
    }
    fallback_nonce_high()
}

// Pid xor start time, so a restarted backend reusing a pid still differs.
fn fallback_nonce_high() -> u32 {
    let pid = std::process::id();
    match process_start_time() {
        Some(time) => pid ^ (time as u32) ^ ((time >> 32) as u32),
        None => pid ^ FALLBACK_XOR,
    }
}

// Field 22 of /proc/self/stat; the comm may hold spaces, so split after the last paren.
fn process_start_time() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/stat").ok()?;
    let after = text.rsplit(')').next()?;
    after
        .split_whitespace()
        .nth(START_INDEX)?
        .parse::<u64>()
        .ok()
}

impl Journal {
    // A nonce this backend alone can reattach; the random high half keeps it unique across backends.
    pub(super) fn next_nonce(&mut self) -> u64 {
        self.next_nonce = self.next_nonce.wrapping_add(1);
        ((self.nonce_high as u64) << 32) | (self.next_nonce as u64)
    }

    // Test-only: the next nonce without recording anything, so uniqueness is pinned directly.
    #[cfg(test)]
    pub(crate) fn test_nonce(&mut self) -> u64 {
        self.next_nonce()
    }
}
