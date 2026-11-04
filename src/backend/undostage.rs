// A document staged for the file: each entry rendered once, so trimming counts bytes and a write is a join.
use super::undocodec::{decode_pairs, entry, get, stored_redo, VERSION};
use super::undoshare::Doc;
use crate::jsondoc::{parse, render_at, Json, INDENT};
use std::fmt::Write;

// An array item sits two levels in: the document object, then its undo or redo array.
const ITEM_DEPTH: usize = 2;

// What one parse of the file says: a document, a file only a newer writer can own, or garbage.
pub(crate) enum Read {
    Doc(Doc),
    Newer,
    Malformed,
}

// Sample input: {"v":3,"gen":1,"undo":[],"redo":[]}
pub(crate) fn read(text: &str) -> Read {
    #[cfg(test)]
    super::undoprobe::decode();
    let Ok(root) = parse(text) else { return Read::Malformed };
    let Some(pairs) = root.as_object() else { return Read::Malformed };
    if pairs.iter().any(|(key, _)| !["v", "gen", "jobGen", "undo", "redo"].contains(&key.as_str())) {
        return Read::Malformed;
    }
    match get(pairs, "v") {
        Some(Json::Num(literal)) if literal.parse::<u64>().is_ok_and(|v| v > VERSION as u64) => return Read::Newer,
        // v1 files predate the barrier kind and v2 the merge kinds; both still read, anything else is foreign.
        Some(Json::Num(literal)) if literal == "1" || literal == "2" || literal == &VERSION.to_string() => {}
        _ => return Read::Malformed,
    }
    match decode_pairs(pairs) {
        Some(doc) => Read::Doc(doc),
        None => Read::Malformed,
    }
}

// Every entry rendered once, each as it reads inside the document, so a size is a sum and a write is a join.
fn encode_pieces(doc: &Doc) -> (Vec<String>, Vec<String>) {
    let undo = doc.undo.iter().map(|e| render_at(&entry(e), ITEM_DEPTH)).collect();
    let redo = doc.redo.iter().map(|r| render_at(&stored_redo(r), ITEM_DEPTH)).collect();
    (undo, redo)
}

// The document's generation and its rendered undo and redo items, in file order.
pub(crate) struct Staged {
    push_gen: u64,
    job_gen: u64,
    undo: Vec<String>,
    redo: Vec<String>,
}

impl Staged {
    pub(crate) fn new(doc: &Doc) -> Staged {
        #[cfg(test)]
        super::undoprobe::render();
        let (undo, redo) = encode_pieces(doc);
        Staged {
            push_gen: doc.push_gen,
            job_gen: doc.job_gen,
            undo,
            redo,
        }
    }

    pub(crate) fn len(&self) -> u64 {
        let mut tally = Tally(0);
        write_document(
            &mut tally,
            self.push_gen,
            self.job_gen,
            &self.undo,
            &self.redo,
        );
        tally.0 as u64
    }

    // Drops oldest undo then oldest redo until the size fits; the newest record never goes.
    pub(crate) fn trim_to(&mut self, cap: u64) {
        while self.len() > cap {
            if self.undo.len() > 1 {
                self.undo.remove(0);
            } else if !self.redo.is_empty() {
                self.redo.remove(0);
            } else {
                break;
            }
        }
    }

    pub(crate) fn fits(&self, cap: u64) -> bool {
        self.len() <= cap
    }

    // True when a redo record was there to drop.
    pub(crate) fn drop_newest_redo(&mut self) -> bool {
        self.redo.pop().is_some()
    }

    pub(crate) fn text(&self) -> String {
        let mut out = String::new();
        write_document(
            &mut out,
            self.push_gen,
            self.job_gen,
            &self.undo,
            &self.redo,
        );
        out
    }
}

// Counts the bytes write_document would write without building them.
struct Tally(usize);

impl Write for Tally {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.0 += text.len();
        Ok(())
    }
}

fn write_list(out: &mut impl Write, pieces: &[String]) {
    if pieces.is_empty() {
        let _ = out.write_str("[]");
        return;
    }
    let _ = out.write_char('[');
    for (index, piece) in pieces.iter().enumerate() {
        let _ = write!(out, "{}\n{}{}", if index > 0 { "," } else { "" }, INDENT.repeat(ITEM_DEPTH), piece);
    }
    let _ = write!(out, "\n{}]", INDENT);
}

// Renders v, gen, optional jobGen, undo and redo in that order, then one newline.
fn write_document(
    out: &mut impl Write,
    push_gen: u64,
    job_gen: u64,
    undo: &[String],
    redo: &[String],
) {
    let _ = write!(
        out,
        "{{\n{}\"v\": {},\n{}\"gen\": {},\n",
        INDENT, VERSION, INDENT, push_gen
    );
    if job_gen != 0 {
        let _ = writeln!(out, "{}\"jobGen\": {},", INDENT, job_gen);
    }
    let _ = write!(out, "{}\"undo\": ", INDENT);
    write_list(out, undo);
    let _ = write!(out, ",\n{}\"redo\": ", INDENT);
    write_list(out, redo);
    let _ = out.write_str("\n}\n");
}
