//! The bench-only media kit: the headless mpv arm, and a window that records presentation, CPU and memory.
mod headless;
mod window;

use std::ffi::c_int;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const FLAG: &str = "--media-kit";
/// The file every scratch root holds, so nothing outside one is ever written or removed.
pub const MARKER: &str = ".flea-pv3-kit-root";

/// Runs the kit when the arguments ask for it, returning its exit status.
pub fn dispatch(args: &[String]) -> Option<i32> {
    let (first, rest) = args.split_first()?;
    if first != FLAG {
        return None;
    }
    let result = match rest.split_first() {
        Some((mode, words)) if mode == "headless" => headless::run(words),
        Some((mode, words)) if mode == "window" => window::run(words),
        _ => Err("usage: --media-kit headless [--seconds N] [--rounds N] | window --fifo PATH --records PATH".into()),
    };
    Some(match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("flea-media-kit: {error}");
            1
        }
    })
}

/// A fresh marked root under the working directory's `.superpowers/g2-pv3`, made by mktemp.
pub fn scratch() -> Result<PathBuf, String> {
    let parent = std::env::current_dir()
        .map_err(|error| error.to_string())?
        .join(".superpowers/g2-pv3");
    std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
    let output = Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("kit.XXXXXX"))
        .output()
        .map_err(|error| error.to_string())?;
    let root = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    if !output.status.success()
        || !root.is_absolute()
        || !root.starts_with(&parent)
        || root == parent
    {
        return Err("mktemp gave no usable root".into());
    }
    std::fs::write(root.join(MARKER), b"pv3\n").map_err(|error| error.to_string())?;
    Ok(root)
}

/// A path inside a marked root that does not exist yet, or a refusal.
pub fn fresh(root: &Path, name: &str) -> Result<PathBuf, String> {
    let path = root.join(name);
    if !root.is_absolute()
        || !root.join(MARKER).is_file()
        || path.exists()
        || path.parent() != Some(root)
    {
        return Err(format!(
            "refused {}: not a fresh path in a marked root",
            path.display()
        ));
    }
    Ok(path)
}

/// One testsrc2 clip with a tone, so a decoder's audio path is exercised alongside its video.
pub fn clip(root: &Path, codec: &str, size: &str, seconds: u32) -> Result<PathBuf, String> {
    let extension = if codec == "h264" { "mp4" } else { "webm" };
    let file = fresh(root, &format!("clip-{codec}-{size}.{extension}"))?;
    let mut command = Command::new("ffmpeg");
    command.args(["-nostdin", "-v", "error", "-n", "-f", "lavfi", "-i"]);
    command.arg(format!("testsrc2=size={size}:rate=30"));
    command.args([
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=48000",
        "-t",
    ]);
    command.arg(seconds.to_string());
    command.args(["-pix_fmt", "yuv420p"]);
    if codec == "h264" {
        command.args([
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "28",
            "-c:a",
            "aac",
        ]);
    } else {
        command.args([
            "-c:v",
            "libvpx",
            "-deadline",
            "realtime",
            "-cpu-used",
            "8",
            "-b:v",
            "8M",
            "-c:a",
            "libvorbis",
        ]);
    }
    let status = command
        .arg(&file)
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err(format!("ffmpeg exited {status} for {}", file.display()));
    }
    Ok(file)
}

/// A PCM tone with no picture, the audio-only arm.
pub fn tone(root: &Path, seconds: u32) -> Result<PathBuf, String> {
    let file = fresh(root, "tone.wav")?;
    let status = Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-n",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-t",
        ])
        .arg(seconds.to_string())
        .args(["-c:a", "pcm_s16le"])
        .arg(&file)
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err(format!("ffmpeg exited {status} for the tone"));
    }
    Ok(file)
}

extern "C" {
    fn sysconf(name: c_int) -> i64;
}

/// Clock ticks per second, the unit /proc reports CPU time in.
pub fn ticks_per_second() -> f64 {
    const SC_CLK_TCK: c_int = 2;
    // SAFETY: sysconf takes a constant and has no other effect.
    let ticks = unsafe { sysconf(SC_CLK_TCK) };
    if ticks > 0 {
        ticks as f64
    } else {
        100.
    }
}

fn stat(pid: u32) -> Option<(u32, u64)> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, fields) = text.rsplit_once(')')?;
    let fields: Vec<&str> = fields.split_whitespace().collect();
    let parent = fields.get(1)?.parse().ok()?;
    let user: u64 = fields.get(11)?.parse().ok()?;
    let system: u64 = fields.get(12)?.parse().ok()?;
    Some((parent, user + system))
}

/// The root and every process under it.
pub fn tree(root: u32) -> Vec<u32> {
    let mut all = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            if let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
            {
                if let Some((parent, _)) = stat(pid) {
                    all.push((pid, parent));
                }
            }
        }
    }
    let mut kept = vec![root];
    let mut index = 0;
    while index < kept.len() {
        let parent = kept[index];
        kept.extend(
            all.iter()
                .filter(|(_, up)| *up == parent)
                .map(|(pid, _)| *pid),
        );
        index += 1;
    }
    kept.retain(|pid| stat(*pid).is_some());
    kept
}

/// User plus system CPU of a whole tree, in milliseconds.
pub fn cpu_ms(pids: &[u32]) -> f64 {
    let ticks: u64 = pids
        .iter()
        .filter_map(|pid| stat(*pid))
        .map(|(_, ticks)| ticks)
        .sum();
    ticks as f64 * 1000. / ticks_per_second()
}

/// Proportional set size of a whole tree, in KiB, and how many processes answered.
pub fn pss_kib(pids: &[u32]) -> (u64, usize) {
    let mut total = 0;
    let mut read = 0;
    for pid in pids {
        let Ok(text) = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")) else {
            continue;
        };
        let pss = text
            .lines()
            .find_map(|line| line.strip_prefix("Pss:"))
            .and_then(|rest| {
                rest.trim()
                    .trim_end_matches("kB")
                    .trim()
                    .parse::<u64>()
                    .ok()
            });
        if let Some(pss) = pss {
            total += pss;
            read += 1;
        }
    }
    (total, read)
}

/// Lowest, median and highest of a sample, for the summary's ranges.
pub fn range(values: &mut [f64]) -> Option<(f64, f64, f64)> {
    values.sort_by(f64::total_cmp);
    let (first, last) = (values.first()?, values.last()?);
    Some((*first, values[values.len() / 2], *last))
}
