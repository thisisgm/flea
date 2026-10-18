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
            .ok_or_else(|| format!("{} needs {}", flag, if flag == "--tui-keymap" { "a preset" } else { "a path" }))?;
        if flag == "--tui-keymap" {
            if !matches!(value.as_str(), "default" | "vim" | "mac" | "windows" | "yazi") {
                return Err("--tui-keymap accepts default, vim, mac, windows or yazi".into());
            }
            if self.keymap.replace(value.clone()).is_some() { return Err(format!("{} was given twice", flag)); }
        } else {
            let path = PathBuf::from(value);
            let path = if path.is_absolute() { path } else { std::env::current_dir().map_err(|e| e.to_string())?.join(path) };
            if path.file_name().is_none() { return Err(format!("{} needs a file path", flag)); }
            let slot = if flag == "--cwd-file" { &mut self.cwd_file } else { &mut self.chooser_file };
            if slot.replace(path).is_some() { return Err(format!("{} was given twice", flag)); }
        }
        Ok(true)
    }
    pub fn validate(&self, tui: bool, print_target: bool) -> Result<(), String> {
        if (self.cwd_file.is_some() || self.chooser_file.is_some() || self.keymap.is_some()) && (!tui || print_target) {
            return Err("--cwd-file, --chooser-file and --tui-keymap require --tui".into());
        }
        let cwd = self.cwd_file.as_deref().and_then(output_identity);
        let chooser = self.chooser_file.as_deref().and_then(output_identity);
        if self.cwd_file.is_some() && (self.cwd_file == self.chooser_file || cwd.is_some() && cwd == chooser) {
            return Err("--cwd-file and --chooser-file need different output paths".into());
        }
        Ok(())
    }
    pub fn finish(&self, directory: Option<&Path>, chosen: Option<&[String]>, clean: bool, no_cwd: bool) -> io::Result<i32> {
        if !clean { return Ok(if self.cwd_file.is_some() || self.chooser_file.is_some() { 130 } else { 0 }); }
        if !no_cwd && self.cwd_file.is_some() && directory.is_none() {
            return Err(io::Error::other("No directory was successfully entered, so no cwd file was written"));
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
            .ok_or_else(|| io::Error::other("A chosen path cannot be represented as one text line"))?;
        body.extend_from_slice(text.as_bytes());
        body.push(b'\n');
    }
    if body.is_empty() { return Err(io::Error::other("No paths were chosen")); }
    let parent = output.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = output.file_name().ok_or_else(|| io::Error::other("The output needs a file name"))?;
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
    let (temp, mut file) = made.ok_or_else(|| io::Error::other("No private output temporary file is available"))?;
    let result = (|| {
        file.write_all(&body)?;
        file.sync_all()?;
        fs::rename(&temp, output)
    })();
    if result.is_err() { let _ = fs::remove_file(&temp); }
    result.map_err(|e| io::Error::other(format!("Could not write {}: {}", output.display(), e)))
}

#[cfg(test)]
#[path = "cwd_tests.rs"]
mod tests;
