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
            let key = flea::ui_logic_status_feedback::errors::os_reason(&diagnostic);
            Err(OutputError::Access { key, diagnostic })
        }
    }
}

// Combine names its own output, so a taken name refuses there.
pub fn paths(paths: Vec<String>) -> Result<Vec<String>, OutputError> {
    paths
        .into_iter()
        .map(|path| vacant(&path).map(|()| path))
        .collect()
}

// A Convert lands a taken output under Duplicate's next free copy name, as the backend does, so the card names that one.
pub fn converted(paths: Vec<String>, pdf_pages: bool) -> Result<Vec<String>, OutputError> {
    use flea::ui_logic_print_convert::output::{copy_path, page_fallback, COPY_TRIES};
    let free = |path: &Path| {
        for n in 0..=COPY_TRIES {
            let Some(candidate) = copy_path(path, n) else {
                break;
            };
            let candidate = candidate.to_string_lossy().into_owned();
            match vacant(&candidate) {
                Err(OutputError::Exists(_)) => continue,
                found => return found.map(|()| candidate),
            }
        }
        Err(OutputError::Exists(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        ))
    };
    paths
        .into_iter()
        .map(|path| {
            let file = Path::new(&path);
            match vacant(&path) {
                Err(OutputError::Exists(_)) if pdf_pages => {
                    free(&file.with_file_name(page_fallback(file)))
                }
                _ => free(file),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn op4_real_output_probe_refuses_dangling_links_and_steps_to_free_names() {
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
            paths(vec![candidate.clone()]),
            Err(OutputError::Exists(_))
        ));
        let image = converted(vec![candidate.clone()], false).unwrap();
        assert!(image[0].ends_with("report (page 1) copy.png"), "{image:?}");
        let fallback = converted(vec![candidate.clone()], true).unwrap();
        assert!(fallback[0].ends_with("report (page 1) (converted).png"));
        std::fs::write(&fallback[0], b"kept").unwrap();
        let stepped = converted(vec![candidate], true).unwrap();
        assert!(
            stepped[0].ends_with("report (page 1) (converted) copy.png"),
            "{stepped:?}"
        );
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
