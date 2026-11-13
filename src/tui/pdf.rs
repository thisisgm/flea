use super::{
    graphics::{kitty, Protocol, FALLBACK_CELL},
    job::Job,
    pane::Ink,
};
use std::path::PathBuf;

pub struct Pdf {
    // The file the row names; path is the PDF drawn, which an Office conversion makes a copy of.
    pub source: PathBuf,
    pub path: PathBuf,
    pub title: String,
    pub author: String,
    pub page: usize,
    pub pages: usize,
    // An office preview's cut: a count that reaches it reads "30+" (viewer::cut).
    pub first: Option<usize>,
    pub zoom: usize,
    pub scroll: usize,
    pub control: usize,
    pub bytes: Vec<u8>,
    // A half-block page's PNG, and a count that rises with each new one so its cells are asked for once.
    pub png: Vec<u8>,
    pub revision: usize,
    pub error: String,
    pub failure: Option<super::preview::words::Text>,
    pub columns: usize,
    pub rows: usize,
    pub pixels: (usize, usize),
    // The decoded page's pixels, and whether a first page decode has finished either way.
    pub shown: Option<(usize, usize)>,
    pub settled: bool,
    protocol: Protocol,
    // The window's page ground veil, and whether the page is decoded as PNG for half-block cells.
    ground: String,
    still: bool,
    info: Option<Job>,
    image: Option<Job>,
}
impl Pdf {
    pub fn new(
        path: PathBuf,
        protocol: Protocol,
        columns: usize,
        rows: usize,
        pixels: (usize, usize),
        ground: &str,
        still: bool,
    ) -> Self {
        let info = Some(Job::start(
            path.clone(),
            vec!["/usr/bin/pdfinfo".into(), "{input}".into()],
        ));
        let mut pdf = Self {
            source: path.clone(),
            title: String::new(),
            author: String::new(),
            path,
            page: 1,
            pages: 0,
            first: None,
            zoom: 100,
            scroll: 0,
            control: 0,
            bytes: Vec::new(),
            png: Vec::new(),
            revision: 0,
            error: String::new(),
            failure: None,
            columns,
            rows,
            pixels,
            shown: None,
            settled: false,
            protocol,
            ground: ground.into(),
            still: still && protocol == Protocol::None,
            info,
            image: None,
        };
        pdf.refresh();
        pdf
    }
    pub fn refresh(&mut self) {
        if self.protocol == Protocol::None && !self.still {
            self.settled = true;
            return;
        }
        let (width, height) = self.extent();
        let format = if self.protocol == Protocol::Sixel { "sixel:-" } else { "png:-" };
        // The script takes numbers and one #rrggbb ground; /input is the sandbox's held file descriptor.
        let script = format!("/usr/bin/pdftoppm -f \"$1\" -l \"$1\" -singlefile -scale-to \"$2\" -png /input | /usr/bin/magick png:- -resize \"$3\" -resize \"$4\" -crop \"$5\" +repage -fill \"$7\" -colorize {VEIL_PERCENT}% \"$6\"");
        self.image = Some(Job::start(
            self.path.clone(),
            vec![
                "/usr/bin/bash".into(),
                "-o".into(),
                "pipefail".into(),
                "-c".into(),
                script,
                "flea-pdf".into(),
                self.page.to_string(),
                width.max(height).saturating_mul(3).min(4096).to_string(),
                format!("{}x{}", width, height),
                format!("{}%", self.zoom),
                format!("{}x{}+0+{}", width, height, self.scroll),
                format.into(),
                ground(&self.ground),
            ],
        ));
    }
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        if let Some(info) = &self.info {
            if let Ok(result) = info.result.try_recv() {
                let failure = result.as_ref().err().map(|error| info.failure(error.clone()));
                self.info = None;
                if failure.is_some() { self.failure = failure; }
                match result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        self.pages = pages(&text);
                        self.title = fact(&text, "Title:");
                        self.author = fact(&text, "Author:");
                    }
                    Err(e) => self.error = e,
                }
                changed = true;
            }
        }
        if let Some(image) = &self.image {
            if let Ok(result) = image.result.try_recv() {
                let failure = result.as_ref().err().map(|error| image.failure(error.clone()));
                self.image = None;
                if failure.is_some() { self.failure = failure; }
                match result {
                    Ok(bytes) => {
                        self.shown = size(&bytes);
                        // A half-block page never reaches the terminal as bytes; chafa reads it from png.
                        if self.still {
                            self.png = bytes;
                            self.revision += 1;
                        } else if self.protocol == Protocol::Kitty {
                            self.bytes = kitty(&bytes);
                        } else {
                            self.bytes = bytes;
                        }
                    }
                    Err(e) => self.error = e,
                }
                self.settled = true;
                changed = true;
            }
        }
        changed
    }
    pub fn turn(&mut self, delta: isize) {
        let next = (self.page as isize + delta).max(1) as usize;
        let next = if self.pages > 0 {
            next.min(self.pages)
        } else {
            1
        };
        if next != self.page {
            self.page = next;
            self.scroll = 0;
            self.refresh();
        }
    }
    pub fn zoom(&mut self, delta: isize) {
        let next = (self.zoom as isize + delta * 25).clamp(50, 300) as usize;
        if next != self.zoom {
            self.zoom = next;
            self.scroll = 0;
            self.refresh();
        }
    }
    pub fn scroll(&mut self, delta: isize) {
        let height = self.extent().1;
        let next = (self.scroll as isize + delta * (height / self.rows.max(1)) as isize)
            .max(0)
            .min(height as isize * 2) as usize;
        if next != self.scroll {
            self.scroll = next;
            self.refresh();
        }
    }
    // The page box in pixels; a half-block page without known cell pixels takes eight by sixteen a cell.
    fn extent(&self) -> (usize, usize) {
        if self.still && self.pixels.0 * self.pixels.1 == 0 { (self.columns * FALLBACK_CELL.0, self.rows * FALLBACK_CELL.1) } else { self.pixels }
    }
    // A page count in the locale's digits, "30+" for an office preview that reaches its cut.
    pub fn count_words(&self, i18n: &crate::i18n::I18n, count: usize) -> String {
        let plus = if flea::ui_logic_doc_preview::viewer::cut(count, self.first) { "+" } else { "" };
        format!("{}{plus}", i18n.locale().format_count(count as i64))
    }
    // Page 1 of 12 · 100%, the page line over the controls.
    pub fn page_line(&self, i18n: &crate::i18n::I18n) -> String {
        let pages = if self.pages == 0 { "?".into() } else { self.count_words(i18n, self.pages) };
        i18n.tr_args("pdf.page", &[&i18n.locale().format_count(self.page as i64), &pages, &i18n.locale().format_count(self.zoom as i64)])
    }
    pub fn control_at(&self, cell: usize, quicklook: bool, columns: usize) -> Option<(usize, bool)> {
        toolbar(self.control, false, quicklook, columns).1.get(cell).copied().flatten()
    }
    pub fn runs(&self, focused: bool, quicklook: bool, columns: usize) -> Vec<(Ink, String)> {
        toolbar(self.control, focused, quicklook, columns).0
    }
}
// The toolbar's inked runs, and per cell the control a press there reaches and whether it activates it.
type Runs = Vec<(Ink, String)>;
type Hits = Vec<Option<(usize, bool)>>;
// Each control owns three cells, its brackets blank until the preview holds the keyboard, so nothing moves when focus enters.
fn toolbar(control: usize, focused: bool, quicklook: bool, columns: usize) -> (Runs, Hits) {
    const LABELS: [&str; 6] = ["‹", "›", "−", "+", "↗", "×"];
    const SLOT: usize = 3;
    let count = if quicklook { 6 } else { 5 };
    let overflow = count * SLOT > columns;
    let shown = if overflow { (columns.saturating_sub(2) / SLOT).max(1) } else { count };
    // Keep the focused control in view; overflow arrows reveal controls without activating them.
    let first = control.saturating_sub(shown - 1).min(count - shown);
    let end = first + shown;
    let (mut runs, mut hits) = (Vec::new(), Vec::new());
    let ink = if focused { Ink::Foreground } else { Ink::Muted };
    if overflow {
        runs.push((ink, if first > 0 { "←" } else { " " }.to_string()));
        hits.push(first.checked_sub(1).map(|index| (index, false)));
    }
    for (index, label) in LABELS.iter().enumerate().take(end).skip(first) {
        let text = if focused && index == control { format!("[{label}]") } else { format!(" {label} ") };
        runs.push((ink, text));
        hits.extend([Some((index, true)); SLOT]);
    }
    if overflow {
        let gap = columns.saturating_sub(shown * SLOT + 2);
        runs.push((ink, " ".repeat(gap)));
        hits.extend(std::iter::repeat_n(None, gap));
        runs.push((ink, if end < count { "→" } else { " " }.to_string()));
        hits.push((end < count).then_some((end, false)));
    }
    (runs, hits)
}
const FALLBACK_GROUND: &str = "#101315";
const VEIL_PERCENT: usize = 5;
// The magick colour for the page veil, the ground as #rrggbb or the fallback ground when it is not one.
fn ground(hex: &str) -> String {
    if hex.len() == 7 && hex.starts_with('#') && hex[1..].bytes().all(|b| b.is_ascii_hexdigit()) { hex.into() } else { FALLBACK_GROUND.into() }
}
// A decoded page's pixels: a PNG's IHDR, or a sixel's raster attributes after its introducer.
// Sample input: "1;1;640;480
fn size(bytes: &[u8]) -> Option<(usize, usize)> {
    if bytes.starts_with(b"\x89PNG") && bytes.len() >= 24 {
        let read = |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize;
        return Some((read(16), read(20)));
    }
    let start = bytes.iter().take(64).position(|&b| b == b'"')? + 1;
    let end = bytes[start..].iter().position(|b| !b.is_ascii_digit() && *b != b';')? + start;
    let fields: Vec<usize> = std::str::from_utf8(&bytes[start..end]).ok()?.split(';').map(|n| n.parse().ok()).collect::<Option<_>>()?;
    (fields.len() == 4).then(|| (fields[2], fields[3]))
}
// Sample input: Title:           Spring report 2025; a field the file leaves out is an empty string.
fn fact(text: &str, key: &str) -> String {
    text.lines().find_map(|line| line.strip_prefix(key)).map(|value| value.trim().to_string()).unwrap_or_default()
}
// Sample input: Pages:           12
fn pages(text: &str) -> usize {
    text.lines()
        .find_map(|line| {
            line.strip_prefix("Pages:")
                .and_then(|v| v.trim().parse().ok())
        })
        .unwrap_or(0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn page_count_uses_named_fact() {
        assert_eq!(pages("Title: 99\nPages: 12\n"), 12);
        assert_eq!(pages("Pages: unknown"), 0);
    }
    // An office preview cut at 30 pages reads "30+" on the page line once its PDF reaches the cut.
    #[test]
    fn a_cut_office_preview_counts_its_pages_with_a_plus() {
        let i18n = crate::i18n::test_catalogs::english();
        let mut pdf = Pdf::new(PathBuf::from("/c/x.pdf"), Protocol::None, 48, 21, (0, 0), "#101315", false);
        let line = |pdf: &Pdf| pdf.page_line(&i18n);
        pdf.pages = 30;
        assert!(!line(&pdf).contains("30+"), "a whole PDF counts exactly");
        pdf.first = Some(30);
        assert!(line(&pdf).contains("30+"), "{}", line(&pdf));
        pdf.pages = 12;
        assert!(!line(&pdf).contains('+'), "a preview short of the cut is the whole document");
    }
    #[test]
    fn narrow_toolbar_keeps_focus_and_pointer_targets_visible() {
        let labels = ["‹", "›", "−", "+", "↗", "×"];
        let toolbar = |control, quicklook, columns| {
            let (runs, hits) = super::toolbar(control, true, quicklook, columns);
            (runs.into_iter().map(|run| run.1).collect::<String>(), hits)
        };
        assert_eq!(toolbar(0, false, 80).0, "[‹] ›  −  +  ↗ ");
        assert!(!super::super::render::fit(&toolbar(4, false, 80).0, 6).contains("[↗]"));
        // At rest every slot keeps its three cells in the muted ink, with no brackets.
        let (rest, _) = super::toolbar(0, false, false, 80);
        assert_eq!(rest.iter().map(|run| run.1.as_str()).collect::<String>(), " ‹  ›  −  +  ↗ ");
        assert!(rest.iter().all(|run| run.0 == Ink::Muted));
        assert!(super::toolbar(0, true, false, 80).0.iter().all(|run| run.0 == Ink::Foreground));
        for quicklook in [false, true] {
            let count = if quicklook { 6 } else { 5 };
            for terminal_columns in 12..=100 {
                let columns = if quicklook { terminal_columns } else { super::super::render::panes(terminal_columns, true).2 };
                for control in 0..count {
                    let (line, hits) = toolbar(control, quicklook, columns);
                    assert!(super::super::render::text_width(&line) <= columns);
                    assert_eq!(hits.len(), super::super::render::text_width(&line));
                    assert!(line.contains(&format!("[{}]", labels[control])));
                    assert_eq!(hits.iter().filter(|hit| **hit == Some((control, true))).count(), 3);
                    for (index, activate) in hits.iter().flatten() {
                        assert!(*index < count);
                        assert_eq!(*activate, line.contains(labels[*index]));
                    }
                }
                for forward in [false, true] {
                    let mut control = if forward { 0 } else { count - 1 };
                    let mut reachable = [false; 6];
                    for _ in 0..count {
                        let (_, hits) = toolbar(control, quicklook, columns);
                        for (index, activate) in hits.iter().flatten() {
                            if *activate { reachable[*index] = true; }
                        }
                        let edge = if forward { hits.last() } else { hits.first() };
                        let Some(Some((next, false))) = edge else { break; };
                        assert!(if forward { *next > control } else { *next < control });
                        control = *next;
                    }
                    assert!(reachable[..count].iter().all(|visible| *visible));
                }
            }
        }
    }
}
