// Sample: "ESC[7;4H[2]ESC[7;8HPeerESC[7;13HC" replays onto this grid as "[2] Peer C" where the CLI drew it.
// The CLI draws only changed cells and steps over blanks with cursor moves, so stripped escapes name no device.

#[derive(Clone, Copy)]
struct Cell {
    // '\0' is the second half of a wide character drawn in the cell before it.
    ch: char,
    // Drawn with a background colour or reversed, which is how the panel marks its chosen row.
    lit: bool,
}

const BLANK: Cell = Cell { ch: ' ', lit: false };
const LOG_CAP: usize = 256 * 1024;

enum State {
    Ground,
    Escape,
    Csi,
    Osc,
    OscEscape,
    Charset,
}

pub struct Screen {
    rows: usize,
    cols: usize,
    main: Vec<Vec<Cell>>,
    alt: Option<Vec<Vec<Cell>>>,
    row: usize,
    col: usize,
    saved: (usize, usize),
    lit: bool,
    state: State,
    params: String,
    utf8: Vec<u8>,
    log: String,
}

impl Screen {
    pub fn new(rows: usize, cols: usize) -> Screen {
        Screen {
            rows,
            cols,
            main: vec![vec![BLANK; cols]; rows],
            alt: None,
            row: 0,
            col: 0,
            saved: (0, 0),
            lit: false,
            state: State::Ground,
            params: String::new(),
            utf8: Vec::new(),
            log: String::new(),
        }
    }

    // Pty reads come in any size, so a sequence or character split between two reads is finished by the next.
    pub fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.byte(b);
        }
    }

    // Every character the CLI printed, in order, with a carriage return or a line feed each ending a line.
    pub fn log(&self) -> &str {
        &self.log
    }

    // The screen as it stands, one line per row, the panel included while it is open.
    pub fn visible(&self) -> String {
        let grid = self.alt.as_ref().unwrap_or(&self.main);
        grid.iter().map(|row| line_of(row)).collect::<Vec<_>>().join("\n")
    }

    // The row the panel has chosen: the one whose "[n]" is drawn lit. None when no row is lit.
    pub fn chosen(&self) -> Option<String> {
        let grid = self.alt.as_ref().unwrap_or(&self.main);
        grid.iter().find_map(|row| {
            let open = row.windows(2).position(|w| w[0].ch == '[' && w[1].ch.is_ascii_digit())?;
            row[open].lit.then(|| line_of(row))
        })
    }

    fn grid(&mut self) -> &mut Vec<Vec<Cell>> {
        self.alt.as_mut().unwrap_or(&mut self.main)
    }

    fn byte(&mut self, b: u8) {
        match self.state {
            State::Ground => self.ground(b),
            State::Escape => self.escape(b),
            State::Csi => match b {
                0x30..=0x3f => self.params.push(b as char),
                0x20..=0x2f => {}
                0x40..=0x7e => {
                    self.state = State::Ground;
                    self.csi(b as char);
                }
                _ => self.state = State::Ground,
            },
            State::Osc => match b {
                0x07 => self.state = State::Ground,
                0x1b => self.state = State::OscEscape,
                _ => {}
            },
            State::OscEscape | State::Charset => self.state = State::Ground,
        }
    }

    fn ground(&mut self, b: u8) {
        if !self.utf8.is_empty() || b >= 0x80 {
            return self.utf8_byte(b);
        }
        match b {
            0x1b => self.state = State::Escape,
            b'\r' => {
                self.col = 0;
                self.end_log_line();
            }
            b'\n' => self.line_feed(),
            0x08 => self.col = self.col.saturating_sub(1),
            b'\t' => self.col = ((self.col / 8 + 1) * 8).min(self.cols - 1),
            0x20..=0x7e => self.put(b as char),
            _ => {}
        }
    }

    fn utf8_byte(&mut self, b: u8) {
        // A byte that cannot continue the pending character ends it as one replacement and starts afresh.
        if !self.utf8.is_empty() && b & 0xc0 != 0x80 {
            self.utf8.clear();
            self.put('\u{fffd}');
            return self.ground(b);
        }
        self.utf8.push(b);
        let want = match self.utf8[0] {
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf7 => 4,
            _ => 1,
        };
        if self.utf8.len() < want {
            return;
        }
        let ch = std::str::from_utf8(&self.utf8).ok().and_then(|s| s.chars().next()).unwrap_or('\u{fffd}');
        self.utf8.clear();
        self.put(ch);
    }

    fn escape(&mut self, b: u8) {
        self.state = State::Ground;
        match b {
            b'[' => {
                self.params.clear();
                self.state = State::Csi;
            }
            b']' => self.state = State::Osc,
            b'(' | b')' | b'*' | b'+' => self.state = State::Charset,
            b'7' => self.saved = (self.row, self.col),
            b'8' => (self.row, self.col) = self.saved,
            b'M' => self.row = self.row.saturating_sub(1),
            _ => {}
        }
    }

    fn put(&mut self, ch: char) {
        self.log.push(ch);
        // Widths come from the TUI table, glibc C.UTF-8, so an emoji in a name keeps the columns the CLI used.
        let wide = crate::tui::cells::width(ch);
        if wide == 0 {
            return;
        }
        if self.col + wide > self.cols {
            self.col = 0;
            self.line_feed_quiet();
        }
        let (row, col, lit) = (self.row, self.col, self.lit);
        let grid = self.grid();
        grid[row][col] = Cell { ch, lit };
        if wide == 2 {
            grid[row][col + 1] = Cell { ch: '\0', lit };
        }
        self.col += wide;
    }

    fn line_feed(&mut self) {
        self.end_log_line();
        self.line_feed_quiet();
    }

    // Sample: "...[ETA: 0s]ESC[1GESC[2KS Peer B: Sent 1 file".
    // A status line is redrawn in place with no carriage return, so any return to the first column ends a log line.
    fn end_log_line(&mut self) {
        if !self.log.is_empty() && !self.log.ends_with('\n') {
            self.log.push('\n');
        }
        // Progress redraws about once a second for a whole transfer, so past the cap only the recent half is kept.
        if self.log.len() > LOG_CAP {
            let mut half = self.log.len() / 2;
            while !self.log.is_char_boundary(half) {
                half += 1;
            }
            let cut = self.log[half..].find('\n').map_or(self.log.len(), |at| half + at + 1);
            self.log.drain(..cut);
        }
    }

    fn line_feed_quiet(&mut self) {
        if self.row + 1 < self.rows {
            self.row += 1;
            return;
        }
        let cols = self.cols;
        let grid = self.grid();
        grid.remove(0);
        grid.push(vec![BLANK; cols]);
    }

    fn csi(&mut self, last: char) {
        let private = self.params.starts_with('?');
        let nums: Vec<usize> = self.params.trim_start_matches('?').split(';').map(|p| p.parse().unwrap_or(0)).collect();
        let n = |i: usize| nums.get(i).copied().filter(|v| *v > 0).unwrap_or(1);
        let (rows, cols) = (self.rows, self.cols);
        match last {
            'H' | 'f' => (self.row, self.col) = ((n(0) - 1).min(rows - 1), (n(1) - 1).min(cols - 1)),
            'A' => self.row = self.row.saturating_sub(n(0)),
            'B' | 'e' => self.row = self.row.saturating_add(n(0)).min(rows - 1),
            'C' | 'a' => self.col = self.col.saturating_add(n(0)).min(cols - 1),
            'D' => self.col = self.col.saturating_sub(n(0)),
            'E' => (self.row, self.col) = (self.row.saturating_add(n(0)).min(rows - 1), 0),
            'F' => (self.row, self.col) = (self.row.saturating_sub(n(0)), 0),
            'G' | '`' => self.col = (n(0) - 1).min(cols - 1),
            'd' => self.row = (n(0) - 1).min(rows - 1),
            'J' => self.erase_display(nums[0]),
            'K' => self.erase_line(nums[0]),
            'X' => {
                let (row, col, count) = (self.row, self.col, n(0));
                for cell in self.grid()[row].iter_mut().skip(col).take(count) {
                    *cell = BLANK;
                }
            }
            'm' => self.style(&nums),
            'h' | 'l' if private && matches!(nums[0], 47 | 1047 | 1049) => self.switch(last == 'h'),
            _ => {}
        }
        if self.col == 0 && matches!(last, 'H' | 'f' | 'E' | 'F' | 'G' | '`') {
            self.end_log_line();
        }
    }

    fn erase_line(&mut self, mode: usize) {
        let (row, col) = (self.row, self.col.min(self.cols - 1));
        let line = &mut self.grid()[row];
        let span = match mode {
            0 => col..line.len(),
            1 => 0..col + 1,
            _ => 0..line.len(),
        };
        for cell in &mut line[span] {
            *cell = BLANK;
        }
    }

    fn erase_display(&mut self, mode: usize) {
        let row = self.row;
        let rows = match mode {
            0 => row + 1..self.rows,
            1 => 0..row,
            _ => 0..self.rows,
        };
        if mode < 2 {
            self.erase_line(mode);
        }
        for r in rows {
            for cell in self.grid()[r].iter_mut() {
                *cell = BLANK;
            }
        }
    }

    // The CLI marks its chosen row with a background colour; any background or reverse counts as lit.
    fn style(&mut self, nums: &[usize]) {
        let mut i = 0;
        while i < nums.len() {
            match nums[i] {
                0 | 27 | 49 => self.lit = false,
                7 | 40..=47 | 100..=107 => self.lit = true,
                // 38 and 48 carry 5;n or 2;r;g;b after them, which are not codes of their own.
                code @ (38 | 48) => {
                    if code == 48 {
                        self.lit = true;
                    }
                    i += if nums.get(i + 1) == Some(&2) { 4 } else { 2 };
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn switch(&mut self, to_alt: bool) {
        if to_alt {
            self.saved = (self.row, self.col);
            self.alt = Some(vec![vec![BLANK; self.cols]; self.rows]);
        } else if self.alt.take().is_some() {
            (self.row, self.col) = self.saved;
        }
    }
}

fn line_of(row: &[Cell]) -> String {
    row.iter().filter(|c| c.ch != '\0').map(|c| c.ch).collect::<String>().trim_end().to_string()
}

#[cfg(test)]
#[path = "localsendscreen_tests.rs"]
mod tests;
