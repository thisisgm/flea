use super::{
    graphics::{kitty, Protocol},
    job::Job,
};
use std::path::PathBuf;

pub struct Pdf {
    pub path: PathBuf,
    pub page: usize,
    pub pages: usize,
    pub zoom: usize,
    pub scroll: usize,
    pub control: usize,
    pub bytes: Vec<u8>,
    pub error: String,
    pub columns: usize,
    pub rows: usize,
    pub pixels: (usize, usize),
    protocol: Protocol,
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
    ) -> Self {
        let info = Some(Job::start(
            path.clone(),
            vec!["/usr/bin/pdfinfo".into(), "{input}".into()],
        ));
        let mut pdf = Self {
            path,
            page: 1,
            pages: 0,
            zoom: 100,
            scroll: 0,
            control: 0,
            bytes: Vec::new(),
            error: String::new(),
            columns,
            rows,
            pixels,
            protocol,
            info,
            image: None,
        };
        pdf.refresh();
        pdf
    }
    pub fn refresh(&mut self) {
        if self.protocol == Protocol::None {
            return;
        }
        let (width, height) = self.pixels;
        let format = if self.protocol == Protocol::Kitty {
            "png:-"
        } else {
            "sixel:-"
        };
        // The script has only numeric positional arguments; /input is the sandbox's held file descriptor.
        let script="/usr/bin/pdftoppm -f \"$1\" -l \"$1\" -singlefile -scale-to \"$2\" -png /input | /usr/bin/magick png:- -resize \"$3\" -resize \"$4\" -crop \"$5\" +repage \"$6\"";
        self.image = Some(Job::start(
            self.path.clone(),
            vec![
                "/usr/bin/bash".into(),
                "-o".into(),
                "pipefail".into(),
                "-c".into(),
                script.into(),
                "flea-pdf".into(),
                self.page.to_string(),
                width.max(height).saturating_mul(3).min(4096).to_string(),
                format!("{}x{}", width, height),
                format!("{}%", self.zoom),
                format!("{}x{}+0+{}", width, height, self.scroll),
                format.into(),
            ],
        ));
    }
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        if let Some(info) = &self.info {
            if let Ok(result) = info.result.try_recv() {
                self.info = None;
                match result {
                    Ok(bytes) => self.pages = pages(&String::from_utf8_lossy(&bytes)),
                    Err(e) => self.error = e,
                }
                changed = true;
            }
        }
        if let Some(image) = &self.image {
            if let Ok(result) = image.result.try_recv() {
                self.image = None;
                match result {
                    Ok(bytes) => {
                        self.bytes = if self.protocol == Protocol::Kitty {
                            kitty(&bytes, self.columns, self.rows)
                        } else {
                            bytes
                        }
                    }
                    Err(e) => self.error = e,
                }
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
        let next = (self.scroll as isize + delta * (self.pixels.1 / self.rows.max(1)) as isize)
            .max(0)
            .min(self.pixels.1 as isize * 2) as usize;
        if next != self.scroll {
            self.scroll = next;
            self.refresh();
        }
    }
    pub fn prefix(&self) -> String {
        format!("Page {} / {} · {}%  ", self.page, if self.pages == 0 { "?".into() } else { self.pages.to_string() }, self.zoom)
    }
    pub fn control_at(&self, cell: usize, quicklook: bool, columns: usize) -> Option<(usize, bool)> {
        toolbar(self.control, quicklook, columns).1.get(cell).copied().flatten()
    }
    pub fn line(&self, quicklook: bool, columns: usize) -> String {
        toolbar(self.control, quicklook, columns).0
    }
}
fn toolbar(control: usize, quicklook: bool, columns: usize) -> (String, Vec<Option<(usize, bool)>>) {
    let labels: Vec<String> = ["‹", "›", "−", "+", "↗", "×"].iter()
        .take(if quicklook { 6 } else { 5 }).enumerate()
        .map(|(index, label)| if index == control { format!("[{label}]") } else { label.to_string() }).collect();
    let widths: Vec<usize> = labels.iter().map(|label| super::render::text_width(label)).collect();
    let overflow = widths.iter().sum::<usize>() + (labels.len() - 1) * 2 > columns;
    let available = columns.saturating_sub(if overflow { 2 } else { 0 });
    let (mut first, mut end, mut used) = (control, control + 1, widths[control]);
    // Keep complete controls around focus; overflow arrows reveal controls without activating them.
    while first > 0 && used + 2 + widths[first - 1] <= available {
        first -= 1;
        used += 2 + widths[first];
    }
    while end < labels.len() && used + 2 + widths[end] <= available {
        used += 2 + widths[end];
        end += 1;
    }
    let (mut line, mut hits) = (String::new(), Vec::new());
    let mut append = |text: &str, hit| {
        line.push_str(text);
        hits.extend(std::iter::repeat(hit).take(super::render::text_width(text)));
    };
    if overflow {
        append(if first > 0 { "←" } else { " " }, first.checked_sub(1).map(|index| (index, false)));
    }
    for (index, label) in labels.iter().enumerate().take(end).skip(first) {
        if index > first { append("  ", None); }
        append(label, Some((index, true)));
    }
    if overflow {
        append(&" ".repeat(available.saturating_sub(used)), None);
        append(if end < labels.len() { "→" } else { " " }, (end < labels.len()).then_some((end, false)));
    }
    (line, hits)
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
    #[test]
    fn narrow_toolbar_keeps_focus_and_pointer_targets_visible() {
        let labels = ["‹", "›", "−", "+", "↗", "×"];
        assert_eq!(toolbar(0, false, 80).0, "[‹]  ›  −  +  ↗");
        assert!(!super::super::render::fit(&toolbar(4, false, 80).0, 6).contains("[↗]"));
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
