//! The decoder helper: one sandboxed process per media subject, reading /input and writing records on stdout.
pub(crate) mod dl;
mod mpv;
mod mpv_api;

use flea::ui_logic_pv3::frame::{header, Command, Event, Failure, Record};
use std::io::{self, BufRead, Write};
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

/// The argument that turns the GUI binary into its own decoder helper.
pub const FLAG: &str = "--media-helper";
/// Where the sandbox mounts the one file the helper may read.
pub const INPUT: &str = "/input";

/// What one helper run was asked for.
pub struct Options {
    pub box_w: u32,
    pub box_h: u32,
    pub audio: bool,
    pub autoplay: bool,
}

impl Options {
    /// `BOX_W BOX_H AUDIO AUTOPLAY`, the words the sandbox passes after the flag.
    pub fn parse(words: &[String]) -> Option<Self> {
        let [box_w, box_h, audio, autoplay] = words else {
            return None;
        };
        let flag = |word: &str| match word {
            "0" => Some(false),
            "1" => Some(true),
            _ => None,
        };
        Some(Self {
            box_w: box_w.parse().ok().filter(|w| *w > 0)?,
            box_h: box_h.parse().ok().filter(|h| *h > 0)?,
            audio: flag(audio)?,
            autoplay: flag(autoplay)?,
        })
    }

    pub fn words(&self) -> [String; 4] {
        let flag = |on: bool| if on { "1" } else { "0" }.to_string();
        [
            self.box_w.to_string(),
            self.box_h.to_string(),
            flag(self.audio),
            flag(self.autoplay),
        ]
    }
}

/// The helper's stdout: whole records only, so the GUI never reads half of one.
pub struct Output {
    out: io::StdoutLock<'static>,
    pub started: Instant,
    pub first_frame: bool,
}

impl Output {
    fn new() -> Self {
        Self {
            out: io::stdout().lock(),
            started: Instant::now(),
            first_frame: true,
        }
    }

    pub fn event(&mut self, event: Event) -> io::Result<()> {
        let line = event.render();
        let record = Record::Event {
            length: line.len() as u32,
        };
        self.out.write_all(&header(record))?;
        self.out.write_all(line.as_bytes())?;
        self.out.flush()
    }

    /// One BGRA picture; the first one also says on stderr how long the helper took to make it.
    pub fn frame(&mut self, width: u32, height: u32, pixels: &[u8]) -> io::Result<()> {
        if self.first_frame {
            self.first_frame = false;
            eprintln!(
                "flea-media: first_render_ms={:.3}",
                self.started.elapsed().as_secs_f64() * 1000.
            );
        }
        self.out
            .write_all(&header(Record::Frame { width, height }))?;
        self.out.write_all(pixels)?;
        self.out.flush()
    }
}

/// Commands from the GUI, a closed stdin read as quit.
fn commands() -> Receiver<Command> {
    let (send, receive) = mpsc::sync_channel(32);
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if let Some(command) = Command::parse(&line) {
                if send.send(command).is_err() {
                    return;
                }
            }
        }
        let _ = send.send(Command::Quit);
    });
    receive
}

/// The sandbox mounts the input read-only; a helper that could open it for writing refuses to run.
fn refuse_writes() -> Result<(), String> {
    match std::fs::OpenOptions::new().write(true).open(INPUT) {
        Ok(_) => Err("the input is writable inside the sandbox".into()),
        Err(_) => Ok(()),
    }
}

/// Runs the helper when the arguments ask for it, returning its exit status.
pub fn dispatch(args: &[String]) -> Option<i32> {
    let words = args.strip_prefix(&[FLAG.to_string()])?;
    let Some(options) = Options::parse(words) else {
        eprintln!("flea-media: invalid helper arguments");
        return Some(2);
    };
    let mut output = Output::new();
    let result = refuse_writes().and_then(|()| mpv::run(&options, &mut output, &commands()));
    Some(match result {
        Ok(()) => 0,
        Err(failure) => {
            eprintln!("flea-media: {failure}");
            let _ = output.event(Event::Failed(Failure::Helper));
            1
        }
    })
}

/// What the decoder loop does with libmpv's frame this turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Draw {
    /// Render it at the decoded size and send it.
    Frame,
    /// Let it go unseen: its size is not known yet, and holding it would stall libmpv for 200 ms.
    Release,
    Nothing,
}

/// A waiting frame is drawn once its size is known and released before; a released one is drawn again once it is.
pub fn draw(pending: bool, sized: bool, owed: bool) -> Draw {
    match (pending || owed, sized) {
        (true, true) => Draw::Frame,
        _ if pending => Draw::Release,
        _ => Draw::Nothing,
    }
}

/// Swaps a BGRx picture's padding byte for opaque alpha, in place.
pub fn opaque(pixels: &mut [u8]) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
}

#[cfg(test)]
mod tests {
    use super::{draw, Draw};

    #[test]
    fn upd_a_frame_with_no_size_is_released_then_drawn_once_sized() {
        // Base behaviour held this frame until dwidth came, which libmpv only reports after its 200 ms wait.
        assert_eq!(draw(true, false, false), Draw::Release);
        assert_eq!(
            draw(false, true, true),
            Draw::Frame,
            "the released frame is drawn again once sized"
        );
        assert_eq!(draw(true, true, false), Draw::Frame);
        assert_eq!(draw(false, true, false), Draw::Nothing);
        assert_eq!(
            draw(false, false, true),
            Draw::Nothing,
            "still unsized: nothing to draw yet"
        );
    }
}
