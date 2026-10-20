//! The headless arm: mpv decodes each clip in the real sandbox, timed from start to the first received picture.
use super::{clip, cpu_ms, pss_kib, range, scratch, tone, tree};
use crate::g2::pv3::engine::{Backend, Incoming, Playback, Request};
use crate::g2::pv3::sandbox::Sandbox;
use flea::ui_logic_pv3::frame::{Command, Event};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const SIZES: [(&str, u32, u32); 2] = [("1920x1080", 1920, 1080), ("3840x2160", 3840, 2160)];
const FIRST_FRAME: Duration = Duration::from_secs(15);

struct Arm {
    decoder: Box<dyn Playback>,
    frames: std::sync::Arc<crate::g2::pv3::engine::FrameSlot>,
    received: Receiver<(Instant, Incoming)>,
    started: Instant,
}

fn start(path: &Path, box_w: u32, box_h: u32) -> Result<Arm, String> {
    let sandbox = Sandbox::current().map_err(|error| error.to_string())?;
    let request = Request {
        path: path.into(),
        box_w,
        box_h,
        audio: false,
        autoplay: true,
    };
    let started = Instant::now();
    let (decoder, feed) = sandbox.start(&request)?;
    let (send, received) = mpsc::channel();
    let incoming = feed.incoming.clone();
    std::thread::spawn(move || {
        while let Ok(item) = incoming.recv_blocking() {
            if send.send((Instant::now(), item)).is_err() {
                break;
            }
        }
    });
    Ok(Arm {
        decoder,
        frames: feed.frames,
        received,
        started,
    })
}

impl Arm {
    /// Waits for the first record that `wanted` accepts, returning it with its time since start.
    fn wait(
        &self,
        limit: Duration,
        mut wanted: impl FnMut(&Incoming) -> bool,
    ) -> Result<(f64, Incoming), String> {
        let deadline = Instant::now() + limit;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let (at, item) = self
                .received
                .recv_timeout(left)
                .map_err(|_| "timed out".to_string())?;
            if let Incoming::Event(Event::Failed(failure)) = item {
                return Err(format!("failed {}", failure.word()));
            }
            if wanted(&item) {
                return Ok(((at - self.started).as_secs_f64() * 1000., item));
            }
            if item == Incoming::Closed {
                return Err("helper closed".into());
            }
            // A clip whose video decoder is missing loads as sound alone.
            if matches!(item, Incoming::Event(Event::Loaded { video: false, .. })) {
                return Err("loaded without a video stream".into());
            }
        }
    }

    fn pids(&self) -> Vec<u32> {
        self.decoder
            .pids()
            .first()
            .map_or_else(Vec::new, |root| tree(*root))
    }
}

fn field(name: &str, value: impl std::fmt::Display) -> String {
    format!("\"{name}\":{value}")
}

/// One clip: first picture, sustained CPU per picture, helper-tree PSS and, on round 0, the controls.
fn case(
    path: &Path,
    (width, height): (u32, u32),
    seconds: u64,
    controls: bool,
) -> Result<Vec<String>, String> {
    let arm = start(path, width, height)?;
    let (first_ms, _) = arm.wait(FIRST_FRAME, |item| *item == Incoming::Frame)?;
    let image = arm.frames.take();
    let colored = image
        .as_ref()
        .and_then(|image| image.as_bytes(0))
        .is_some_and(|bytes| {
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[0] != pixel[1] || pixel[1] != pixel[2])
        });
    let mut fields = vec![
        field("first_receive_ms", format!("{first_ms:.3}")),
        field("colored", colored),
    ];
    let pids = arm.pids();
    let (cpu_before, began) = (cpu_ms(&pids), Instant::now());
    let mut frames = 0_u64;
    while began.elapsed() < Duration::from_secs(seconds) {
        match arm.received.recv_timeout(Duration::from_millis(100)) {
            Ok((_, Incoming::Frame)) => frames += 1,
            Ok((_, Incoming::Event(Event::Failed(failure)))) => {
                return Err(format!("failed {}", failure.word()))
            }
            Ok((_, Incoming::Event(Event::Ended))) | Ok((_, Incoming::Closed)) => break,
            _ => {}
        }
        let _ = arm.frames.take();
    }
    let elapsed = began.elapsed().as_secs_f64();
    let pids = arm.pids();
    let (pss, answered) = pss_kib(&pids);
    let cpu = cpu_ms(&pids) - cpu_before;
    fields.push(field("frames", frames));
    fields.push(field(
        "fps",
        format!("{:.2}", frames as f64 / elapsed.max(0.001)),
    ));
    fields.push(field(
        "cpu_ms_per_frame",
        format!("{:.3}", cpu / frames.max(1) as f64),
    ));
    fields.push(field("helper_pss_kib", pss));
    fields.push(field("helper_pss_read", answered));
    fields.push(field("helper_tree", pids.len()));
    if controls {
        fields.extend(controls_case(arm)?);
    } else {
        fields.push(teardown(arm));
    }
    Ok(fields)
}

fn controls_case(mut arm: Arm) -> Result<Vec<String>, String> {
    let mut fields = Vec::new();
    let mut check = |name: &str, command: Command, expect: fn(&Event) -> bool, arm: &mut Arm| {
        arm.decoder.send(command);
        let passed = arm
            .wait(
                Duration::from_secs(4),
                |item| matches!(item, Incoming::Event(event) if expect(event)),
            )
            .is_ok();
        fields.push(field(name, passed));
    };
    check(
        "pause",
        Command::Pause,
        |event| *event == Event::Paused,
        &mut arm,
    );
    check(
        "seek",
        Command::Seek(1000),
        |event| *event == Event::Seeked(1000),
        &mut arm,
    );
    check(
        "play",
        Command::Play,
        |event| *event == Event::Playing,
        &mut arm,
    );
    arm.decoder.send(Command::Mute(true));
    arm.decoder.send(Command::Gain(0.5));
    check(
        "mute_gain_then_stop",
        Command::Stop,
        |event| *event == Event::Stopped,
        &mut arm,
    );
    fields.push(teardown(arm));
    Ok(fields)
}

/// Drops the decoder and times until its whole tree is gone.
fn teardown(arm: Arm) -> String {
    let pids = arm.pids();
    let began = Instant::now();
    drop(arm);
    while began.elapsed() < Duration::from_secs(3)
        && pids
            .iter()
            .any(|pid| Path::new(&format!("/proc/{pid}")).exists())
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    let gone = !pids
        .iter()
        .any(|pid| Path::new(&format!("/proc/{pid}")).exists());
    format!(
        "{},{}",
        field(
            "teardown_ms",
            format!("{:.1}", began.elapsed().as_secs_f64() * 1000.)
        ),
        field("tree_gone", gone)
    )
}

fn audio_case(path: &Path) -> Result<Vec<String>, String> {
    let arm = start(path, 16, 16)?;
    let (loaded_ms, loaded) = arm.wait(Duration::from_secs(10), |item| {
        matches!(item, Incoming::Event(Event::Loaded { .. }))
    })?;
    let video = matches!(loaded, Incoming::Event(Event::Loaded { video: true, .. }));
    let (clock_ms, _) = arm.wait(
        Duration::from_secs(5),
        |item| matches!(item, Incoming::Event(Event::Position(ms)) if *ms > 0),
    )?;
    Ok(vec![
        field("loaded_ms", format!("{loaded_ms:.3}")),
        field("video", video),
        field("clock_ms", format!("{clock_ms:.3}")),
        teardown(arm),
    ])
}

pub fn run(words: &[String]) -> Result<(), String> {
    let number = |flag: &str, default: u64| {
        words
            .iter()
            .position(|word| word == flag)
            .and_then(|at| words.get(at + 1))
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    };
    let (seconds, rounds) = (number("--seconds", 4), number("--rounds", 3));
    let root = scratch()?;
    eprintln!("flea-media-kit: scratch {}", root.display());
    let mut firsts: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut red = 0;
    for codec in ["h264", "vp8"] {
        for (size, width, height) in SIZES {
            let file = clip(&root, codec, size, seconds as u32 + 3)?;
            for round in 0..rounds {
                let head = format!("\"engine\":\"mpv\",\"codec\":\"{codec}\",\"size\":\"{size}\",\"round\":{round}");
                match case(&file, (width, height), seconds, round == 0) {
                    Ok(fields) => {
                        if let Some(ms) = fields[0]
                            .strip_prefix("\"first_receive_ms\":")
                            .and_then(|ms| ms.parse().ok())
                        {
                            firsts
                                .entry(format!("mpv {codec} {size}"))
                                .or_default()
                                .push(ms);
                        }
                        println!("{{{head},{}}}", fields.join(","));
                    }
                    Err(cause) => {
                        red += 1;
                        println!(
                            "{{{head},\"red\":true,\"cause\":\"{}\"}}",
                            cause.replace('"', "'")
                        );
                    }
                }
            }
        }
    }
    let audio = tone(&root, 4)?;
    match audio_case(&audio) {
        Ok(fields) => println!(
            "{{\"engine\":\"mpv\",\"codec\":\"pcm\",{}}}",
            fields.join(",")
        ),
        Err(cause) => {
            red += 1;
            println!("{{\"engine\":\"mpv\",\"codec\":\"pcm\",\"red\":true,\"cause\":\"{cause}\"}}");
        }
    }
    for (arm, mut values) in firsts {
        if let Some((low, median, high)) = range(&mut values) {
            println!(
                "{{\"summary\":\"{arm}\",\"first_receive_ms\":[{low:.1},{median:.1},{high:.1}]}}"
            );
        }
    }
    println!("{{\"red_cases\":{red}}}");
    Ok(())
}
