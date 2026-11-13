use crate::backend::{jail, sandbox};
use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::thread::JoinHandle;

// Match the TUI preview job's byte/time limits and the PDF preview's 4096-sample dimension cap.
const OUTPUT_LIMIT: usize = 32 * 1024 * 1024;
const MAX_DIMENSION: usize = 4096;
// The longest SGR parameter string one decoder escape may carry.
const SGR_PARAMETER_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorMode {
    Truecolor,
    Indexed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Color {
    #[default]
    Default,
    Rgb(u8, u8, u8),
    Indexed(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cell {
    pub symbol: char,
    pub foreground: Color,
    pub background: Color,
}

// Row-major cells, padded to columns; dimensions describe the fitted image, not the requested box.
#[derive(Debug, Eq, PartialEq)]
pub struct Grid {
    pub columns: usize,
    pub rows: usize,
    pub cells: Vec<Cell>,
}

pub struct Job {
    pub result: mpsc::Receiver<io::Result<Grid>>,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Job {
    // The caller owns freshness/generation checks; this job never reopens the selected pathname.
    pub fn start(
        input: File,
        columns: usize,
        rows: usize,
        colors: ColorMode,
    ) -> io::Result<Option<Self>> {
        Self::start_on(
            input,
            columns,
            rows,
            colors,
            std::env::var_os("PATH")
                .as_deref()
                .unwrap_or(OsStr::new("")),
        )
    }

    pub(super) fn start_on(
        input: File,
        columns: usize,
        rows: usize,
        colors: ColorMode,
        path: &OsStr,
    ) -> io::Result<Option<Self>> {
        let Some(executable) = find_on(path) else {
            return Ok(None);
        };
        geometry(columns, rows)?;
        if !input.metadata()?.is_file() {
            return Err(refused());
        }
        if !sandbox::available() {
            return Err(super::preview::words::issue(
                "actions.preview.no_sandbox",
                "Preview sandbox is unavailable",
            ));
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let stopped = cancel.clone();
        let (tx, result) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("flea-chafa".into())
            .spawn(move || {
                let _ = tx.send(process::run(
                    input,
                    &executable,
                    (columns, rows),
                    colors,
                    &stopped,
                ));
            })?;
        Ok(Some(Self {
            result,
            cancel,
            worker: Some(worker),
        }))
    }

    // Explicit off-thread shutdown waits for cleanup; input-loop replacement should just drop the job.
    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = std::thread::Builder::new()
                .name("flea-chafa-reap".into())
                .spawn(move || {
                    let _ = worker.join();
                });
        }
    }
}

// Whether chafa is on PATH, for a decoder whose only reader would be chafa.
pub fn installed() -> bool {
    std::env::var_os("PATH").is_some_and(|path| find_on(&path).is_some())
}

fn find_on(path: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path)
        .filter(|directory| directory.is_absolute())
        .find_map(|directory| {
            let executable = directory.join("chafa");
            if !jail::executable_file(&executable) {
                return None;
            }
            let executable = executable.canonicalize().ok()?;
            executable.to_str()?;
            Some(executable)
        })
}

fn geometry(columns: usize, rows: usize) -> io::Result<()> {
    if columns == 0
        || rows == 0
        || columns > MAX_DIMENSION
        || rows > MAX_DIMENSION / 2
        || columns
            .checked_mul(rows)
            .is_none_or(|cells| cells > OUTPUT_LIMIT / std::mem::size_of::<Cell>())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            super::preview::words::Fault::new(
                "tui.preview.rectangle",
                "Preview rectangle exceeds its limit",
            ),
        ));
    }
    Ok(())
}

fn refused() -> io::Error {
    super::preview::words::issue("preview.unshown", "Image preview could not be shown")
}

// No decoder escape survives this boundary: only typed colours and known single-cell symbols leave it.
// Sample input: b"\x1b[38;2;132;24;24m\xe2\x96\x84\x1b[0m\n".
fn validate(bytes: &[u8], columns: usize, rows: usize) -> io::Result<Grid> {
    geometry(columns, rows)?;
    if bytes.len() > OUTPUT_LIMIT {
        return Err(refused());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| refused())?;
    let (mut foreground, mut background) = (Color::Default, Color::Default);
    let mut inverted = false;
    let mut lines = Vec::new();
    let mut line = Vec::new();
    let mut chars = text.chars();
    while let Some(symbol) = chars.next() {
        match symbol {
            '\x1b' => {
                if chars.next() != Some('[') {
                    return Err(refused());
                }
                let mut parameters = String::new();
                loop {
                    match chars.next() {
                        Some('m') => break,
                        Some(c)
                            if (c.is_ascii_digit() || c == ';')
                                && parameters.len() < SGR_PARAMETER_BYTES =>
                        {
                            parameters.push(c)
                        }
                        _ => return Err(refused()),
                    }
                }
                sgr(&parameters, &mut foreground, &mut background, &mut inverted)?;
            }
            '\n' => {
                if line.is_empty() || lines.len() >= rows {
                    return Err(refused());
                }
                lines.push(std::mem::take(&mut line));
            }
            ' ' | '▀' | '▄' => {
                if line.len() >= columns || lines.len() >= rows {
                    return Err(refused());
                }
                let (symbol, background) = match (inverted, symbol) {
                    (true, '▀') => ('▄', background),
                    (true, '▄') => ('▀', background),
                    (true, ' ') => ('▀', foreground),
                    _ => (symbol, background),
                };
                line.push(Cell {
                    symbol,
                    foreground,
                    background,
                });
            }
            _ => return Err(refused()),
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    let width = lines.iter().map(Vec::len).max().ok_or_else(refused)?;
    let height = lines.len();
    let blank = Cell {
        symbol: ' ',
        foreground: Color::Default,
        background: Color::Default,
    };
    let mut cells = Vec::with_capacity(width * height);
    for mut line in lines {
        line.resize(width, blank);
        cells.extend(line);
    }
    Ok(Grid {
        columns: width,
        rows: height,
        cells,
    })
}

// Sample input: "38;2;132;24;24;48;5;16".
fn sgr(
    text: &str,
    foreground: &mut Color,
    background: &mut Color,
    inverted: &mut bool,
) -> io::Result<()> {
    if text.is_empty() {
        *foreground = Color::Default;
        *background = Color::Default;
        *inverted = false;
        return Ok(());
    }
    let parameters: Vec<u8> = text
        .split(';')
        .map(|part| part.parse::<u8>().map_err(|_| refused()))
        .collect::<io::Result<_>>()?;
    let mut at = 0;
    while let Some(&parameter) = parameters.get(at) {
        at += 1;
        match parameter {
            0 => {
                *foreground = Color::Default;
                *background = Color::Default;
                *inverted = false;
            }
            7 => *inverted = true,
            27 => *inverted = false,
            39 => *foreground = Color::Default,
            49 => *background = Color::Default,
            38 | 48 => {
                let color = match parameters.get(at..) {
                    Some([2, r, g, b, ..]) => {
                        at += 4;
                        Color::Rgb(*r, *g, *b)
                    }
                    Some([5, n, ..]) => {
                        at += 2;
                        Color::Indexed(*n)
                    }
                    _ => return Err(refused()),
                };
                if parameter == 38 {
                    *foreground = color;
                } else {
                    *background = color;
                }
            }
            _ => return Err(refused()),
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "chafa_file_tests.rs"]
mod file_tests;
#[cfg(test)]
#[path = "chafa_image_tests.rs"]
mod image_tests;
#[cfg(test)]
#[path = "chafa_job_tests.rs"]
mod job_tests;
#[path = "chafa_process.rs"]
mod process;
#[cfg(test)]
#[path = "chafa_process_tests.rs"]
mod process_tests;
#[cfg(test)]
#[path = "chafa_test_support.rs"]
pub(super) mod test_support;
#[cfg(test)]
#[path = "chafa_tests.rs"]
mod tests;
