use crate::backend::sandbox;
use crate::backend::thumbwrite::exclusive_temp;
use std::io::{self, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};

const EXEC_GATE: &str = "/flea-sandbox-exec-gate";

pub struct Launch {
    pub argv: Vec<String>,
    pub error: PathBuf,
}

impl Launch {
    pub fn new(inner: &[String], input: &Path, output: &Path) -> io::Result<Self> {
        let exe = std::env::current_exe()?;
        let dir = output.parent().ok_or_else(|| io::Error::other("thumbnail output has no parent"))?;
        let error = exclusive_temp(dir).ok_or_else(|| io::Error::other("thumbnail exec error file could not be created"))?;
        let argv = wrap_thumbnail(inner, input, output, &error, &exe);
        Ok(Self { argv, error })
    }
}

impl Drop for Launch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.error);
    }
}

fn wrap_thumbnail(inner: &[String], input: &Path, output: &Path, error: &Path, exe: &Path) -> Vec<String> {
    let mut argv = sandbox::wrap_thumbnail_support(input, output, error, exe, Path::new(EXEC_GATE), inner.len() + 4);
    argv.push(EXEC_GATE.to_string());
    #[cfg(test)]
    {
        argv.extend(["--exact".to_string(), "backend::sandbox_broker::tests::sandbox_exec_gate_child".to_string(), "--nocapture".to_string()]);
        return with_test_gate_environment(argv, inner, error);
    }
    #[cfg(not(test))]
    {
        argv.push("--sandbox-exec-gate".to_string());
        argv.push(error.to_string_lossy().into_owned());
        argv.push("--".to_string());
        argv.extend_from_slice(inner);
        argv
    }
}

#[cfg(test)]
fn with_test_gate_environment(mut argv: Vec<String>, inner: &[String], error: &Path) -> Vec<String> {
    let command = argv.split_off(argv.len() - 4);
    argv.extend(["--setenv".to_string(), "FLEA_EXEC_GATE_ERROR".to_string(), error.to_string_lossy().into_owned()]);
    argv.extend(["--setenv".to_string(), "FLEA_EXEC_GATE_ARGC".to_string(), inner.len().to_string()]);
    for (index, arg) in inner.iter().enumerate() {
        argv.extend(["--setenv".to_string(), format!("FLEA_EXEC_GATE_ARG_{index}"), arg.clone()]);
    }
    argv.extend(command);
    argv
}

pub fn exec_gate_main(args: &[String]) -> i32 {
    let (error_path, argv) = match args {
        [_, mode, error, separator, rest @ ..] if mode == "--sandbox-exec-gate" && separator == "--" && !rest.is_empty() => (error, rest),
        _ => return 2,
    };
    let error = std::process::Command::new(&argv[0]).args(&argv[1..]).exec();
    if let Ok(mut file) = std::fs::OpenOptions::new().write(true).truncate(true).open(error_path) {
        let _ = file.write_all(error.to_string().as_bytes());
    }
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_exec_gate_rejects_a_missing_program() {
        let args = vec!["flea".to_string(), "--sandbox-exec-gate".to_string(), "/error".to_string(), "--".to_string()];
        assert_eq!(exec_gate_main(&args), 2);
    }

    #[test]
    fn the_thumbnail_wrapper_binds_its_gate_and_preserves_decoder_arguments() {
        let hostile = "two words\n${path%/*}".to_string();
        let decoder = vec!["/usr/bin/decoder".to_string(), hostile.clone()];
        let got = wrap_thumbnail(&decoder, Path::new("/in/a.mp4"), Path::new("/out/x.png"), Path::new("/out/error"), Path::new("/proc/self/exe"));
        assert!(got.windows(3).any(|w| w == ["--bind", "/out/error", "/out/error"]));
        assert!(got.windows(3).any(|w| w == ["--ro-bind", "/proc/self/exe", EXEC_GATE]));
        assert_eq!(got.iter().filter(|arg| **arg == hostile).count(), 1);
        assert!(got.windows(3).any(|w| w == ["--setenv", "FLEA_EXEC_GATE_ARG_1", hostile.as_str()]));
    }

    #[test]
    fn a_launch_owns_and_removes_its_error_file() {
        let sandbox = crate::backend::testdir::TestDir::new("sandbox-exec");
        let output = sandbox.file("out.png", "");
        let error = {
            let launch = Launch::new(&["/usr/bin/true".to_string()], &output, &output).unwrap();
            assert!(launch.error.exists());
            launch.error.clone()
        };
        assert!(!error.exists());
    }
}
