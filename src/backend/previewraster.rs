//! A transient, sized preview through the existing thumbnail sandbox, without replacing the shared 256 px cache.
use super::{
    aliases::Aliases,
    child::{run_with_timeout, Ran},
    mime::Db,
    rastertemp::{preview_dir, MAX_OUTPUT_BYTES},
    thumbargv, thumbbind,
    thumbspec::Thumbnailers,
    thumbwrite,
    workerlink::worker_shape,
};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::OnceLock,
};

struct Tables {
    aliases: Aliases,
    mime: Db,
    specs: Thumbnailers,
}
fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let aliases = Aliases::load();
        let specs = Thumbnailers::load(&aliases);
        Tables {
            aliases,
            mime: Db::load(),
            specs,
        }
    })
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        if self.0.is_absolute() && self.0.file_name().is_some() {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

/// The caller bounds concurrency and decodes the returned PNG under its existing pixel bound.
pub fn generate(path: &Path, size: u32) -> Option<Vec<u8>> {
    let edge = flea::ui_logic_preview_core::pixels::MAX_RASTER_EDGE;
    if !(1..=edge).contains(&size) || !path.is_absolute() || !super::sandbox::available() {
        return None;
    }
    let tables = tables();
    let name = path.file_name()?.to_str()?;
    let mime = tables.mime.lookup(name)?;
    let spec = tables.specs.for_mime(mime, &tables.aliases)?;
    if tables.specs.raw_route(mime, &tables.aliases).is_some() {
        let copy = super::rawpreview::ensure_in(&super::rawpreview::cache_dir(), path)?;
        return read(&copy.file);
    }
    let dir = preview_dir();
    if !dir.is_absolute() {
        return None;
    }
    std::fs::create_dir_all(&dir).ok()?;
    let output = Temporary(thumbwrite::exclusive_temp(&dir)?);
    let requested = requested_size(mime, spec, size);
    let (input, argv) = thumbargv::argv(spec, path, &output.0, requested)?;
    let full = thumbbind::wrap(&argv, &input, &output.0)?;
    let ran = run_with_timeout(&full, super::thumbs::JOB_TIMEOUT);
    if !matches!(ran, Ran::Succeeded) {
        return None;
    }
    read(&output.0)
}

fn read(path: &Path) -> Option<Vec<u8>> {
    let file = super::regfile::open_regular(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_OUTPUT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (!bytes.is_empty() && bytes.len() <= MAX_OUTPUT_BYTES).then_some(bytes)
}

/// Private launcher mode used only by physical preview refinement.
pub fn run(args: &[String]) -> i32 {
    use std::io::Write;
    let [path, size] = args else {
        return 2;
    };
    let Some(size) = size.parse::<u32>().ok() else {
        return 2;
    };
    let Some(bytes) = generate(Path::new(path), size) else {
        return 1;
    };
    if std::io::stdout().lock().write_all(&bytes).is_ok() {
        0
    } else {
        1
    }
}

fn requested_size(mime: &str, spec: &super::thumbspec::Spec, size: u32) -> u32 {
    if mime.starts_with("video/") && worker_shape(spec).is_some() {
        0
    } else {
        size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_quality_video_skips_thumbnail_downscaling() {
        let spec = super::super::thumbspec::Spec {
            exec: vec![
                "ffmpegthumbnailer".into(),
                "-i".into(),
                "%i".into(),
                "-s".into(),
                "%s".into(),
                "-o".into(),
                "%o".into(),
            ],
        };
        assert_eq!(requested_size("video/mp4", &spec, 512), 0);
        assert_eq!(requested_size("image/png", &spec, 512), 512);
    }
    #[test]
    fn preview_quality_helper_refuses_invalid_requests_before_opening() {
        assert_eq!(run(&[]), 2);
        assert_eq!(run(&["/unopened".into(), "nan".into()]), 2);
        assert!(generate(Path::new("relative"), 512).is_none());
        assert!(generate(Path::new("/unopened"), 0).is_none());
        assert!(generate(Path::new("/unopened"), 8193).is_none());
    }
}
