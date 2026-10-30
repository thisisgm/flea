//! The native presentation kit: a Quick Look card driven from a FIFO, recording wp_presentation, CPU and PSS.
use super::{cpu_ms, pss_kib, tree, MARKER};
use crate::g2::pv3::ink::{size as board, MediaInk};
use crate::g2::pv3::player::{MediaPlayer, Subject};
use crate::g2::pv3::sandbox::Sandbox;
use crate::g2::pv3::sound::MediaSound;
use crate::g2::pv3::surface::{Caches, Facts, QuickLook};
use crate::list::Typeface;
use flea::ui_logic_pv3::player::MediaKey;
use gpui::{
    div, prelude::*, px, size, App, Bounds, Context, Entity, Subscription, Window, WindowBounds,
    WindowOptions,
};
use std::ffi::c_int;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

// ql-video.html's card.
const CARD: (f32, f32) = (902., 574.);
const CLOCK_MONOTONIC: c_int = 1;

#[repr(C)]
struct Timespec {
    seconds: i64,
    nanoseconds: i64,
}

extern "C" {
    fn clock_gettime(clock: c_int, time: *mut Timespec) -> c_int;
}

/// CLOCK_MONOTONIC now, the clock Wayland's presentation feedback reports on.
fn monotonic() -> Duration {
    let mut time = Timespec {
        seconds: 0,
        nanoseconds: 0,
    };
    // SAFETY: clock_gettime fills the struct it is given.
    unsafe { clock_gettime(CLOCK_MONOTONIC, &mut time) };
    Duration::new(
        time.seconds.max(0) as u64,
        time.nanoseconds.clamp(0, 999_999_999) as u32,
    )
}

/// One open in flight: when it was asked for, and the frame number its first picture commits as.
struct Trigger {
    path: String,
    asked: Instant,
    asked_mono: Duration,
    generation: u64,
    received_ms: Option<f64>,
    target: Option<u64>,
}

struct Kit {
    player: Entity<MediaPlayer>,
    caches: Caches,
    typeface: Typeface,
    catalog: Arc<flea::i18n::I18n>,
    records: std::fs::File,
    trigger: Option<Trigger>,
    opened: u64,
    _presented: Option<Subscription>,
}

impl Kit {
    fn record(&mut self, line: String) {
        let _ = writeln!(self.records, "{line}");
        let _ = self.records.flush();
    }

    fn command(&mut self, line: &str, window: &mut Window, cx: &mut Context<Self>) {
        let words: Vec<&str> = line.split_whitespace().collect();
        let player = self.player.clone();
        match words.as_slice() {
            ["open", path, rest @ ..] => {
                let scale = window.scale_factor();
                let subject = Subject {
                    path: PathBuf::from(path),
                    video: !rest.contains(&"audio"),
                    box_w: (CARD.0 * scale).round() as u32,
                    box_h: ((CARD.1 - board::STRIP) * scale).round() as u32,
                    autoplay: true,
                };
                self.opened += 1;
                self.trigger = Some(Trigger {
                    path: (*path).into(),
                    asked: Instant::now(),
                    asked_mono: monotonic(),
                    generation: self.opened,
                    received_ms: None,
                    target: None,
                });
                player.update(cx, |player, cx| player.open(subject, cx));
            }
            ["toggle"] => player.update(cx, |player, cx| player.key(MediaKey::TogglePlay, cx)),
            ["mute"] => player.update(cx, |player, cx| player.key(MediaKey::Mute, cx)),
            ["seek", ms] => {
                let ms = ms.parse().unwrap_or(0);
                player.update(cx, |player, _| player.seek_to(ms));
            }
            ["volume", level] => {
                let level = level.parse::<u8>().unwrap_or(100);
                MediaSound::update(cx, |sound| {
                    sound.set_volume(level);
                    sound.keep_volume()
                });
            }
            ["close"] => player.update(cx, |player, cx| player.close(cx)),
            ["state"] => {
                let (status, position, duration) = {
                    let player = self.player.read(cx);
                    (player.status().word(), player.position, player.duration)
                };
                let sound = MediaSound::get(cx);
                self.record(format!("{{\"event\":\"state\",\"status\":\"{status}\",\"position\":{position},\"duration\":{duration},\"muted\":{},\"volume\":{}}}", sound.muted, sound.volume()));
            }
            ["phase", name] => self.phase(name, cx),
            ["sustain", seconds] => self.sustain(seconds.parse().unwrap_or(10), window, cx),
            ["quit"] => cx.quit(),
            _ => self.record(format!(
                "{{\"event\":\"refused\",\"line\":\"{}\"}}",
                line.replace('"', "'")
            )),
        }
        cx.notify();
    }

    /// The five-phase memory record: the GUI alone and every child it started, decoder trees included.
    fn phase(&mut self, name: &str, _: &mut Context<Self>) {
        let own = std::process::id();
        let all = tree(own);
        let (gui, _) = pss_kib(&[own]);
        let (total, read) = pss_kib(&all);
        self.record(format!("{{\"event\":\"pss\",\"phase\":\"{name}\",\"gui_kib\":{gui},\"children_kib\":{},\"total_kib\":{total},\"processes\":{},\"read\":{read}}}", total - gui, all.len()));
    }

    fn sustain(&mut self, seconds: u64, window: &mut Window, cx: &mut Context<Self>) {
        let own = std::process::id();
        let before = (
            cpu_ms(&[own]),
            cpu_ms(&tree(own)),
            window
                .presented_frames()
                .map_or(0, |frames| frames.presented),
        );
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(seconds)).await;
            let _ = this.update_in(cx, |this, window, _| {
                let presented = window.presented_frames().map_or(0, |frames| frames.presented) - before.2;
                let (gui, total) = (cpu_ms(&[own]) - before.0, cpu_ms(&tree(own)) - before.1);
                let frames = presented.max(1) as f64;
                this.record(format!("{{\"event\":\"sustain\",\"seconds\":{seconds},\"presented\":{presented},\"gui_cpu_ms_per_frame\":{:.3},\"tree_cpu_ms_per_frame\":{:.3}}}", gui / frames, total / frames));
            });
        })
        .detach();
    }

    /// The first picture of an open: its receive time, then the commit it rides, then wp_presentation's time for it.
    fn player_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (generation, framed) = {
            let player = self.player.read(cx);
            (
                self.opened,
                player.frame.is_some() && player.frame_generation == player.generation(),
            )
        };
        let Some(trigger) = self
            .trigger
            .as_mut()
            .filter(|trigger| trigger.generation == generation)
        else {
            return;
        };
        if framed && trigger.received_ms.is_none() {
            trigger.received_ms = Some(trigger.asked.elapsed().as_secs_f64() * 1000.);
            trigger.target = window.presented_frames().map(|frames| frames.committed + 1);
        }
    }

    fn presented(&mut self, window: &mut Window) {
        let Some(frames) = window.presented_frames() else {
            return;
        };
        let Some(trigger) = self.trigger.as_ref() else {
            return;
        };
        let (Some(target), Some(at)) = (trigger.target, frames.presented_at) else {
            return;
        };
        if frames.presented < target {
            return;
        }
        let clock = if frames.clock_id == CLOCK_MONOTONIC as u32 {
            "monotonic"
        } else {
            "other"
        };
        let ms = at.saturating_sub(trigger.asked_mono).as_secs_f64() * 1000.;
        let line = format!(
            "{{\"event\":\"first_present\",\"path\":\"{}\",\"engine\":\"mpv\",\"received_ms\":{:.3},\"presented_ms\":{ms:.3},\"frame\":{},\"clock\":\"{clock}\"}}",
            trigger.path.replace('"', "'"),
            trigger.received_ms.unwrap_or(0.),
            frames.presented
        );
        self.trigger = None;
        self.record(line);
    }
}

impl Render for Kit {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let ink = MediaInk::inherited(&window.text_style(), theme);
        let known = Facts::default();
        let card = QuickLook {
            player: &self.player,
            known: &known,
            catalog: &self.catalog,
            ink,
            typeface: self.typeface.clone(),
            caches: &self.caches,
            strip_shown: true,
            touched: None,
        }
        .build(cx);
        div().size_full().bg(ink.surface).child(card)
    }
}

fn marked(path: &Path) -> Result<(), String> {
    match path.parent() {
        Some(parent) if path.is_absolute() && parent.join(MARKER).is_file() => Ok(()),
        _ => Err(format!(
            "{} is not inside a marked kit root",
            path.display()
        )),
    }
}

pub fn run(words: &[String]) -> Result<(), String> {
    let value = |flag: &str| {
        words
            .iter()
            .position(|word| word == flag)
            .and_then(|at| words.get(at + 1))
            .map(PathBuf::from)
    };
    let (fifo, records) = (
        value("--fifo").ok_or("--fifo PATH is required")?,
        value("--records").ok_or("--records PATH is required")?,
    );
    marked(&fifo)?;
    marked(&records)?;
    let records = std::fs::OpenOptions::new()
        .create_new(true)
        .append(true)
        .open(&records)
        .map_err(|error| error.to_string())?;
    let catalog = Arc::new(flea::i18n::I18n::load(flea::i18n::Paths::default()));
    let (lines, received) = async_channel::unbounded::<String>();
    std::thread::spawn(move || loop {
        let Ok(file) = std::fs::File::open(&fifo) else {
            return;
        };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if lines.send_blocking(line).is_err() {
                return;
            }
        }
    });
    gpui_platform::application().run(move |cx: &mut App| {
        let options = WindowOptions {
            app_id: Some(crate::window::APP_ID.into()),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(CARD.0), px(CARD.1)),
                cx,
            ))),
            ..Default::default()
        };
        let backend: Rc<dyn crate::g2::pv3::engine::Backend> = match Sandbox::current() {
            Ok(sandbox) => Rc::new(sandbox),
            Err(error) => {
                eprintln!("flea-media-kit: {error}");
                std::process::exit(2);
            }
        };
        let opened = cx.open_window(options, |window, cx| {
            cx.new(|cx| {
                let player = cx.new(|cx| MediaPlayer::new(backend, None, cx));
                cx.observe_in(&player, window, |this: &mut Kit, _, window, cx| {
                    this.player_changed(window, cx)
                })
                .detach();
                let weak = cx.entity().downgrade();
                let presented = window.observe_frame_presented(cx, move |window, cx| {
                    let _ = weak.update(cx, |kit, _| kit.presented(window));
                });
                Kit {
                    player,
                    caches: Caches::default(),
                    typeface: Typeface::omarchy_default(),
                    catalog,
                    records,
                    trigger: None,
                    opened: 0,
                    _presented: Some(presented),
                }
            })
        });
        let Ok(handle) = opened else {
            eprintln!("flea-media-kit: the window did not open");
            std::process::exit(2);
        };
        cx.spawn(async move |cx| {
            while let Ok(line) = received.recv().await {
                let _ = handle.update(cx, |kit, window, cx| kit.command(&line, window, cx));
            }
        })
        .detach();
    });
    Ok(())
}
