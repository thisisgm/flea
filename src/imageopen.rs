// The image gallery's default-handler capability and shared argv bound, never a viewer choice.
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

pub const MAX_IMAGES: usize = 4096;
const MAX_BYTES: usize = 512 * 1024;
const RESERVE_BYTES: usize = 64 * 1024;
const MAX_PATH_BYTES: usize = 32 * 1024;
const URI_ARG_OVERHEAD_BYTES: usize = 16;
const URI_ESCAPE_MULTIPLIER: usize = 3;
const MIN_ARG_MAX_BYTES: usize = 128 * 1024;
const SC_ARG_MAX: i32 = 0;
extern "C" {
    fn sysconf(name: i32) -> std::os::raw::c_long;
}

pub struct Budget {
    bytes: usize,
    count: usize,
}
impl Budget {
    pub fn new() -> Self {
        // Linux _SC_ARG_MAX is 0; the fallback is Linux's minimum, never the usual 2 MiB guess.
        let maximum = usize::try_from(unsafe { sysconf(SC_ARG_MAX) }).unwrap_or(MIN_ARG_MAX_BYTES);
        let environment: usize = std::env::vars_os()
            .map(|(key, value)| {
                key.as_bytes().len() + value.as_bytes().len() + 2 + std::mem::size_of::<usize>()
            })
            .sum();
        Self::with_bytes(
            maximum
                .saturating_sub(environment + RESERVE_BYTES)
                .min(MAX_BYTES),
        )
    }
    fn with_bytes(bytes: usize) -> Self {
        Self { bytes, count: 0 }
    }
    pub fn take(&mut self, path: &Path) -> bool {
        // %U may escape every UTF-8 byte as three ASCII bytes; include file://, NUL and argv pointer.
        let length = path.as_os_str().as_bytes().len();
        if length > MAX_PATH_BYTES {
            return false;
        }
        let cost = length
            .saturating_mul(URI_ESCAPE_MULTIPLIER)
            .saturating_add(URI_ARG_OVERHEAD_BYTES);
        if self.count >= MAX_IMAGES || cost > self.bytes {
            return false;
        }
        self.bytes -= cost;
        self.count += 1;
        true
    }
}

#[cfg(test)]
#[path = "imageopen_tests.rs"]
mod handoff_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ten_thousand_paths_fit_the_count_and_uri_byte_bounds() {
        let mut budget = Budget::with_bytes(MAX_BYTES);
        let chosen: Vec<_> = (0..10_000)
            .map(|i| format!("/images/image-{i:05}.png"))
            .take_while(|p| budget.take(Path::new(p)))
            .collect();
        assert_eq!(chosen.len(), MAX_IMAGES);
        assert!(chosen.iter().map(|p| p.len() * 3 + 16).sum::<usize>() <= MAX_BYTES);
        let mut budget = Budget::with_bytes(128);
        assert!(budget.take(Path::new("/a.png")));
        assert!(!budget.take(Path::new(&format!("/{}.png", "x".repeat(100)))));
        assert_eq!(budget.count, 1);
        assert!(
            !Budget::with_bytes(MAX_BYTES).take(Path::new(&format!("/{}", "x".repeat(33 * 1024))))
        );
    }
}
