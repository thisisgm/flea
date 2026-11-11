//! One bounded worker coalesces changes; cancelled epochs cannot replace a newer layout.
use super::*;
#[path = "font.rs"]
mod font;
#[path = "source.rs"]
mod source;
use crate::backend::{archivework::Work, printtool};
use crate::tui::graphics::{Graphics, Protocol, Task};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::sync::Arc;
use std::{
    io::Read,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

const OUTPUT_LIMIT: u64 = 32 * 1024 * 1024;
const PRIVATE_ATTEMPTS: usize = 64;
static PRIVATE_SEQUENCE: AtomicUsize = AtomicUsize::new(0);
struct PrivateRoot(PathBuf);
impl PrivateRoot {
    fn new() -> io::Result<Self> {
        let parent = std::env::temp_dir();
        if !parent.is_absolute() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        for _ in 0..PRIVATE_ATTEMPTS {
            let id = PRIVATE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(
                ".flea-work-tui-combine-preview-private-{}-{id}",
                std::process::id()
            ));
            match std::fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => {
                    let root = Self(path);
                    std::fs::set_permissions(&root.0, std::fs::Permissions::from_mode(0o700))?;
                    return Ok(root);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(io::ErrorKind::AlreadyExists.into())
    }
}
impl Drop for PrivateRoot {
    fn drop(&mut self) {
        // Work drops first; a kept child makes this nonrecursive removal refuse the parent.
        if self.0.is_absolute() {
            let _ = std::fs::remove_dir(&self.0);
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Ticket {
    menu: usize,
    revision: usize,
    pixels: (usize, usize),
    protocol: Protocol,
    locale: crate::i18n::Locale,
    ink: [[u8; 3]; 3],
}
struct Reply {
    sizes: Vec<(u32, u32)>,
    bytes: Vec<u8>,
    sources: Arc<Vec<source::Source>>,
}
#[derive(Default)]
pub struct State {
    task: Option<(Ticket, Task<Result<Reply, Text>>)>,
    shown: Option<Ticket>,
    entered: bool,
    sources: Option<Arc<Vec<source::Source>>>,
    source_menu: usize,
}
impl State {
    #[cfg(test)]
    pub(super) fn pending(&self) -> bool {
        self.task.is_some()
    }
    pub fn poll_cancelled(&mut self) {
        if let Some((_, task)) = &mut self.task {
            task.cancel();
            if task.poll().is_some() {
                self.task = None;
            }
        }
        self.entered = false;
    }
    pub fn cancel(&mut self) {
        if let Some((_, task)) = &self.task {
            task.cancel();
        }
        self.shown = None;
        self.sources = None;
    }
}
pub fn step(m: &mut Model, graphics: &mut Graphics, frame: &mut String) -> (usize, usize) {
    let f = m.combining.card.as_ref().unwrap();
    let g = geometry(m);
    let cell = m
        .preview_state
        .cell
        .unwrap_or(super::super::graphics::FALLBACK_CELL);
    let pixels = (
        g.preview.width.saturating_sub(2).saturating_mul(cell.0),
        g.preview.height.saturating_sub(2).saturating_mul(cell.1),
    );
    let palette = m.model3d_palette;
    let rgb = |v: [f32; 3]| v.map(|c| (c.clamp(0., 1.) * 255.).round() as u8);
    let ticket = Ticket {
        menu: m.action_id,
        revision: f.revision,
        pixels,
        protocol: m.capabilities.graphics,
        locale: m.i18n.locale().clone(),
        ink: [
            rgb(palette.foreground),
            rgb(palette.muted),
            rgb(palette.ground),
        ],
    };
    let state = &mut m.combining.preview;
    if !state.entered {
        graphics.clear();
        state.entered = true;
    }
    if state.shown.as_ref().is_some_and(|shown| shown != &ticket) {
        graphics.clear();
        state.shown = None;
    }
    if let Some((held, task)) = &mut state.task {
        if held != &ticket {
            task.cancel();
        }
        if let Some(reply) = task.poll() {
            let current = held == &ticket && !task.cancelled();
            state.task = None;
            if current {
                state.shown = Some(ticket.clone());
                let reply = match reply {
                    Ok(result) => result,
                    Err(error) => Err(Text::Data(error)),
                };
                match reply {
                    Ok(reply) => {
                        m.combining.card.as_mut().unwrap().sizes = reply.sizes;
                        state.sources = Some(reply.sources);
                        state.source_menu = ticket.menu;
                        if ticket.protocol == Protocol::Kitty {
                            graphics.transmit =
                                super::super::graphics::kitty_chunks(&reply.bytes, 't');
                            graphics.bytes = super::super::graphics::PLACE.to_vec();
                        } else {
                            graphics.bytes = reply.bytes;
                        }
                    }
                    Err(error) => {
                        let f = m.combining.card.as_mut().unwrap();
                        Held::set(
                            &mut f.error,
                            &mut f.error_words,
                            Text::Args("dialogs.output.error.operation", vec![error]),
                            &m.i18n,
                        );
                    }
                }
                frame.clear();
            }
        }
    }
    let state = &mut m.combining.preview;
    if state.task.is_none() && state.shown.as_ref() != Some(&ticket) {
        let f = m.combining.card.as_ref().unwrap();
        if !f.paths.is_empty()
            && pixels.0 > 0
            && pixels.1 > 0
            && pixels.0 <= 4096
            && pixels.1 <= 4096
            && (f.settings.layout != Layout::Grid
                || (1..=output::MAX_ITEMS).contains(&f.settings.columns))
        {
            let (paths, settings, focus) = (
                f.paths.clone(),
                f.settings,
                (f.focus == 0).then_some(f.image),
            );
            let labels = (1..=paths.len())
                .map(|n| m.i18n.locale().format_count(n as i64))
                .collect::<Vec<_>>();
            let held = ticket.clone();
            let sources = if state.source_menu == ticket.menu {
                state.sources.clone()
            } else {
                None
            };
            state.task = Some((
                ticket,
                Task::start(move |cancel| {
                    Ok(render(
                        &paths, sources, settings, focus, &labels, &held, cancel,
                    ))
                }),
            ));
        }
    }
    (g.preview.x + 2, g.preview.y + 2)
}
fn render(
    paths: &[String],
    sources: Option<Arc<Vec<source::Source>>>,
    settings: Settings,
    focus: Option<usize>,
    labels: &[String],
    ticket: &Ticket,
    cancel: &AtomicBool,
) -> Result<Reply, Text> {
    let private = PrivateRoot::new().map_err(|e| crate::error::io_message(&e))?;
    let mut work = Work::new(&private.0, "tui-combine-preview").map_err(|e| e.msg)?;
    std::fs::set_permissions(&work.dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| crate::error::io_message(&e))?;
    let native = ticket.protocol != Protocol::None;
    let sources = match sources.filter(|sources| {
        paths.iter().all(|path| {
            sources
                .iter()
                .any(|s| &s.path == path && (!native || !s.png.is_empty()))
        })
    }) {
        Some(sources) => sources,
        None => Arc::new(source::load(paths, native, cancel)?),
    };
    let ordered = paths
        .iter()
        .map(|path| sources.iter().find(|s| &s.path == path).unwrap())
        .collect::<Vec<_>>();
    let sizes = ordered.iter().map(|s| s.size).collect::<Vec<_>>();
    let composition = flea::ui_logic_print_convert::geometry::Composition::new(&sizes, &settings)
        .map_err(Text::Key)?;
    if !native {
        return Ok(Reply {
            sizes,
            bytes: Vec::new(),
            sources,
        });
    }
    let output = work.dir.join("encoded");
    let (width, height) = ticket.pixels;
    let mut args = vec![
        "magick".into(),
        "-size".into(),
        format!("{width}x{height}"),
        "xc:none".into(),
    ];
    font::append(&mut args, &work.dir, cancel);
    let fitted = composition.fitted(width as f64, height as f64, 0.);
    let color = |rgb: [u8; 3]| format!("rgb({},{},{})", rgb[0], rgb[1], rgb[2]);
    let (ox, oy) = (
        ((width as f64 - fitted.width) / 2.).round(),
        ((height as f64 - fitted.height) / 2.).round(),
    );
    args.extend([
        "-fill".into(),
        settings.background.ink().into(),
        "-draw".into(),
        format!(
            "rectangle {ox},{oy} {},{}",
            ox + fitted.width - 1.,
            oy + fitted.height - 1.
        ),
    ]);
    for (i, rect) in fitted.tiles.iter().enumerate() {
        let input = work.dir.join(format!("tile-{i}.png"));
        std::fs::write(&input, &ordered[i].png).map_err(|e| crate::error::io_message(&e))?;
        args.extend([
            "(".into(),
            input.to_string_lossy().into_owned(),
            "-resize".into(),
            format!("{}x{}!", rect.width as usize, rect.height as usize),
            ")".into(),
            "-geometry".into(),
            format!("+{}+{}", rect.x as usize, rect.y as usize),
            "-compose".into(),
            "over".into(),
            "-composite".into(),
        ]);
    }
    for (i, rect) in fitted.tiles.iter().enumerate() {
        if focus == Some(i) {
            args.extend([
                "-fill".into(),
                "none".into(),
                "-stroke".into(),
                color(ticket.ink[0]),
                "-strokewidth".into(),
                "2".into(),
                "-draw".into(),
                format!(
                    "rectangle {},{} {},{}",
                    rect.x,
                    rect.y,
                    rect.x + rect.width - 1.,
                    rect.y + rect.height - 1.
                ),
            ]);
        }
        let x = rect.x.round() as i64;
        let y = rect.y.round() as i64;
        args.extend([
            "-stroke".into(),
            "none".into(),
            "-fill".into(),
            color(ticket.ink[2]),
            "-draw".into(),
            format!("rectangle {x},{y} {},{}", x + 12, y + 14),
            "-fill".into(),
            color(ticket.ink[1]),
            "-pointsize".into(),
            "11".into(),
            "-gravity".into(),
            "northwest".into(),
            "-annotate".into(),
            format!("+{}+{}", x + 2, y + 1),
            labels[i].clone(),
        ]);
    }
    args.push(format!(
        "{}:{}",
        if ticket.protocol == Protocol::Kitty {
            "png"
        } else {
            "sixel"
        },
        output.display()
    ));
    printtool::boxed(args, &mut work, cancel)?;
    let file = crate::backend::regfile::open_if_regular(&output, 0)
        .map_err(|e| crate::error::io_message(&e))?;
    let mut bytes = Vec::new();
    file.take(OUTPUT_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| crate::error::io_message(&e))?;
    if bytes.is_empty() || bytes.len() as u64 > OUTPUT_LIMIT {
        return Err(Text::Key("dialogs.output.error.dimensions"));
    }
    Ok(Reply {
        sizes,
        bytes,
        sources,
    })
}
