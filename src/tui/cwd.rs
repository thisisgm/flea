use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

#[derive(Default, Debug)]
pub struct Options {
    pub cwd_file: Option<PathBuf>,
    pub chooser_file: Option<PathBuf>,
    pub keymap: Option<String>,
}
impl Options {
    pub fn take(&mut self, args: &[String], index: &mut usize) -> Result<bool, String> {
        let flag = args[*index].as_str();
        if !matches!(flag, "--cwd-file" | "--chooser-file" | "--tui-keymap") { return Ok(false); }
        *index += 1;
        let value = args.get(*index).filter(|s| !s.is_empty() && !s.starts_with("--"))
            .ok_or_else(|| super::words::args(if flag == "--tui-keymap" { "tui.cwd.needs_preset" } else { "tui.cwd.needs_path" }, vec![flag.into()]).to_string())?;
        if flag == "--tui-keymap" {
            if !matches!(value.as_str(), "default" | "vim" | "mac" | "windows" | "yazi") {
                return Err(super::words::Text::Key("tui.cwd.tui_keymap_accepts_default_vim_mac_windows_or_yazi").to_string());
            }
            if self.keymap.replace(value.clone()).is_some() { return Err(super::words::args("tui.cwd.twice", vec![flag.into()]).to_string()); }
        } else {
            let path = flea::bytename::exact(value);
            let path = if path.is_absolute() { path } else { std::env::current_dir().map_err(|e| e.to_string())?.join(path) };
            if path.file_name().is_none() { return Err(super::words::args("tui.cwd.needs_file_path", vec![flag.into()]).to_string()); }
            let slot = if flag == "--cwd-file" { &mut self.cwd_file } else { &mut self.chooser_file };
            if slot.replace(path).is_some() { return Err(super::words::args("tui.cwd.twice", vec![flag.into()]).to_string()); }
        }
        Ok(true)
    }
    pub fn validate(&self, tui: bool, print_target: bool) -> Result<(), String> {
        if (self.cwd_file.is_some() || self.chooser_file.is_some() || self.keymap.is_some()) && (!tui || print_target) {
            return Err(super::words::Text::Key("tui.cwd.cwd_file_chooser_file_and_tui_keymap_require_tui").to_string());
        }
        // An output whose folder is missing is refused here, before the session, rather than failing at quit.
        for (flag, output) in [("--cwd-file", &self.cwd_file), ("--chooser-file", &self.chooser_file)] {
            if let Some(folder) = output.as_deref().and_then(Path::parent).filter(|folder| !folder.is_dir()) {
                return Err(super::words::args("tui.cwd.folder_missing", vec![flag.into(), folder.display().to_string().into()]).to_string());
            }
        }
        let cwd = self.cwd_file.as_deref().and_then(output_identity);
        let chooser = self.chooser_file.as_deref().and_then(output_identity);
        if self.cwd_file.is_some() && (self.cwd_file == self.chooser_file || cwd.is_some() && cwd == chooser) {
            return Err(super::words::Text::Key("tui.cwd.cwd_file_and_chooser_file_need_different_output_paths").to_string());
        }
        Ok(())
    }
    pub fn finish(&self, directory: Option<&Path>, chosen: Option<&[String]>, clean: bool, no_cwd: bool) -> io::Result<i32> {
        if !clean { return Ok(if self.cwd_file.is_some() || self.chooser_file.is_some() { 130 } else { 0 }); }
        if !no_cwd && self.cwd_file.is_some() && directory.is_none() {
            return Err(super::words::error("tui.cwd.no_directory_was_successfully_entered_so_no_cwd_file_was_written"));
        }
        if let Some(output) = &self.chooser_file {
            let Some(paths) = chosen else { return Ok(1); };
            write_paths(output, paths.iter().map(Path::new))?;
        }
        if !no_cwd {
            if let (Some(output), Some(directory)) = (&self.cwd_file, directory) {
                write_paths(output, [directory])?;
            }
        }
        Ok(0)
    }
}
fn output_identity(path: &Path) -> Option<PathBuf> {
    Some(path.parent()?.canonicalize().ok()?.join(path.file_name()?))
}

// Exclusive owner-only temporary files and rename preserve the caller's destination on failure.
pub fn write_paths<'a>(output: &Path, paths: impl IntoIterator<Item = &'a Path>) -> io::Result<()> {
    let mut body = Vec::new();
    for path in paths {
        let text = path.to_str().filter(|s| path.is_absolute() && !s.contains(['\n', '\r']))
            .ok_or_else(|| super::words::error("tui.cwd.a_chosen_path_cannot_be_represented_as_one_text_line"))?;
        body.extend_from_slice(text.as_bytes());
        body.push(b'\n');
    }
    if body.is_empty() { return Err(super::words::error("tui.cwd.no_paths_were_chosen")); }
    let parent = output.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = output.file_name().ok_or_else(|| super::words::error("tui.cwd.the_output_needs_a_file_name"))?;
    let mut made = None;
    for serial in 0..64 {
        let mut temp = OsString::from(".");
        temp.push(name);
        temp.push(format!(".{}.{}.tmp", std::process::id(), serial));
        let temp = parent.join(temp);
        match OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temp) {
            Ok(file) => { made = Some((temp, file)); break; }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    let (temp, mut file) = made.ok_or_else(|| super::words::error("tui.cwd.no_private_output_temporary_file_is_available"))?;
    let result = (|| {
        file.write_all(&body)?;
        file.sync_all()?;
        fs::rename(&temp, output)
    })();
    if result.is_err() { let _ = fs::remove_file(&temp); }
    result.map_err(|e| io::Error::other(super::words::args("tui.cwd.write_error", vec![output.display().to_string().into(), e.to_string().into()])))
}

#[cfg(test)]
#[path = "cwd_tests.rs"]
mod tests;
