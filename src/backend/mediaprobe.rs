// Duration, pixels and sample rate for a media row, which the preview column names and no listing
// carries. ffprobe reads the container's own header; nothing is decoded and nothing is written.
use crate::backend::sandbox;
use flea::bytename::ExactText as _;
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const SIGKILL: i32 = 9;
// A header read on a local file is milliseconds; this is a runaway, not a slow file.
const PROBE_LIMIT: Duration = Duration::from_secs(5);
// Only the path where stdout closed before the child exited polls at all, so this is that case's step and not the common one's.
const REAP_STEP: Duration = Duration::from_millis(1);

// The one lock the watchdog's kill and the probe's reap both take: waitpid frees the pid, so a kill on the other side of an unsynchronised reap can name a process group the kernel has already handed to somebody else.
#[derive(Default)]
struct Watch {
    reaped: Mutex<bool>,
    wake: Condvar,
}

#[derive(Default, PartialEq, Debug)]
pub struct Media {
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub sample_rate: u32,
    // ffprobe's own codec names for the first video and first audio stream, empty when the file has none.
    pub video_codec: String,
    pub audio_codec: String,
    // The container's overall bits per second, zero when the file does not say.
    pub bit_rate: u64,
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
        "format=duration,bit_rate",
        "-show_entries",
        "stream=width,height,sample_rate,codec_name,codec_type:stream_side_data=rotation",
        "-of",
        "default=noprint_wrappers=1",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain(std::iter::once(path.exact_text().to_string()))
    .collect()
}

pub fn probe(path: &Path) -> Media {
    let inner = argv(path);
    // The same jail the thumbnail pipeline uses; with no bwrap on PATH the probe is simply skipped.
    if !sandbox::available() {
        return Media::default();
    }
    let mut full = sandbox::wrap_readonly(&inner, path);
    sandbox::add_status(&mut full, inner.len());
    let mut jailed = match crate::backend::jail::spawn_jailed(&full, |cmd| {
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::null());
    }) {
        Ok(j) => j,
        Err(_) => return Media::default(),
    };
    // prlimit's --cpu cannot bound a probe blocked in open(2) or read(2), because a blocked process burns no CPU at all.
    let watch = Arc::new(Watch::default());
    let watchdog = watchdog(jailed.child.id() as i32, Arc::clone(&jailed.sandbox_pid), Arc::clone(&watch));
    let mut stdout = Vec::new();
    if let Some(mut pipe) = jailed.child.stdout.take() {
        let _ = pipe.read_to_end(&mut stdout);
    }
    // The reap runs with the watchdog still armed, because end of stdout says every writer closed it and not that the child exited, and standing the watchdog down first is what leaves that case unbounded.
    let status = reap(&mut jailed.child, &watch);
    let _ = watchdog.join();
    match status {
        // A probe that did not exit zero was not answering about this file, so its stdout is not a measurement.
        Ok(s) if s.success() => parse(&String::from_utf8_lossy(&stdout)),
        _ => Media::default(),
    }
}

// One thread, one timed wait, one signal, waking the instant the probe is reaped rather than at the end of a sleep; the same shape metareq.rs uses to bound an archive listing.
fn watchdog(pid: i32, sandbox_pid: Arc<std::sync::atomic::AtomicI32>, watch: Arc<Watch>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let deadline = Instant::now() + PROBE_LIMIT;
        let mut reaped = watch.reaped.lock().unwrap();
        while !*reaped {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            reaped = watch.wake.wait_timeout(reaped, left).unwrap().0;
        }
        // Signalled with the lock still held, so no reap can free this pid underneath it.
        if !*reaped {
            // The group ends bwrap, and the sandbox pid ends the jailed tool --new-session hid from it.
            crate::backend::jail::kill_sandbox(&sandbox_pid, pid);
            let _ = crate::procsignal::signal_group(pid, SIGKILL);
        }
    })
}

// try_wait rather than wait, because the lock the watchdog kills under cannot be held across a blocking reap.
fn reap(child: &mut std::process::Child, watch: &Watch) -> std::io::Result<std::process::ExitStatus> {
    loop {
        {
            let mut reaped = watch.reaped.lock().unwrap();
            match child.try_wait() {
                Ok(Some(status)) => {
                    *reaped = true;
                    watch.wake.notify_all();
                    return Ok(status);
                }
                // The watchdog stays armed on an error, because nothing was reaped and the child still has to be ended by somebody.
                Err(e) => return Err(e),
                Ok(None) => {}
            }
        }
        std::thread::sleep(REAP_STEP);
    }
}

// Sample input, one key=value per line, streams before the format block; ffprobe prints a stream's codec_name before its codec_type:
//   codec_name=h264
//   codec_type=video
//   width=1920
//   height=1080
//   rotation=-90
//   duration=10.000000
//   bit_rate=8100000
pub fn parse(text: &str) -> Media {
    let mut m = Media::default();
    // A file can carry several streams; the video one owns the pixels and the audio one the rate.
    let mut in_audio = false;
    // A phone stores a sideways recording with a quarter turn that the poster and the player both apply, so the pixels are given as shown.
    let mut turned = false;
    // The name printed ahead of the type line that says what stream it names.
    let mut name = "";
    for line in text.lines() {
        let (key, value) = match line.split_once('=') {
            Some(pair) => pair,
            None => continue,
        };
        match key {
            "codec_name" => name = if value == "N/A" { "" } else { value },
            "codec_type" => {
                in_audio = value == "audio";
                // The first video and the first audio stream name the file; a cover picture or a second track does not.
                let slot = match value {
                    "video" => Some(&mut m.video_codec),
                    "audio" => Some(&mut m.audio_codec),
                    _ => None,
                };
                if let Some(slot) = slot.filter(|slot| slot.is_empty()) {
                    *slot = name.to_string();
                }
                name = "";
            }
            // A video stream's dimensions win; an audio stream carries none, so nothing is overwritten.
            "width" if !in_audio => {
                m.width = value.parse().unwrap_or(0);
                // These pixels replace any earlier stream's, so only their own turn, printed after them, applies; a data stream prints none.
                turned = false;
            }
            "height" if !in_audio => m.height = value.parse().unwrap_or(0),
            "rotation" if !in_audio => turned = quarter_turn(value),
            "sample_rate" => m.sample_rate = value.parse().unwrap_or(0),
            "duration" => m.duration_ms = seconds_to_ms(value),
            "bit_rate" => m.bit_rate = value.parse().unwrap_or(0),
            _ => {}
        }
    }
    if turned {
        std::mem::swap(&mut m.width, &mut m.height);
    }
    m
}

// ffprobe prints the display matrix's turn in degrees, either sign.
fn quarter_turn(value: &str) -> bool {
    value.parse::<f64>().is_ok_and(|degrees| (degrees.round() as i64).rem_euclid(180) == 90)
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
        assert_eq!(m, Media { duration_ms: 10000, width: 1920, height: 1080, ..Media::default() });
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
    fn a_quarter_turned_video_reports_its_pixels_as_shown() {
        // A phone's MOV: the video, its audio, then a timecode track, as ffprobe 7.1 prints one.
        let m = parse("codec_type=video\nwidth=1920\nheight=1080\nrotation=-90\ncodec_type=audio\nsample_rate=48000\ncodec_type=data\nduration=10.000000\n");
        assert_eq!((m.width, m.height), (1080, 1920));
        assert_eq!(parse("codec_type=video\nwidth=640\nheight=360\nrotation=270\n").width, 360);
        assert_eq!(parse("codec_type=video\nwidth=640\nheight=360\nrotation=180\n").width, 640, "a half turn keeps the sides");
        assert_eq!(parse("codec_type=video\nwidth=640\nheight=360\nrotation=90\ncodec_type=video\nwidth=320\nheight=240\n").width, 320);
    }

    #[test]
    fn an_audio_only_file_reports_a_rate_and_no_pixels() {
        let m = parse("codec_type=audio\nsample_rate=44100\nduration=245.000000\n");
        assert_eq!(m, Media { duration_ms: 245000, sample_rate: 44100, ..Media::default() });
    }

    #[test]
    fn junk_and_missing_values_are_zeroes_rather_than_a_panic() {
        assert!(parse("").is_empty());
        assert!(parse("no equals sign at all\n").is_empty());
        assert!(parse("duration=N/A\ncodec_type=video\nwidth=N/A\n").is_empty());
        assert_eq!(parse("duration=-3.0\n").duration_ms, 0, "a negative duration is not a duration");
        assert_eq!(parse("duration=0.000000\n").duration_ms, 0);
    }

    // Sample input as ffprobe 7.1 prints one: codec_name leads its stream's block, and the format block's bit_rate comes last.
    const MP4: &str = "codec_name=h264\ncodec_type=video\nwidth=1920\nheight=1080\n\
                       codec_name=aac\ncodec_type=audio\nsample_rate=48000\n\
                       duration=10.000000\nbit_rate=8100000\n";

    #[test]
    fn the_codecs_and_the_bit_rate_come_off_the_same_probe_text() {
        let m = parse(MP4);
        assert_eq!((m.video_codec.as_str(), m.audio_codec.as_str(), m.bit_rate), ("h264", "aac", 8_100_000));
        assert_eq!((m.width, m.height, m.sample_rate, m.duration_ms), (1920, 1080, 48000, 10000), "the old answers are unmoved");
    }

    #[test]
    fn a_codec_name_belongs_to_the_stream_whose_type_follows_it() {
        // An audio track first and a data track last: each name goes to its own type, and a data stream's goes nowhere.
        let m = parse("codec_name=opus\ncodec_type=audio\ncodec_name=vp9\ncodec_type=video\nwidth=640\nheight=360\ncodec_name=bin_data\ncodec_type=data\n");
        assert_eq!((m.video_codec.as_str(), m.audio_codec.as_str()), ("vp9", "opus"));
        // Only the first video and first audio stream name the file: a cover picture after the real video does not.
        let m = parse("codec_name=hevc\ncodec_type=video\ncodec_name=mjpeg\ncodec_type=video\ncodec_name=aac\ncodec_type=audio\ncodec_name=ac3\ncodec_type=audio\n");
        assert_eq!((m.video_codec.as_str(), m.audio_codec.as_str()), ("hevc", "aac"));
        assert_eq!(parse("codec_name=aac\n").audio_codec, "", "a name with no type after it is not a stream");
    }

    #[test]
    fn a_bit_rate_that_is_not_a_number_is_zero() {
        assert_eq!(parse("bit_rate=N/A\n").bit_rate, 0);
        assert_eq!(parse("bit_rate=-4\n").bit_rate, 0);
        assert_eq!(parse("bit_rate=640000\n").bit_rate, 640_000);
        assert_eq!(parse("codec_name=N/A\ncodec_type=video\n").video_codec, "", "N/A names no codec");
    }

    #[test]
    fn the_argv_asks_for_the_codec_name_and_the_format_bit_rate_in_the_one_probe() {
        let a = argv(Path::new("/home/gm/clip.mp4"));
        assert!(a.iter().any(|s| s == "format=duration,bit_rate"), "one format entry, no second process");
        assert!(a.iter().any(|s| s.starts_with("stream=") && s.split(':').next().unwrap().split(',').any(|f| f == "codec_name")));
        assert_eq!(a.iter().filter(|s| *s == "-show_entries").count(), 2, "no third entry list");
    }

    #[test]
    fn the_argv_names_ffprobe_and_ends_with_the_file() {
        let a = argv(Path::new("/home/gm/clip.mp4"));
        assert_eq!(a[0], "ffprobe");
        assert_eq!(a.last().unwrap(), "/home/gm/clip.mp4");
        assert!(a.contains(&"-show_entries".to_string()));
        assert!(a.iter().any(|s| s.ends_with(":stream_side_data=rotation")), "the turn decides which side is the width");
        // No decode and no write: the probe reads headers and prints key=value on stdout.
        assert!(!a.iter().any(|s| s == "-o" || s == "-f"));
    }
}
