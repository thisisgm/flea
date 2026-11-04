//! Output probing runs on the executor; every publication is still revalidated by the backend.
use super::seam::OutputError;
use std::path::Path;

fn vacant(path: &str) -> Result<(), OutputError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Err(OutputError::Exists(
            path.rsplit('/').next().unwrap_or(path).into(),
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => {
            let diagnostic = flea::error::io_message(&e);
            let key = flea::ui_logic_fb1::errors::os_reason(&diagnostic);
            Err(OutputError::Access { key, diagnostic })
        }
    }
}

pub fn paths(paths: Vec<String>, pdf_pages: bool) -> Result<Vec<String>, OutputError> {
    paths
        .into_iter()
        .map(|path| {
            if pdf_pages && vacant(&path).is_err() {
                let file = Path::new(&path);
                let fallback = file.with_file_name(flea::ui_logic_op4::output::page_fallback(file));
                let fallback = fallback.to_string_lossy().into_owned();
                vacant(&fallback)?;
                Ok(fallback)
            } else {
                vacant(&path)?;
                Ok(path)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn op4_real_output_probe_refuses_dangling_links_and_resolves_page_fallback() {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g2-op4");
        let parent = std::fs::canonicalize(&parent).unwrap_or_else(|_| {
            std::fs::create_dir_all(&parent).unwrap();
            std::fs::canonicalize(&parent).unwrap()
        });
        let made = std::process::Command::new("mktemp")
            .arg("-d")
            .arg(parent.join("probe.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(made.status.success());
        let root = std::path::PathBuf::from(String::from_utf8(made.stdout).unwrap().trim_end());
        assert!(root.is_absolute() && root.parent() == Some(parent.as_path()));
        std::fs::write(root.join(".flea-test-sandbox"), b"output probe").unwrap();
        let candidate = root.join("report (page 1).png");
        std::os::unix::fs::symlink(root.join("absent"), &candidate).unwrap();
        let candidate = candidate.to_string_lossy().into_owned();
        assert!(matches!(
            paths(vec![candidate.clone()], false),
            Err(OutputError::Exists(_))
        ));
        let fallback = paths(vec![candidate.clone()], true).unwrap();
        assert!(fallback[0].ends_with("report (page 1) (converted).png"));
        std::fs::write(&fallback[0], b"kept").unwrap();
        assert!(paths(vec![candidate], true).is_err());
        assert_eq!(std::fs::read(&fallback[0]).unwrap(), b"kept");
        assert!(
            root.is_absolute()
                && !root.as_os_str().is_empty()
                && root.parent() == Some(parent.as_path())
                && root.join(".flea-test-sandbox").is_file()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
