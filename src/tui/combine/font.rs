//! Resolve the installed monospace once; a missing font leaves ImageMagick's default intact.
use crate::backend::{printtool, regfile};
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, LazyLock};

static MONOSPACE: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
    resolve(
        &[
            "fc-match".into(),
            "-f".into(),
            "%{file}".into(),
            "monospace".into(),
        ],
        &AtomicBool::new(false),
    )
});

pub(super) fn append(args: &mut Vec<String>, work: &Path, cancel: &AtomicBool) {
    let font = &*MONOSPACE;
    if !cancel.load(std::sync::atomic::Ordering::Relaxed) {
        append_file(args, font.as_deref(), work);
    }
}

fn resolve(query: &[String], cancel: &AtomicBool) -> Option<PathBuf> {
    let path = PathBuf::from(printtool::run(query, cancel, printtool::QUERY_WAIT).ok()?);
    (path.is_absolute() && regfile::open_if_regular(&path, 0).is_ok()).then_some(path)
}

fn append_file(args: &mut Vec<String>, font: Option<&Path>, work: &Path) {
    let staged = font.and_then(|font| {
        let mut source = regfile::open_if_regular(font, 0).ok()?;
        let path = work.join("label-font");
        let mut output = std::fs::File::create(&path).ok()?;
        std::io::copy(&mut source, &mut output).ok()?;
        Some(path)
    });
    if let Some(path) = staged {
        // User-installed fonts live outside /usr, so copy into the existing private image jail.
        args.extend([
            "-font".into(),
            flea::ui_logic_print_convert::output::magick_font(&path),
        ]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn tui_combine_fontconfig_failure_omits_font() {
        let root = TestDir::new("tui-combine-font-fallback");
        for query in [
            vec!["/nonexistent-flea-fc-match".into()],
            vec![
                "/bin/sh".into(),
                "-c".into(),
                "printf /usr/bin/sh; exit 1".into(),
            ],
            vec!["/bin/true".into()],
            vec!["/bin/echo".into(), "relative-font.ttf".into()],
        ] {
            let font = resolve(&query, &AtomicBool::new(false));
            assert!(font.is_none());
            let mut args = vec!["magick".into()];
            append_file(&mut args, font.as_deref(), root.path());
            assert_eq!(args, ["magick"]);
        }
    }

    #[test]
    fn tui_combine_font_file_is_staged_and_missing_file_falls_back() {
        let root = TestDir::new("tui-combine-font-file");
        let source = root.join("user font.ttf");
        std::fs::write(&source, b"font bytes").unwrap();
        let resolved = resolve(
            &[
                "/usr/bin/printf".into(),
                "%s".into(),
                source.to_string_lossy().into_owned(),
            ],
            &AtomicBool::new(false),
        );
        assert_eq!(resolved.as_deref(), Some(source.as_path()));
        let mut args = Vec::new();
        append_file(&mut args, resolved.as_deref(), root.path());
        assert_eq!(args[0], "-font");
        assert_eq!(Path::new(&args[1]), root.join("label-font"));
        assert_eq!(std::fs::read(&args[1]).unwrap(), b"font bytes");
        let mut fallback = Vec::new();
        append_file(&mut fallback, Some(&root.join("missing.ttf")), root.path());
        assert!(fallback.is_empty());
    }
}
