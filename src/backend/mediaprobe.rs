// Duration, pixels and sample rate for a media row, which the preview column names and no listing
// carries. ffprobe reads the container's own header; nothing is decoded and nothing is written.
use crate::backend::sandbox;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

// std offers no way to kill a child from another thread without owning it, so the signal is declared here rather than taking a crate, the same call metareq.rs makes for its archive watchdog.
extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

const SIGKILL: i32 = 9;
// A header read on a local file is milliseconds; this is a runaway, not a slow file.
const PROBE_LIMIT: Duration = Duration::from_secs(5);
// The watchdog's own wake-up, so the deadline is honoured to a twentieth of a second.
const WATCHDOG_STEP: Duration = Duration::from_millis(50);

#[derive(Default, PartialEq, Debug)]
pub struct Media {
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub sample_rate: u32,
}

impl Media {
    // Test-only: production reads the fields, and a zero in one of them is the answer, not an error.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        *self == Media::default()
    }
}

fn argv(path: &Path) -> Vec<String> {
    [
        "ffprobe",
        "-v",
        "error",
        "-show_entries",
        "format=duration",
        "-show_entries",
        "stream=width,height,sample_rate,codec_type",
        "-of",
        "default=noprint_wrappers=1",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain(std::iter::once(path.to_string_lossy().to_string()))
    .collect()
}

pub fn probe(path: &Path) -> Media {
    let inner = argv(path);
    // The same jail the thumbnail pipeline uses; with no bwrap on PATH the probe is simply skipped.
    if !sandbox::available() {
        return Media::default();
    }
    let full = sandbox::wrap_readonly(&inner, path);
    let mut cmd = Command::new(&full[0]);
    cmd.args(&full[1..]);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::null());
    // Its own group, so the watchdog's kill(-pid) has a group to name: a child left in the backend's own group is not a group leader, and -pid would match nothing.
    cmd.process_group(0);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => return Media::default(),
    };
    // prlimit's --cpu cannot bound a probe blocked in open(2) or read(2), because a blocked process burns no CPU at all.
    let done = Arc::new(AtomicBool::new(false));
    let watchdog = watchdog(child.id() as i32, Arc::clone(&done));
    let mut stdout = Vec::new();
    if let Some(mut pipe) = child.stdout.take() {
        let _ = pipe.read_to_end(&mut stdout);
    }
    // The reap runs with the watchdog still armed, because end of stdout says every writer closed it and not that the child exited, and standing the watchdog down first is what leaves that case unbounded.
    let status = child.wait();
    done.store(true, Ordering::Relaxed);
    let _ = watchdog.join();
    match status {
        // A probe that did not exit zero was not answering about this file, so its stdout is not a measurement.
        Ok(s) if s.success() => parse(&String::from_utf8_lossy(&stdout)),
        _ => Media::default(),
    }
}

// One thread, one sleep, one signal, standing down the moment the probe is reaped; the same shape metareq.rs uses to bound an archive listing.
fn watchdog(pid: i32, done: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut waited = Duration::ZERO;
        while waited < PROBE_LIMIT {
            if done.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(WATCHDOG_STEP);
            waited += WATCHDOG_STEP;
        }
        // The group holds prlimit and the outer bwrap only: --new-session puts the sandboxed child in a session of its own, and what reaps that is --die-with-parent plus the PID namespace dying with it.
        if !done.load(Ordering::Relaxed) {
            unsafe { kill(-pid, SIGKILL) };
        }
    })
}

// Sample input, one key=value per line, streams before the format block:
//   codec_type=video
//   width=1920
//   height=1080
//   duration=10.000000
pub fn parse(text: &str) -> Media {
    let mut m = Media::default();
    // A file can carry several streams; the video one owns the pixels and the audio one the rate.
    let mut in_audio = false;
    for line in text.lines() {
        let (key, value) = match line.split_once('=') {
            Some(pair) => pair,
            None => continue,
        };
        match key {
            "codec_type" => in_audio = value == "audio",
            // A video stream's dimensions win; an audio stream carries none, so nothing is overwritten.
            "width" if !in_audio => m.width = value.parse().unwrap_or(0),
            "height" if !in_audio => m.height = value.parse().unwrap_or(0),
            "sample_rate" => m.sample_rate = value.parse().unwrap_or(0),
            "duration" => m.duration_ms = seconds_to_ms(value),
            _ => {}
        }
    }
    m
}

// ffprobe prints seconds with six decimals, and "N/A" for a stream it could not measure.
fn seconds_to_ms(value: &str) -> u64 {
    match value.parse::<f64>() {
        Ok(s) if s.is_finite() && s > 0.0 => (s * 1000.0).round() as u64,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_video_stream_reports_pixels_and_the_format_reports_duration() {
        let m = parse("codec_type=video\nwidth=1920\nheight=1080\nduration=10.000000\n");
        assert_eq!(m, Media { duration_ms: 10000, width: 1920, height: 1080, sample_rate: 0 });
    }

    #[test]
    fn an_audio_stream_reports_its_rate_and_never_overwrites_the_video_pixels() {
        let text = "codec_type=video\nwidth=1920\nheight=1080\n\
                    codec_type=audio\nwidth=N/A\nheight=N/A\nsample_rate=48000\n\
                    duration=125.400000\n";
        let m = parse(text);
        assert_eq!(m.width, 1920, "the audio stream's N/A must not clear the video's own answer");
        assert_eq!(m.height, 1080);
        assert_eq!(m.sample_rate, 48000);
        assert_eq!(m.duration_ms, 125400);
    }

    #[test]
    fn an_audio_only_file_reports_a_rate_and_no_pixels() {
        let m = parse("codec_type=audio\nsample_rate=44100\nduration=245.000000\n");
        assert_eq!(m, Media { duration_ms: 245000, width: 0, height: 0, sample_rate: 44100 });
    }

    #[test]
    fn junk_and_missing_values_are_zeroes_rather_than_a_panic() {
        assert!(parse("").is_empty());
        assert!(parse("no equals sign at all\n").is_empty());
        assert!(parse("duration=N/A\ncodec_type=video\nwidth=N/A\n").is_empty());
        assert_eq!(parse("duration=-3.0\n").duration_ms, 0, "a negative duration is not a duration");
        assert_eq!(parse("duration=0.000000\n").duration_ms, 0);
    }

    #[test]
    fn the_argv_names_ffprobe_and_ends_with_the_file() {
        let a = argv(Path::new("/home/gm/clip.mp4"));
        assert_eq!(a[0], "ffprobe");
        assert_eq!(a.last().unwrap(), "/home/gm/clip.mp4");
        assert!(a.contains(&"-show_entries".to_string()));
        // No decode and no write: the probe reads headers and prints key=value on stdout.
        assert!(!a.iter().any(|s| s == "-o" || s == "-f"));
    }
}
