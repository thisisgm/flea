//! One top-level list as flat entries (depth, marker, loose flag per list), and the quote and list blocks built from them.

use super::markdown_event::{Frame, Lead, Line, LineEvent, LineKind};
use super::markdown_leaf::{alert_title, pin_task_box, task_text};
use super::markdown_model::{Block, List, Quote};
use super::markdown_text::blank;
use std::collections::HashMap;

/// The disc an unordered item draws.
pub const BULLET: &str = "\u{2022}";

/// What a container's blocks need from their writer: the inline pass, and the blocks a run of lines holds (None past the nesting cap).
pub trait Host {
    fn inline(&mut self, text: &str) -> String;
    fn parts(&mut self, lines: &[String]) -> Option<Vec<Block>>;
}

/// One list now open at a depth: its marker, its start, its item count and whether a blank line loosened it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Level {
    pub mark: u8,
    pub start: u64,
    pub count: u64,
    pub loose: bool,
}

/// One flat entry: its depth, its lines, the list it belongs to, the frame that opened it, its content line count and its marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub depth: usize,
    pub lines: Vec<Line>,
    pub list: usize,
    pub frame_id: usize,
    pub content: usize,
    pub marker: String,
}

/// Item "a" holding a nested list "b" builds entries a (depth 0, a bullet) and b (depth 1).
#[derive(Debug, Default)]
pub struct Builder {
    pub entries: Vec<Entry>,
    pub lists: Vec<Level>,
    levels: Vec<Option<usize>>,
    heads: HashMap<usize, usize>,
    cur: Option<usize>,
    blank: bool,
}

impl Builder {
    fn sibling(&self, depth: usize, chain: &[Frame]) -> bool {
        matches!(self.levels.get(depth), Some(Some(lv)) if self.lists[*lv].mark == chain[depth].mark)
    }

    fn set_level(&mut self, depth: usize, list: usize) {
        self.levels.resize(depth.max(self.levels.len()), None);
        self.levels.truncate(depth);
        self.levels.push(Some(list));
    }

    fn open(&mut self, chain: &[Frame], depth: usize) -> usize {
        let frame = &chain[depth];
        let list = if self.sibling(depth, chain) {
            let lv = self.levels[depth].unwrap_or_default();
            self.lists[lv].count += 1;
            lv
        } else {
            self.lists.push(Level {
                mark: frame.mark,
                start: frame.start,
                count: 0,
                loose: false,
            });
            self.lists.len() - 1
        };
        self.set_level(depth, list);
        let level = &self.lists[list];
        let marker = if frame.ordered {
            format!("{}.", level.start + level.count)
        } else {
            BULLET.to_string()
        };
        self.entries.push(Entry {
            depth,
            lines: Vec::new(),
            list,
            frame_id: frame.id,
            content: 0,
            marker,
        });
        self.heads.insert(frame.id, self.entries.len() - 1);
        self.entries.len() - 1
    }

    // A blank line before a new item loosens the list the item joins, or the list whose item it nests in.
    fn loosen_opened(&mut self, chain: &[Frame], depth: usize) {
        let target = if self.sibling(depth, chain) {
            self.levels[depth]
        } else if depth > 0 {
            self.levels.get(depth - 1).copied().flatten()
        } else {
            None
        };
        if let Some(lv) = target {
            self.lists[lv].loose = true;
        }
    }

    /// Adds one line of the list with its container-free text.
    pub fn add(&mut self, event: &LineEvent, text: String) {
        let chain = event.chain;
        let lead: &Lead = event.lead;
        let line = Line::of(event, text);
        let from = lead.retained.min(lead.n);
        // A blank line opens no item; "-" alone opens an empty one.
        if event.kind == LineKind::Run && blank(&line.text) && from + 1 > lead.n {
            self.blank = true;
            if let Some(cur) = self.cur {
                self.entries[cur].lines.push(line);
            }
            return;
        }
        if from < lead.n {
            if self.blank {
                self.loosen_opened(chain, from);
            }
            for depth in from..lead.n {
                self.cur = Some(self.open(chain, depth));
            }
        } else {
            let Some(last) = lead.n.checked_sub(1) else {
                return;
            };
            let Some(&head) = self.heads.get(&chain[last].id) else {
                return;
            };
            let (head_frame, head_list) = (self.entries[head].frame_id, self.entries[head].list);
            let continues = self
                .cur
                .is_some_and(|cur| self.entries[cur].frame_id == head_frame);
            if !continues {
                self.levels.truncate(last + 1);
                self.entries.push(Entry {
                    depth: last,
                    lines: Vec::new(),
                    list: head_list,
                    frame_id: head_frame,
                    content: 0,
                    marker: String::new(),
                });
                self.cur = Some(self.entries.len() - 1);
                if self.blank {
                    self.lists[head_list].loose = true;
                }
            } else if let Some(cur) = self
                .cur
                .filter(|cur| self.blank && self.entries[*cur].content > 0)
            {
                let list = self.entries[cur].list;
                self.lists[list].loose = true;
            }
        }
        self.blank = false;
        if let Some(cur) = self.cur {
            self.entries[cur].content += 1;
            self.entries[cur].lines.push(line);
        }
    }
}

/// The text of the lines that draw, and which of them are code lines the inline pass never sees.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Kept {
    pub texts: Vec<String>,
    pub raw: Vec<bool>,
}

/// The lines that draw: hidden ones go, and an escaped definition keeps its bracket literal.
pub fn visible_lines(lines: &[Line]) -> Kept {
    let mut kept = Kept::default();
    for line in lines.iter().filter(|line| !line.hidden) {
        let text = if line.escaped {
            match line.text.find('[') {
                Some(at) => format!("{}\\[{}", &line.text[..at], &line.text[at + 1..]),
                None => {
                    let cut = line.text.char_indices().last().map_or(0, |(i, _)| i);
                    format!("{}\\[{}", &line.text[..cut], line.text)
                }
            }
        } else {
            line.text.clone()
        };
        kept.texts.push(text);
        kept.raw.push(line.raw);
    }
    kept
}

// Prose goes through the inline pass; a fence or indented code in an item or quote stays verbatim for the renderer.
fn inline_lines(kept: &Kept, host: &mut dyn Host) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut prose: Vec<&str> = Vec::new();
    for k in 0..=kept.texts.len() {
        if k < kept.texts.len() && !kept.raw[k] {
            prose.push(&kept.texts[k]);
            continue;
        }
        if !prose.is_empty() {
            parts.push(host.inline(&prose.join("\n")));
        }
        prose.clear();
        if k < kept.texts.len() {
            parts.push(kept.texts[k].clone());
        }
    }
    parts.join("\n")
}

/// A container's text, or its blocks when it holds more than prose.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Held {
    pub text: String,
    pub parts: Option<Vec<Block>>,
}

/// Lines "a" and "```" hold parts, prose alone its one text, and nothing to draw an empty text.
pub fn content(kept: &Kept, host: &mut dyn Host) -> Held {
    if blank(&kept.texts.join("\n")) {
        return Held::default();
    }
    let Some(mut parts) = host.parts(&kept.texts) else {
        return Held {
            text: inline_lines(kept, host),
            parts: None,
        };
    };
    match parts.as_slice() {
        [] => Held::default(),
        [Block::Run { maths, .. }] if maths.is_empty() => Held {
            text: parts
                .pop()
                .map(|block| block.text().to_string())
                .unwrap_or_default(),
            parts: None,
        },
        _ => Held {
            text: String::new(),
            parts: Some(parts),
        },
    }
}

/// A quote line and the quote depth it sits at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuoteLine {
    pub line: Line,
    pub depth: usize,
}

/// One quote block per run of lines at one nesting depth; the depth shows past the first level, joined after the first block.
pub fn quote_blocks(all: &[QuoteLine], host: &mut dyn Host) -> Vec<Block> {
    let mut out = Vec::new();
    let mut from = 0;
    while from < all.len() {
        let depth = all[from].depth;
        let to = from
            + all[from..]
                .iter()
                .take_while(|line| line.depth == depth)
                .count();
        let lines: Vec<Line> = all[from..to].iter().map(|line| line.line.clone()).collect();
        let mut quote = visible_lines(&lines);
        // A quote with nothing to draw is still a block: a browser gives it its margin.
        if from == 0 && !quote.texts.is_empty() && !quote.raw[0] {
            if let Some(title) = alert_title(&quote.texts[0]) {
                quote.texts[0] = title;
            }
        }
        let held = content(&quote, host);
        out.push(Block::Quote(Quote {
            text: held.text,
            parts: held.parts,
            depth: (depth > 1).then_some(depth),
            joined: from > 0,
        }));
        from = to;
    }
    out
}

/// One list as a block; depths, markers and gaps only when nesting or looseness needs them, parts when an item holds more than prose.
pub fn list_block(builder: &Builder, ordered: bool, start: u64, host: &mut dyn Host) -> List {
    let mut list = List {
        ordered,
        start,
        ..List::default()
    };
    let (mut depths, mut markers, mut gaps, mut parts) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut nested, mut loose, mut holds) = (false, false, false);
    for entry in &builder.entries {
        // An item that opens on a blank line starts at its first line of text.
        let first = entry
            .lines
            .iter()
            .take(entry.lines.len().saturating_sub(1))
            .take_while(|line| blank(&line.text))
            .count();
        let mut kept = visible_lines(&entry.lines[first..]);
        while kept.texts.last().is_some_and(|text| blank(text)) {
            kept.texts.pop();
            kept.raw.pop();
        }
        if !entry.marker.is_empty() && !kept.texts.is_empty() && !kept.raw[0] {
            kept.texts[0] = task_text(&kept.texts[0]);
        }
        let gap = builder.lists[entry.list].loose;
        nested = nested || entry.depth > 0 || entry.marker.is_empty();
        loose = loose || gap;
        let mut held = content(&kept, host);
        held.text = pin_task_box(&held.text);
        if let Some(Block::Run { text, .. }) =
            held.parts.as_mut().and_then(|parts| parts.first_mut())
        {
            *text = pin_task_box(text);
        }
        holds = holds || held.parts.is_some();
        list.items.push(held.text);
        parts.push(held.parts);
        depths.push(entry.depth);
        markers.push(entry.marker.clone());
        gaps.push(gap);
    }
    if nested || loose {
        list.depths = Some(depths);
        list.markers = Some(markers);
        list.gaps = Some(gaps);
    }
    if holds {
        list.parts = Some(parts);
    }
    list
}
