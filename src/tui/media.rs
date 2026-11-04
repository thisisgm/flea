use super::{
    graphics::Protocol,
    mpv::Process,
    pane::Ink,
    wire::{number, text, word},
};
use crate::jsondoc::{self, Json};
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

pub struct Player {
    process: Option<Process>,
    socket: Option<UnixStream>,
    events: Option<Receiver<Json>>,
    started: Instant,
    protocol: Protocol,
    shm: bool,
    terminal_size: (usize, usize),
    pub path: PathBuf,
    pub paused: bool,
    pub buffering: bool,
    pub position: f64,
    pub duration: f64,
    pub probed: bool,
    // Set by the TUI while its poster of this file is on screen, so mpv can take over from it without a gap.
    pub poster_up: bool,
    pub control: usize,
    pub error: String,
    pub failure: Option<super::preview::words::Text>,
    label_cells: usize,
    locale_ready: bool,
    buffering_label: String,
    locale: crate::i18n::Locale,
    pub geometry: (usize, usize, usize, usize),
    pub pixels: (usize, usize),
    pub resume: Option<(f64, bool)>,
}
impl Player {
    pub fn start(
        path: &Path,
        protocol: Protocol,
        shm: bool,
        geometry: (usize, usize, usize, usize),
        terminal_size: (usize, usize),
        pixels: (usize, usize),
    ) -> io::Result<Self> {
        let mut player = Self::resting(path, protocol, shm, geometry, terminal_size, pixels);
        player.wake()?;
        Ok(player)
    }
    // A player at rest runs no mpv: its controls draw from the backend's probe, and the first play or seek starts mpv.
    pub fn resting(
        path: &Path,
        protocol: Protocol,
        shm: bool,
        geometry: (usize, usize, usize, usize),
        terminal_size: (usize, usize),
        pixels: (usize, usize),
    ) -> Self {
        Self {
            process: None,
            socket: None,
            events: None,
            started: Instant::now(),
            protocol,
            shm,
            terminal_size,
            path: path.into(),
            paused: true,
            buffering: false,
            position: 0.0,
            duration: 0.0,
            probed: false,
            poster_up: false,
            control: 0,
            error: String::new(),
            failure: None,
            label_cells: 5,
            locale_ready: false,
            buffering_label: String::new(),
            locale: crate::i18n::Locale::default(),
            geometry,
            pixels,
            resume: None,
        }
    }
    pub fn at_rest(&self) -> bool {
        self.process.is_none()
    }
    #[cfg(test)]
    pub fn force_starting_for_test(&mut self) {
        self.process = Some(Process::idle());
    }
    pub fn handing_off(&self) -> bool {
        self.process.as_ref().is_some_and(|p| p.poster.load(std::sync::atomic::Ordering::Relaxed))
    }
    pub fn probe(&mut self, milliseconds: usize) {
        if self.at_rest() && !self.probed {
            self.duration = milliseconds as f64 / 1000.0;
            self.probed = true;
        }
    }
    fn wake(&mut self) -> io::Result<()> {
        self.process = Some(Process::spawn(&self.path, self.protocol, self.geometry, self.terminal_size, self.pixels, self.shm, self.poster_up)?);
        self.started = Instant::now();
        Ok(())
    }
    // The controls take the asked state as mpv starts; it seeks there and takes the pause on connect.
    fn wake_at(&mut self, position: f64, paused: bool) -> io::Result<()> {
        self.wake()?;
        self.position = position;
        self.paused = paused;
        self.resume = Some((position, paused));
        Ok(())
    }
    pub fn command(&mut self, command: Vec<Json>) -> io::Result<()> {
        let value = Json::Obj(vec![("command".into(), Json::Arr(command))]);
        let socket = self
            .socket
            .as_mut()
            .ok_or_else(|| super::preview::words::issue("tui.preview.media_waiting", "Media preview is still loading"))?;
        socket.write_all(jsondoc::render(&value).replace('\n', "").as_bytes())?;
        socket.write_all(b"\n")
    }
    pub fn poll(&mut self) {
        if !self.error.is_empty() || self.at_rest() {
            return;
        }
        if self.socket.is_none() {
            match self.connect() {
                Ok(false) => return,
                Ok(true) => {}
                Err(e) => {
                    self.failure = Some(super::preview::words::from_io(&e));
                    self.error = crate::error::io_message(&e);
                    return;
                }
            }
        }
        let Some(events) = &self.events else { return };
        while let Ok(value) = events.try_recv() {
            if text(&value, "event") == "property-change" {
                let data = value.get("data").unwrap_or(&Json::Null);
                match text(&value, "name") {
                    "time-pos" => self.position = data.as_f64().unwrap_or(0.0),
                    "duration" => self.duration = data.as_f64().unwrap_or(0.0),
                    "pause" => self.paused = data.as_bool().unwrap_or(true),
                    "paused-for-cache" => self.buffering = data.as_bool().unwrap_or(false),
                    _ => {}
                }
            }
            if text(&value, "event") == "end-file" && text(&value, "reason") == "error" {
                self.failure = Some(super::preview::words::Text::Key("media.unplayable"));
                self.error = "Could not play selected media".into();
            }
        }
        if self.process.as_mut().is_some_and(|p| p.child.try_wait().ok().flatten().is_some()) {
            self.failure = Some(super::preview::words::Text::Key("tui.preview.media_stopped"));
            self.error = "Media preview stopped".into();
        }
    }
    fn connect(&mut self) -> io::Result<bool> {
        const STARTUP_LIMIT: Duration = Duration::from_secs(3);
        let Some(process) = self.process.as_mut() else { return Ok(false) };
        if process.child.try_wait()?.is_some() || self.started.elapsed() > STARTUP_LIMIT {
            return Err(super::preview::words::issue("tui.preview.media_start", "mpv could not start the inline preview"));
        }
        let socket = match UnixStream::connect(process.directory.join("ipc")) {
            Ok(socket) => socket,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) =>
            {
                return Ok(false)
            }
            Err(e) => return Err(e),
        };
        socket.set_write_timeout(Some(Duration::from_millis(100)))?;
        let reader = socket.try_clone()?;
        let (tx, events) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(reader).lines() {
                let Ok(line) = line else { break };
                if let Ok(value) = jsondoc::parse(&line) {
                    if tx.send(value).is_err() {
                        break;
                    }
                }
            }
        });
        self.socket = Some(socket);
        self.events = Some(events);
        for (i, property) in ["time-pos", "duration", "pause", "paused-for-cache"]
            .iter()
            .enumerate()
        {
            self.command(vec![word("observe_property"), number(i), word(property)])?;
        }
        if let Some((position, paused)) = self.resume.take() {
            self.command(vec![
                word("seek"),
                Json::Num(position.max(0.0).to_string()),
                word("absolute"),
            ])?;
            self.command(vec![
                word("set_property"),
                word("pause"),
                Json::Bool(paused),
            ])?;
            self.paused = paused;
        }
        Ok(true)
    }
    pub fn toggle(&mut self) -> io::Result<()> {
        if self.at_rest() {
            return self.wake_at(self.position, false);
        }
        if self.socket.is_none() {
            // mpv is still starting: the change waits in resume and lands when it connects.
            self.paused = !self.paused;
            self.resume = Some((self.position, self.paused));
            return Ok(());
        }
        let paused = !self.paused;
        self.command(vec![
            word("set_property"),
            word("pause"),
            Json::Bool(paused),
        ])?;
        self.paused = paused;
        Ok(())
    }
    pub fn seek(&mut self, seconds: i32) -> io::Result<()> {
        if self.at_rest() {
            return self.wake_at(rest_target(self.position, self.duration, seconds), true);
        }
        if self.socket.is_none() {
            self.position = rest_target(self.position, self.duration, seconds);
            self.resume = Some((self.position, self.paused));
            return Ok(());
        }
        self.command(vec![
            word("seek"),
            Json::Num(seconds.to_string()),
            word("relative"),
        ])
    }
    pub fn seek_at(&mut self, cell: usize, columns: usize) -> io::Result<()> {
        if self.duration <= 0.0 {
            return Ok(());
        }
        let (start, length) = self.track(columns);
        if cell < start || cell >= start + length { return Ok(()); }
        let ratio = cell.saturating_sub(start).min(length.saturating_sub(1)) as f64
            / length.saturating_sub(1).max(1) as f64;
        if self.at_rest() {
            return self.wake_at(ratio * self.duration, true);
        }
        if self.socket.is_none() {
            self.position = ratio * self.duration;
            self.resume = Some((self.position, self.paused));
            return Ok(());
        }
        self.command(vec![
            word("seek"),
            Json::Num((ratio * 100.0).to_string()),
            word("absolute-percent"),
        ])
    }
    pub fn ensure_locale(&mut self, i18n: &crate::i18n::I18n) {
        if !self.locale_ready { self.refresh_locale(i18n); }
    }
    pub fn refresh_locale(&mut self, i18n: &crate::i18n::I18n) {
        self.locale_ready = true;
        self.label_cells = ["media.play", "media.pause"].into_iter().map(|key| super::cells::text_width(&i18n.tr(key))).max().unwrap_or(0).max(5);
        self.locale = i18n.locale().clone();
        self.buffering_label = i18n.tr("media.buffering");
    }
    pub fn play_width(&self) -> usize { self.label_cells + 2 }
    fn track(&self, columns: usize) -> (usize, usize) {
        let start = (self.play_width() + 2).min(columns);
        let room = columns - start;
        let reserved = super::cells::text_width(&self.tail()).min(room.saturating_sub(1));
        (start, room - reserved)
    }
    fn tail(&self) -> String {
        let buffering = if self.buffering { format!(" · {}", self.buffering_label) } else { String::new() };
        format!("  {}{buffering}", self.clocks())
    }
    fn clocks(&self) -> String {
        format!("{} / {}", clock_with_locale(self.position, &self.locale), clock_with_locale(self.duration, &self.locale))
    }
    // The same locale projection sets both label cells and the pointer's play/track boundary.
    pub fn runs(&self, columns: usize, focused: bool, i18n: &crate::i18n::I18n) -> Vec<(Ink, String)> {
        if self.socket.is_none() && self.error.is_empty() && !self.probed {
            let label = i18n.tr("media.loading");
            return vec![(Ink::Meta, super::cells::ellipsis(&label, super::cells::text_width(&label).min(columns)))];
        }
        let (start, length) = self.track(columns);
        let play = self.play_width().min(columns);
        let ratio = if self.duration > 0.0 { (self.position / self.duration).clamp(0.0, 1.0) } else { 0.0 };
        let position = (ratio * length.saturating_sub(1) as f64).round() as usize;
        let mark = if focused && self.control == 1 { "◆" } else { "●" };
        let label = i18n.tr(if self.paused { "media.play" } else { "media.pause" });
        let bracket = |text| if focused && self.control == 0 { text } else { " " };
        let button = if play < 2 { super::cells::fit(&label, play) }
            else { format!("{}{}{}", bracket("["), super::cells::ellipsis(&label, play - 2), bracket("]")) };
        let mut runs = vec![
            (Ink::Foreground, format!("{button}{}", " ".repeat(start - play))),
            (Ink::Frame, "─".repeat(position.min(length))),
            (Ink::Foreground, if length > 0 { mark.into() } else { String::new() }),
            (Ink::Frame, "─".repeat(length.saturating_sub(position + 1))),
        ];
        let clock_room = columns - start - length;
        let clocks = super::cells::ellipsis(&self.tail(), clock_room);
        runs.push((Ink::Muted, clocks));
        runs
    }
    #[cfg(test)]
    pub fn line(&self, columns: usize, focused: bool, i18n: &crate::i18n::I18n) -> String {
        self.runs(columns, focused, i18n).into_iter().map(|(_, text)| text).collect()
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        if let Some(delete) = self.quit() {
            answer(delete);
        }
    }
}
// mpv's kitty frames carry no id, so only a delete of every placement reaches them; lowercase keeps the TUI's stored pictures.
const MPV_FRAMES_DELETE: &[u8] = b"\x1b_Ga=d,d=a,q=2\x1b\\";
impl Player {
    // What a dropped player must still send: the delete for frames a running mpv drew through the kitty protocol.
    pub(super) fn leftover_delete(&self) -> Option<&'static [u8]> {
        (self.protocol == Protocol::Kitty && !self.at_rest() && !self.handing_off()).then_some(MPV_FRAMES_DELETE)
    }
    // The kill beats mpv's own delete-all on quit, so the delete for its last frame is answered only once mpv is gone.
    fn quit(&mut self) -> Option<&'static [u8]> {
        let _ = self.command(vec![word("quit")]);
        let leftover = self.leftover_delete();
        drop(self.process.take());
        leftover
    }
}
#[cfg(not(test))]
fn answer(delete: &[u8]) {
    let mut out = std::io::stdout().lock();
    let _ = super::tmux::write_graphics(&mut out, delete).and_then(|()| out.flush());
}
// Under test the delete is kept before the tmux transport, so the terminal running cargo changes nothing.
#[cfg(test)]
fn answer(delete: &[u8]) {
    tests::answered(delete);
}
// Only the file that was playing picks up its place; any other file starts at the top.
pub fn resume_for(playback: Option<&(PathBuf, f64, bool)>, path: &Path) -> Option<(f64, bool)> {
    playback.filter(|(previous, ..)| previous == path).map(|(_, position, paused)| (*position, *paused))
}
// Where a seek asked at rest lands: inside the clip when its length is known.
fn rest_target(position: f64, duration: f64, seconds: i32) -> f64 {
    let target = (position + seconds as f64).max(0.0);
    if duration > 0.0 { target.min(duration) } else { target }
}
pub fn localized_clock(seconds: f64, i18n: &crate::i18n::I18n) -> String { clock_with_locale(seconds, i18n.locale()) }
fn clock_with_locale(seconds: f64, locale: &crate::i18n::Locale) -> String {
    clock(seconds).split(':').map(|part| locale.localize_number(part)).collect::<Vec<_>>().join(":")
}
pub fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    const SECONDS_PER_HOUR: u64 = 3600;
    const SECONDS_PER_MINUTE: u64 = 60;
    if seconds >= SECONDS_PER_HOUR {
        format!(
            "{}:{:02}:{:02}",
            seconds / SECONDS_PER_HOUR,
            seconds % SECONDS_PER_HOUR / SECONDS_PER_MINUTE,
            seconds % SECONDS_PER_MINUTE
        )
    } else {
        format!(
            "{}:{:02}",
            seconds / SECONDS_PER_MINUTE,
            seconds % SECONDS_PER_MINUTE
        )
    }
}
#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
