use std::cmp::Ordering;
use std::ffi::{c_char, c_int, CStr, CString};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[repr(C)]
pub struct FleaEntryC {
    pub name: *const c_char,
    pub is_dir: bool,
    pub is_link: bool,
    pub size: u64,
    pub mtime: i64,
    pub mode: u32,
    pub icon: *const c_char,
    pub kind: c_int,
    pub target: *const c_char,
}

#[derive(Clone, Debug)]
pub struct FleaEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_link: bool,
    pub size: u64,
    pub mtime: i64,
    pub mode: u32,
    pub icon: String,
    pub kind: i32,
    pub target: String,
    // Cached CStrings for C ABI
    c_name: Option<CString>,
    c_icon: Option<CString>,
    c_target: Option<CString>,
}

pub struct FleaDirectory {
    pub path: PathBuf,
    pub entries: Vec<FleaEntry>,
}

fn determine_icon(name: &str, is_dir: bool, is_link: bool, mode: u32) -> (&'static str, i32) {
    if is_dir {
        return ("folder", 1);
    }
    if is_link {
        return ("emblem-symbolic-link", 2);
    }
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") || lower.ends_with(".webp") || lower.ends_with(".gif") || lower.ends_with(".svg") {
        return ("image-x-generic", 3);
    }
    if lower.ends_with(".mp4") || lower.ends_with(".mkv") || lower.ends_with(".webm") || lower.ends_with(".mov") || lower.ends_with(".avi") {
        return ("video-x-generic", 4);
    }
    if lower.ends_with(".mp3") || lower.ends_with(".flac") || lower.ends_with(".wav") || lower.ends_with(".ogg") || lower.ends_with(".m4a") {
        return ("audio-x-generic", 5);
    }
    if lower.ends_with(".pdf") {
        return ("application-pdf", 6);
    }
    if lower.ends_with(".tar") || lower.ends_with(".gz") || lower.ends_with(".zip") || lower.ends_with(".7z") || lower.ends_with(".zst") || lower.ends_with(".xz") {
        return ("package-x-generic", 7);
    }
    if lower.ends_with(".rs") || lower.ends_with(".c") || lower.ends_with(".cpp") || lower.ends_with(".h") || lower.ends_with(".py") || lower.ends_with(".js") || lower.ends_with(".ts") || lower.ends_with(".qml") || lower.ends_with(".json") || lower.ends_with(".toml") || lower.ends_with(".md") {
        return ("text-x-script", 8);
    }
    if mode & 0o111 != 0 {
        return ("application-x-executable", 9);
    }
    ("text-x-generic", 0)
}

// Natural alphanumeric comparison
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut a_chars = a.chars().peekable();
    let mut b_chars = b.chars().peekable();

    while let (Some(&ca), Some(&cb)) = (a_chars.peek(), b_chars.peek()) {
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let mut num_a: u64 = 0;
            while let Some(&c) = a_chars.peek() {
                if let Some(d) = c.to_digit(10) {
                    num_a = num_a.saturating_mul(10).saturating_add(d as u64);
                    a_chars.next();
                } else {
                    break;
                }
            }

            let mut num_b: u64 = 0;
            while let Some(&c) = b_chars.peek() {
                if let Some(d) = c.to_digit(10) {
                    num_b = num_b.saturating_mul(10).saturating_add(d as u64);
                    b_chars.next();
                } else {
                    break;
                }
            }

            match num_a.cmp(&num_b) {
                Ordering::Equal => continue,
                non_eq => return non_eq,
            }
        } else {
            let ca_lower = ca.to_ascii_lowercase();
            let cb_lower = cb.to_ascii_lowercase();
            match ca_lower.cmp(&cb_lower) {
                Ordering::Equal => {
                    a_chars.next();
                    b_chars.next();
                }
                non_eq => return non_eq,
            }
        }
    }

    a_chars.peek().is_some().cmp(&b_chars.peek().is_some())
}

#[no_mangle]
pub unsafe extern "C" fn flea_native_scan(
    c_path: *const c_char,
    hidden: bool,
    sort_by: c_int, // 0: name, 1: size, 2: date
    sort_desc: bool,
) -> *mut FleaDirectory {
    if c_path.is_null() {
        return std::ptr::null_mut();
    }
    let path_str = match CStr::from_ptr(c_path).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let path = Path::new(path_str);

    let read_dir = match fs::read_dir(path) {
        Ok(r) => r,
        Err(_) => return std::ptr::null_mut(),
    };

    let mut entries = Vec::new();

    for entry in read_dir {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => return std::ptr::null_mut(),
        };
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy().into_owned();

        if !hidden && name.starts_with('.') {
            continue;
        }

        let file_type = entry.file_type();
        let is_symlink = file_type.as_ref().map_or(false, |ft| ft.is_symlink());
        let meta = entry.metadata().ok();

        let (size, mtime, mode, is_dir) = if let Some(ref m) = meta {
            (m.len(), m.mtime(), m.mode(), m.is_dir())
        } else {
            let is_d = file_type.as_ref().map_or(false, |ft| ft.is_dir());
            (0, 0, 0, is_d)
        };

        let target = if is_symlink {
            fs::read_link(entry.path())
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            String::new()
        };

        let (icon, kind) = determine_icon(&name, is_dir, is_symlink, mode);

        let mut item = FleaEntry {
            name,
            is_dir,
            is_link: is_symlink,
            size,
            mtime,
            mode,
            icon: icon.to_string(),
            kind,
            target,
            c_name: None,
            c_icon: None,
            c_target: None,
        };

        item.c_name = CString::new(item.name.as_bytes()).ok();
        item.c_icon = CString::new(item.icon.as_bytes()).ok();
        item.c_target = CString::new(item.target.as_bytes()).ok();

        entries.push(item);
    }

    // Sort: directories first, then by selected key
    entries.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            return b.is_dir.cmp(&a.is_dir); // true (dir) comes before false
        }
        let ord = match sort_by {
            1 => a.size.cmp(&b.size),
            2 => a.mtime.cmp(&b.mtime),
            _ => natural_cmp(&a.name, &b.name),
        };
        if sort_desc {
            ord.reverse()
        } else {
            ord
        }
    });

    let dir = Box::new(FleaDirectory {
        path: path.to_path_buf(),
        entries,
    });

    Box::into_raw(dir)
}

#[no_mangle]
pub unsafe extern "C" fn flea_native_free(dir: *mut FleaDirectory) {
    if !dir.is_null() {
        drop(Box::from_raw(dir));
    }
}

#[no_mangle]
pub unsafe extern "C" fn flea_native_count(dir: *const FleaDirectory) -> usize {
    if dir.is_null() {
        return 0;
    }
    (*dir).entries.len()
}

#[no_mangle]
pub unsafe extern "C" fn flea_native_get_entry(
    dir: *const FleaDirectory,
    index: usize,
    out: *mut FleaEntryC,
) -> bool {
    if dir.is_null() || out.is_null() {
        return false;
    }
    let entries = &(*dir).entries;
    if index >= entries.len() {
        return false;
    }
    let e = &entries[index];
    (*out).name = e.c_name.as_ref().map_or(std::ptr::null(), |s| s.as_ptr());
    (*out).is_dir = e.is_dir;
    (*out).is_link = e.is_link;
    (*out).size = e.size;
    (*out).mtime = e.mtime;
    (*out).mode = e.mode;
    (*out).icon = e.c_icon.as_ref().map_or(std::ptr::null(), |s| s.as_ptr());
    (*out).kind = e.kind;
    (*out).target = e.c_target.as_ref().map_or(std::ptr::null(), |s| s.as_ptr());
    true
}

#[no_mangle]
pub unsafe extern "C" fn flea_native_filter(
    dir: *const FleaDirectory,
    c_query: *const c_char,
) -> *mut FleaDirectory {
    if dir.is_null() {
        return std::ptr::null_mut();
    }
    if c_query.is_null() {
        return flea_native_clone_dir(dir);
    }
    let query_str = match CStr::from_ptr(c_query).to_str() {
        Ok(s) => s.to_ascii_lowercase(),
        Err(_) => return flea_native_clone_dir(dir),
    };

    if query_str.is_empty() {
        return flea_native_clone_dir(dir);
    }

    let mut filtered_entries = Vec::new();
    for entry in &(*dir).entries {
        if entry.name.to_ascii_lowercase().contains(&query_str) {
            filtered_entries.push(entry.clone());
        }
    }

    let result = Box::new(FleaDirectory {
        path: (*dir).path.clone(),
        entries: filtered_entries,
    });

    Box::into_raw(result)
}

fn flea_native_clone_dir(dir: *const FleaDirectory) -> *mut FleaDirectory {
    unsafe {
        if dir.is_null() {
            return std::ptr::null_mut();
        }
        let cloned = Box::new(FleaDirectory {
            path: (*dir).path.clone(),
            entries: (*dir).entries.clone(),
        });
        Box::into_raw(cloned)
    }
}
