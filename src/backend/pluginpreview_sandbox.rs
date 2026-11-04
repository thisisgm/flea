//! The preview jail's existing fixed binds and resource limits, assembled in one place.
use super::super::sandbox;
use super::{ADDRESS_SPACE_BYTES, CPU_SECONDS, FILE_BYTES, INPUT, OPEN_FILES, OUTPUT};
use std::path::Path;

// Every bind is named here: the plugin folder and the input read-only, /out writable, nothing else from the host.
pub(in crate::backend) fn argv(
    folder: &Path,
    entry: &Path,
    input: Option<i32>,
    out: &Path,
) -> Vec<String> {
    let mut a: Vec<String> = vec![
        sandbox::PRLIMIT.into(),
        format!("--cpu={CPU_SECONDS}"),
        format!("--as={ADDRESS_SPACE_BYTES}"),
        format!("--fsize={FILE_BYTES}"),
        format!("--nofile={OPEN_FILES}"),
        sandbox::BWRAP.into(),
    ];
    a.extend(sandbox::BWRAP_FLAGS.iter().map(|f| f.to_string()));
    a.extend(["--setenv", "PATH", "/usr/bin", "--setenv", "HOME", "/tmp"].map(String::from));
    let folder = folder.to_string_lossy().into_owned();
    a.extend(["--ro-bind".into(), folder.clone(), folder.clone()]);
    if let Some(input) = input {
        a.extend(["--ro-bind-fd".into(), input.to_string(), INPUT.into()]);
    }
    a.extend([
        "--bind".into(),
        out.to_string_lossy().into_owned(),
        OUTPUT.into(),
    ]);
    a.extend(["--chdir".into(), folder]);
    a.push(entry.to_string_lossy().into_owned());
    sandbox::add_status(&mut a, 1);
    a
}
