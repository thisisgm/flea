// Image conversion, delegated to ImageMagick, which converts by the destination's own extension and
// strips every profile and comment in the same pass. One tool does both halves, so there is no
// second one here: that is hard rule 1's first question answered.
use std::path::Path;

const MAGICK: &str = "magick";
pub fn available() -> bool {
    let path = std::env::var("PATH").unwrap_or_default();
    path.split(':')
        .filter(|d| !d.is_empty())
        .any(|d| Path::new(d).join(MAGICK).is_file())
}

// No format argument: magick reads the codec off dest's extension, so a second source of truth for
// the same fact would only be something to disagree with.
pub fn argv(input: &Path, dest: &Path, strip: bool) -> Vec<String> {
    let mut a = vec![MAGICK.to_string(), input.to_string_lossy().to_string()];
    if strip {
        a.push("-strip".to_string());
    }
    a.push(flea::ui_logic_op4::output::magick_output(dest));
    a
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

}
