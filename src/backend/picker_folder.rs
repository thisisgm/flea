// The save picker's only mutation: one new directory under the folder reviewed when the card opened.
use super::{Held, escape, field_str, field_usize, io_message};
use crate::backend::trashmanifest::Cancellation;
use std::ffi::CString;
use std::os::fd::AsRawFd;
use std::path::PathBuf;

extern "C" {
    fn mkdirat(dirfd: i32, pathname: *const std::os::raw::c_char, mode: u32) -> i32;
}

#[derive(Default)]
pub(super) struct Draft {
    held: Option<(usize, Held)>,
}
impl Draft {
    pub(super) fn prepare(&mut self, line: &str, id: usize) -> Result<String, String> {
        self.held = None;
        let path = PathBuf::from(field_str(line, "folder").unwrap_or_default());
        let held = Held::open(&path, true)?;
        if !held.current()?.is_dir() { return Err("The selected location is not a folder.".into()); }
        self.held = Some((id, held));
        Ok(format!(r#""review":{},"folder":"{}""#, id, escape(&path.to_string_lossy())))
    }

    pub(super) fn create(&mut self, line: &str, cancel: &Cancellation) -> Result<String, String> {
        let name = field_str(line, "name").unwrap_or_default();
        if let Some(refusal) = crate::backend::ops::name_refusal_sentence(&name) { return Err(refusal); }
        let (id, held) = self.held.as_ref().ok_or("The location changed; reopen New folder.")?;
        if Some(*id) != field_usize(line, "review") { return Err("The location changed; reopen New folder.".into()); }
        held.current()?;
        let name_c = CString::new(name.as_str()).map_err(|_| "A name cannot contain a null character.")?;
        cancel.check()?;
        // Relative to the held descriptor, never a re-resolved path; atomic and never recursive/overwriting.
        if unsafe { mkdirat(held.file.as_raw_fd(), name_c.as_ptr(), 0o777) } != 0 {
            let error = std::io::Error::last_os_error();
            return Err(match error.kind() {
                std::io::ErrorKind::AlreadyExists => "A file or folder with that name already exists.".into(),
                std::io::ErrorKind::PermissionDenied => "You don't have permission to create a folder here.".into(),
                _ => format!("Could not create the folder: {}.", io_message(&error)),
            });
        }
        // Creation is already committed. A moved parent is a partial success, not a retryable failure.
        let result = if held.current().is_ok() {
            format!(r#""created":true,"path":"{}""#, escape(&held.path.join(name).to_string_lossy()))
        } else {
            r#""created":true,"path":"","error":"The folder was created, but its parent moved. Reopen the save location to find it.""#.into()
        };
        self.held = None;
        Ok(result)
    }

    pub(super) fn close(&mut self) -> Result<String, String> {
        self.held = None;
        Ok(r#""closed":true"#.into())
    }
}
