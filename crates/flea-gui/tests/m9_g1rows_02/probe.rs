//! The LD_PRELOAD probe for m9_g1rows_02: its source, its rustc build and its log reader.
use super::process::{build_dir, rustc};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// Preloaded into the backend: one byte in FLEA_PROBE_LOG per stat inside FLEA_PROBE_DIR and per libc fork, posix_spawn or posix_spawnp (not vfork or raw clone).
const PROBE: &str = r#"
use std::ffi::{c_char, c_int, c_void};
extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn getenv(name: *const c_char) -> *const c_char;
    fn open(path: *const c_char, flags: c_int, ...) -> c_int;
    fn write(fd: c_int, buf: *const c_void, count: usize) -> isize;
    fn close(fd: c_int) -> c_int;
    fn strncmp(a: *const c_char, b: *const c_char, n: usize) -> c_int;
    fn strlen(s: *const c_char) -> usize;
}
const RTLD_NEXT: *mut c_void = -1isize as *mut c_void;
const AT_FDCWD: c_int = -100;
const LOG_FLAGS: c_int = 0o1 | 0o2000 | 0o2000000;
unsafe fn note(kind: u8) {
    let log = getenv(b"FLEA_PROBE_LOG\0".as_ptr().cast());
    if log.is_null() { return; }
    let fd = open(log, LOG_FLAGS);
    if fd < 0 { return; }
    write(fd, (&kind as *const u8).cast(), 1);
    close(fd);
}
unsafe fn stat_note(path: *const c_char) {
    note(b's');
    let log = getenv(b"FLEA_PROBE_PATHS\0".as_ptr().cast());
    if log.is_null() { std::process::abort(); }
    let fd = open(log, LOG_FLAGS);
    if fd < 0 { std::process::abort(); }
    let mut record = std::ffi::CStr::from_ptr(path).to_bytes().to_vec();
    record.push(b'\n');
    if write(fd, record.as_ptr().cast(), record.len()) != record.len() as isize { std::process::abort(); }
    if close(fd) != 0 { std::process::abort(); }
}
unsafe fn inside(dirfd: c_int, path: *const c_char) -> bool {
    if path.is_null() || *path == 0 { return false; }
    if *path != b'/' as c_char { return dirfd != AT_FDCWD; }
    let dir = getenv(b"FLEA_PROBE_DIR\0".as_ptr().cast());
    if dir.is_null() { return false; }
    let n = strlen(dir);
    strncmp(path, dir, n) == 0 && *path.add(n) == b'/' as c_char
}
unsafe fn real(name: &[u8]) -> *mut c_void {
    let found = dlsym(RTLD_NEXT, name.as_ptr().cast());
    if found.is_null() { std::process::abort(); }
    found
}
#[no_mangle] pub unsafe extern "C" fn statx(d: c_int, p: *const c_char, f: c_int, m: u32, b: *mut c_void) -> c_int {
    if inside(d, p) { stat_note(p); }
    let next: extern "C" fn(c_int, *const c_char, c_int, u32, *mut c_void) -> c_int = std::mem::transmute(real(b"statx\0"));
    next(d, p, f, m, b)
}
#[no_mangle] pub unsafe extern "C" fn fstatat(d: c_int, p: *const c_char, b: *mut c_void, f: c_int) -> c_int {
    if inside(d, p) { stat_note(p); }
    let next: extern "C" fn(c_int, *const c_char, *mut c_void, c_int) -> c_int = std::mem::transmute(real(b"fstatat\0"));
    next(d, p, b, f)
}
#[no_mangle] pub unsafe extern "C" fn fstatat64(d: c_int, p: *const c_char, b: *mut c_void, f: c_int) -> c_int {
    if inside(d, p) { stat_note(p); }
    let next: extern "C" fn(c_int, *const c_char, *mut c_void, c_int) -> c_int = std::mem::transmute(real(b"fstatat64\0"));
    next(d, p, b, f)
}
#[no_mangle] pub unsafe extern "C" fn stat(p: *const c_char, b: *mut c_void) -> c_int {
    if inside(AT_FDCWD, p) { stat_note(p); }
    let next: extern "C" fn(*const c_char, *mut c_void) -> c_int = std::mem::transmute(real(b"stat\0"));
    next(p, b)
}
#[no_mangle] pub unsafe extern "C" fn stat64(p: *const c_char, b: *mut c_void) -> c_int {
    if inside(AT_FDCWD, p) { stat_note(p); }
    let next: extern "C" fn(*const c_char, *mut c_void) -> c_int = std::mem::transmute(real(b"stat64\0"));
    next(p, b)
}
#[no_mangle] pub unsafe extern "C" fn lstat(p: *const c_char, b: *mut c_void) -> c_int {
    if inside(AT_FDCWD, p) { stat_note(p); }
    let next: extern "C" fn(*const c_char, *mut c_void) -> c_int = std::mem::transmute(real(b"lstat\0"));
    next(p, b)
}
#[no_mangle] pub unsafe extern "C" fn lstat64(p: *const c_char, b: *mut c_void) -> c_int {
    if inside(AT_FDCWD, p) { stat_note(p); }
    let next: extern "C" fn(*const c_char, *mut c_void) -> c_int = std::mem::transmute(real(b"lstat64\0"));
    next(p, b)
}
#[no_mangle] pub unsafe extern "C" fn fork() -> c_int {
    note(b'p');
    let next: extern "C" fn() -> c_int = std::mem::transmute(real(b"fork\0"));
    next()
}
#[no_mangle] pub unsafe extern "C" fn posix_spawn(pid: *mut c_int, path: *const c_char, actions: *const c_void, attr: *const c_void, argv: *const *mut c_char, envp: *const *mut c_char) -> c_int {
    note(b'p');
    let next: extern "C" fn(*mut c_int, *const c_char, *const c_void, *const c_void, *const *mut c_char, *const *mut c_char) -> c_int = std::mem::transmute(real(b"posix_spawn\0"));
    next(pid, path, actions, attr, argv, envp)
}
#[no_mangle] pub unsafe extern "C" fn posix_spawnp(pid: *mut c_int, path: *const c_char, actions: *const c_void, attr: *const c_void, argv: *const *mut c_char, envp: *const *mut c_char) -> c_int {
    note(b'p');
    let next: extern "C" fn(*mut c_int, *const c_char, *const c_void, *const c_void, *const *mut c_char, *const *mut c_char) -> c_int = std::mem::transmute(real(b"posix_spawnp\0"));
    next(pid, path, actions, attr, argv, envp)
}
"#;

pub(crate) fn probe() -> &'static Path {
    static PROBE_LIB: OnceLock<PathBuf> = OnceLock::new();
    PROBE_LIB.get_or_init(|| {
        let source = build_dir().join("probe.rs");
        fs::write(&source, PROBE).unwrap();
        let library = build_dir().join("libprobe.so");
        rustc(&[
            "--edition=2021".as_ref(),
            "--crate-type=cdylib".as_ref(),
            "-Cpanic=abort".as_ref(),
            source.as_os_str(),
            "-o".as_ref(),
            library.as_os_str(),
        ]);
        library
    })
}

// Sample input: "sssp" in the probe log, one byte per stat or spawn.
pub(crate) fn probed(log: &Path, kind: u8) -> usize {
    fs::read(log)
        .expect("probe log")
        .iter()
        .filter(|&&byte| byte == kind)
        .count()
}

// Sample input: "/tmp/fx/n000001\nn000002\n" in the .paths log, absolute or fixture-relative.
// Fixture names contain no newlines; one successful stat hook writes one complete path record.
pub(crate) fn stat_targets(dir: &Path, log: &Path) -> Vec<PathBuf> {
    fs::read_to_string(log.with_extension("paths"))
        .expect("stat identity log")
        .lines()
        .map(|line| {
            let path = Path::new(line);
            if path.is_absolute() {
                path.to_owned()
            } else {
                dir.join(path)
            }
        })
        .collect()
}

pub(crate) fn offscreen(paths: &[PathBuf], visible: &BTreeSet<PathBuf>) -> usize {
    paths.iter().filter(|path| !visible.contains(*path)).count()
}
