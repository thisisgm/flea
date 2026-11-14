use super::child::{run_with_timeout, Ran};
use super::{extclass, imagesize, iomount, mountinfo, netfs, regfile, thumbbind, thumbs};
use std::fs::OpenOptions;
use std::io::{BufReader, Read};
use std::os::unix::{fs::OpenOptionsExt, process::CommandExt};
use std::path::Path;
use std::time::{Duration, Instant};

pub(super) const PROGRAM: &str = "/usr/bin/vips";
pub(crate) const CHILD_MODE: &str = "--jpeg-thumbnail";
const FAST_TIMEOUT: Duration = Duration::from_secs(2);
const MIN_BYTES: u64 = 768 * 1024;

/// Run the optional JPEG child and the existing decoder within the same job budget.
pub(super) fn run(
    input: &Path,
    output: &Path,
    size: u32,
    source_size: Option<u64>,
    fallback: &[String],
    program: &dyn Fn(&str) -> String,
) -> Ran {
    if source_size.is_some_and(|bytes| bytes < MIN_BYTES) {
        return run_with_timeout(fallback, thumbs::JOB_TIMEOUT);
    }
    let deadline = Instant::now() + thumbs::JOB_TIMEOUT;
    generate(input, output, size, program).unwrap_or_else(|| {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            Ran::NotStarted
        } else {
            run_with_timeout(fallback, remaining)
        }
    })
}

/// Start the jailed JPEG attempt without opening or probing the source on the pool thread.
fn generate(
    input: &Path,
    output: &Path,
    size: u32,
    program: &dyn Fn(&str) -> String,
) -> Option<Ran> {
    let (input_arg, output_arg) = path_arguments(input, output)?;
    if !eligible_mount(input) {
        return None;
    }
    let executable = std::env::current_exe()
        .ok()?
        .into_os_string()
        .into_string()
        .ok()?;
    let args = vec![
        program(&executable),
        CHILD_MODE.into(),
        input_arg.into(),
        output_arg.into(),
        size.to_string(),
    ];
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

/// Filter mounts using kernel metadata without making a filesystem call on the source.
fn eligible_mount(input: &Path) -> bool {
    let body = iomount::mount_body();
    let Some(entry) = mountinfo::mount_entry_in(input, &body) else {
        return false;
    };
    !netfs::can_wedge(&entry.fstype)
        && extclass::classify_entry_with_magic(input, Some(&entry), None).is_empty()
}

/// Probe the JPEG and replace this process with vips inside the same thumbnail jail.
pub(crate) fn run_child(args: &[String]) -> i32 {
    let [input, output, size] = args else {
        return 1;
    };
    let (input, output) = (Path::new(input), Path::new(output));
    let Ok(size) = size.parse::<u32>() else {
        return 1;
    };
    let Some(args) = command(input, output, size) else {
        return 1;
    };
    if !is_large_rgb_jpeg(input) {
        return 1;
    }
    let _ = std::process::Command::new(PROGRAM).args(&args[1..]).exec();
    1
}

/// Admit large regular JPEGs with the three-component color layout the fast decoder supports.
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

/// Refuse paths that vips would interpret as loader or saver options.
fn path_arguments<'a>(input: &'a Path, output: &'a Path) -> Option<(&'a str, &'a str)> {
    if !input.is_absolute() || !output.is_absolute() {
        return None;
    }
    let input = input.to_str()?;
    let output = output.to_str()?;
    if input.contains(['[', ']']) || output.contains(['[', ']']) {
        return None;
    }
    Some((input, output))
}

/// Build the fixed vips command for one bounded thumbnail.
fn command(input: &Path, output: &Path, size: u32) -> Option<Vec<String>> {
    let (input, output) = path_arguments(input, output)?;
    if size == 0 || size > thumbs::THUMB_SIZE {
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

    /// Verify that the JPEG probe accepts only large three-component files.
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

    /// Verify that filenames cannot inject vips loader or saver options.
    #[test]
    fn thumbnailer_options_cannot_come_from_filenames() {
        let input = Path::new("/tmp/a b.jpg");
        let output = Path::new("/tmp/output.png");
        assert_eq!(command(input, output, 256).unwrap()[3], "/tmp/a b.jpg");
        assert!(command(Path::new("/tmp/a[1].jpg"), output, 256).is_none());
        assert!(command(input, Path::new("/tmp/[photos]/out.png"), 256).is_none());
    }
}
