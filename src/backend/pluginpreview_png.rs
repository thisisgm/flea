use std::path::Path;
use std::time::Instant;

const PYTHON: &str = "/usr/bin/python3";
const VALIDATOR: &str = include_str!("pluginpreview_png.py");

// Validate compressed scanlines inside the existing jail before a picture is published.
pub(super) fn check(input: &std::fs::File, root: &Path) -> Result<(), String> {
    let output = super::Work::new(root)?;
    let mut jailed = super::spawn(
        Path::new("/usr"),
        Path::new(PYTHON),
        Some(input),
        &output.0,
        &["-I", "-c", VALIDATOR, super::INPUT],
    )?;
    let deadline = Instant::now() + super::REQUEST_DEADLINE;
    let result = loop {
        match jailed.child.try_wait() {
            Ok(Some(status)) => {
                break if status.success() {
                    Ok(())
                } else {
                    Err("plugin PNG could not be decoded".into())
                }
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(5))
            }
            Ok(None) => break Err("plugin PNG decoder timed out".into()),
            Err(error) => break Err(error.to_string()),
        }
    };
    crate::backend::jail::kill_tree(&mut jailed);
    result
}
