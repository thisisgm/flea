// File conversion, delegated to ImageMagick for images and PDF, and to FFmpeg for audio and
// video. One tool does each half, keeping hard rule 1's single-responsibility design intact.
use std::path::Path;

const MAGICK: &str = "magick";
const FFMPEG: &str = "ffmpeg";

fn is_on_path(binary: &str) -> bool {
    let path = std::env::var("PATH").unwrap_or_default();
    path.split(':')
        .filter(|d| !d.is_empty())
        .any(|d| Path::new(d).join(binary).is_file())
}

pub fn available() -> bool {
    is_on_path(MAGICK) || is_on_path(FFMPEG)
}

pub fn is_media(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        matches!(
            ext.to_ascii_lowercase().as_str(),
            "mp4" | "mov" | "mkv" | "webm" | "avi" | "wmv" | "gif" |
            "mp3" | "m4a" | "wav" | "flac" | "ogg" | "opus" | "aiff" | "wma"
        )
    } else {
        false
    }
}

pub fn available_for(input: &Path, dest: &Path) -> bool {
    if is_media(input) || is_media(dest) {
        is_on_path(FFMPEG)
    } else {
        is_on_path(MAGICK)
    }
}

// Magick reads the codec off dest's extension; FFmpeg infers the container and codec similarly.
pub fn argv(input: &Path, dest: &Path, strip: bool) -> Vec<String> {
    if (is_media(input) || is_media(dest)) && is_on_path(FFMPEG) {
        let mut a = vec![
            FFMPEG.to_string(),
            "-y".to_string(),
            "-i".to_string(),
            input.to_string_lossy().to_string(),
        ];
        if strip {
            a.push("-map_metadata".to_string());
            a.push("-1".to_string());
            a.push("-map_metadata:s".to_string());
            a.push("-1".to_string());
            a.push("-map_chapters".to_string());
            a.push("-1".to_string());
        }
        a.push(dest.to_string_lossy().to_string());
        a
    } else {
        let mut a = vec![MAGICK.to_string(), input.to_string_lossy().to_string()];
        if strip {
            a.push("-strip".to_string());
        }
        a.push(dest.to_string_lossy().to_string());
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_argv_names_magick_and_carries_strip_only_when_it_was_asked_for() {
        let plain = argv(Path::new("/x/a.heic"), Path::new("/x/a.jpg"), false);
        assert_eq!(plain, vec!["magick", "/x/a.heic", "/x/a.jpg"]);
        let stripped = argv(Path::new("/x/a.heic"), Path::new("/x/a.jpg"), true);
        assert_eq!(stripped, vec!["magick", "/x/a.heic", "-strip", "/x/a.jpg"]);
        // No format flag anywhere: the destination's own extension is what magick reads.
        assert!(!plain.iter().any(|s| s == "-format" || s == "-define"));
    }

    #[test]
    fn the_argv_names_ffmpeg_for_media_formats_when_ffmpeg_is_available() {
        if is_on_path(FFMPEG) {
            let media = argv(Path::new("/x/a.mov"), Path::new("/x/a.mp4"), false);
            assert_eq!(media, vec!["ffmpeg", "-y", "-i", "/x/a.mov", "/x/a.mp4"]);
            let audio = argv(Path::new("/x/a.mp4"), Path::new("/x/a.mp3"), false);
            assert_eq!(audio, vec!["ffmpeg", "-y", "-i", "/x/a.mp4", "/x/a.mp3"]);
            let stripped = argv(Path::new("/x/a.mov"), Path::new("/x/a.mp4"), true);
            assert_eq!(stripped, vec![
                "ffmpeg", "-y", "-i", "/x/a.mov",
                "-map_metadata", "-1",
                "-map_metadata:s", "-1",
                "-map_chapters", "-1",
                "/x/a.mp4",
            ]);
        }
    }

    #[test]
    fn format_aware_availability_requires_ffmpeg_for_media_and_magick_for_images() {
        assert_eq!(available_for(Path::new("/x/a.mov"), Path::new("/x/a.mp4")), is_on_path(FFMPEG));
        assert_eq!(available_for(Path::new("/x/a.mp4"), Path::new("/x/a.mp3")), is_on_path(FFMPEG));
        assert_eq!(available_for(Path::new("/x/a.heic"), Path::new("/x/a.jpg")), is_on_path(MAGICK));
        assert_eq!(available_for(Path::new("/x/a.pdf"), Path::new("/x/a.png")), is_on_path(MAGICK));
    }
}
