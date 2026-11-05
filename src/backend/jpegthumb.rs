use super::child::{run_with_timeout, Ran};
use super::{extclass, imagesize, regfile, thumbbind, thumbs};
use std::fs::OpenOptions;
use std::io::{BufReader, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

pub(super) const PROGRAM: &str = "/usr/bin/vips";
const FAST_TIMEOUT: Duration = Duration::from_secs(2);
const MIN_BYTES: u64 = 768 * 1024;

// Both decoders share the usual thumbnail deadline.
pub(super) fn run(input: &Path, output: &Path, size: u32, fallback: &[String]) -> Ran {
    let started = Instant::now();
    generate(input, output, size).unwrap_or_else(|| {
        run_with_timeout(
            fallback,
            thumbs::JOB_TIMEOUT.saturating_sub(started.elapsed()),
        )
    })
}

// Try shrink-on-load in the normal thumbnail jail. None gives the original decoder the job.
fn generate(input: &Path, output: &Path, size: u32) -> Option<Ran> {
    let args = command(input, output, size)?;
    if !extclass::classify(input).is_empty() || !is_large_rgb_jpeg(input) {
        return None;
    }
    let jailed = thumbbind::wrap(&args, input, output)?;
    let ran = run_with_timeout(&jailed, FAST_TIMEOUT);
    if matches!(ran, Ran::Succeeded) && thumbs::wrote_something(output) {
        return Some(ran);
    }
    // A partial write must not become Glycin's output if Glycin exits successfully without writing.
    if OpenOptions::new()
        .write(true)
        .truncate(true)
        .custom_flags(crate::oflags::O_NOFOLLOW | crate::oflags::O_NONBLOCK)
        .open(output)
        .is_err()
    {
        return Some(Ran::NotStarted);
    }
    None
}

fn is_large_rgb_jpeg(input: &Path) -> bool {
    let Ok((file, metadata)) = regfile::open_if_regular_with_meta(input, crate::oflags::O_NOFOLLOW)
    else {
        return false;
    };
    if metadata.len() < MIN_BYTES {
        return false;
    }
    let mut file = BufReader::new(file);
    let mut signature = [0; 2];
    // Keep CMYK and other channel layouts on Glycin: the decoders can interpret their colors differently.
    file.read_exact(&mut signature).is_ok()
        && signature == [0xff, 0xd8]
        && imagesize::jpeg_frame(&mut file).is_some_and(|(_, components)| components == 3)
}

fn command(input: &Path, output: &Path, size: u32) -> Option<Vec<String>> {
    let input = input.to_str()?;
    let output = output.to_str()?;
    // libvips reads brackets in filenames as loader or saver options.
    if input.contains(['[', ']']) || output.contains(['[', ']']) {
        return None;
    }
    Some(vec![
        PROGRAM.to_string(),
        "--vips-concurrency=1".to_string(),
        "thumbnail".to_string(),
        input.to_string(),
        format!("{output}[compression=6,filter=all,strip]"),
        size.to_string(),
        format!("--height={size}"),
        "--size=down".to_string(),
        "--output-profile=srgb".to_string(),
        "--fail-on=warning".to_string(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::io::Write;

    #[test]
    fn only_large_three_component_jpeg_inputs_take_the_fast_path() {
        let directory = TestDir::new("fast-jpeg-signature");
        let path = directory.join("example.jpg");
        let mut header = [0xff, 0xd8, 0xff, 0xc0, 0, 17, 8, 1, 0, 1, 0, 3];
        std::fs::write(&path, header).unwrap();
        assert!(!is_large_rgb_jpeg(&path));
        let file = OpenOptions::new().write(true).open(&path).unwrap();
        file.set_len(MIN_BYTES).unwrap();
        assert!(is_large_rgb_jpeg(&path));
        header[11] = 4;
        (&file).write_all(&header).unwrap();
        assert!(!is_large_rgb_jpeg(&path));
        assert!(!is_large_rgb_jpeg(&directory.join("missing.jpg")));
    }

    #[test]
    fn thumbnailer_options_cannot_come_from_filenames() {
        let input = Path::new("/tmp/a b.jpg");
        let output = Path::new("/tmp/output.png");
        assert_eq!(command(input, output, 256).unwrap()[3], "/tmp/a b.jpg");
        assert!(command(Path::new("/tmp/a[1].jpg"), output, 256).is_none());
        assert!(command(input, Path::new("/tmp/[photos]/out.png"), 256).is_none());
    }
}
