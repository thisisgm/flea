// Keys board: z is undo in every Flea surface, the shelf included. Two things here can be the last
// thing that happened, a move out of the pile and a clear, so this owns both and reverses whichever
// is newer. The move half is a one-step journal on disk, because `flea shelf move` is a process that
// has already exited by the time the card asks for its reversal.
use crate::backend::undo::{born_differs, move_back, ItemIdentity, Step};
use crate::error::io_message;
use crate::jsondoc::{self, Json};
use crate::shelf::Shelf;
use crate::summon::Summon;
use crate::uistore;
use std::fs;
use std::path::{Path, PathBuf};

const DIR: &str = "omarchy/flea-shelf";
const MOVES: &str = "undo.json";
const LOCK: &str = "undo.lock";

// One item a move carried, and the identity of what landed, which is the journal's own same_item
// test: undo refuses to walk a stranger back to a path it never came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Move {
    pub from: String,
    pub to: String,
    pub dev: u64,
    pub ino: u64,
    pub kind: u32,
    // None where the filesystem keeps no birth time, or the row predates it.
    pub born: Option<(u64, u32)>,
}

pub struct Moves {
    file: PathBuf,
    lock: PathBuf,
}

impl Moves {
    pub fn user() -> Result<Moves, String> {
        Ok(Moves::at(&uistore::state_home()?))
    }

    pub fn at(state_dir: &Path) -> Moves {
        let dir = state_dir.join(DIR);
        Moves { file: dir.join(MOVES), lock: dir.join(LOCK) }
    }

    // One record, not a history: the card offers one step back, so a second move replaces the first.
    pub fn record(&self, moves: &[Move], at_ms: u64) -> Result<(), String> {
        if moves.is_empty() {
            return Ok(());
        }
        let dir = self.file.parent().ok_or("the shelf has no folder to write in")?;
        uistore::make_dir(dir)?;
        let lock = uistore::take_lock(&self.lock)?;
        crate::persistdoc::read(&self.file, moves_shape)?;
        let written = uistore::replace(&self.file, &jsondoc::render(&doc_of(moves, at_ms)));
        give_back(&self.lock, lock);
        written
    }

    // When the move happened, which is the only question the two halves of undo ask of each other.
    pub fn at_ms(&self) -> u64 {
        read_doc(&self.file).get("at").and_then(Json::as_f64).unwrap_or(0.0) as u64
    }

    // Read and remove as one step, the way the pane's journal pops an entry before reversing it: a
    // step that fails stops the rest rather than leaving a record that would try the done half again.
    pub fn take(&self) -> Result<Vec<Move>, String> {
        if !self.file.exists() {
            return Ok(Vec::new());
        }
        let lock = uistore::take_lock(&self.lock)?;
        let moves = moves_of(&crate::persistdoc::read(&self.file, moves_shape)?);
        let removed = match fs::remove_file(&self.file) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{} could not be spent: {}", self.file.display(), io_message(&e))),
        };
        give_back(&self.lock, lock);
        removed?;
        Ok(moves)
    }
}

// Identity tokens use strings to avoid f64 precision loss; unknown birth time is omitted.
fn doc_of(moves: &[Move], at_ms: u64) -> Json {
    let rows = moves
        .iter()
        .map(|m| {
            let mut fields = vec![
                ("from".to_string(), Json::Str(m.from.clone())),
                ("to".to_string(), Json::Str(m.to.clone())),
                ("dev".to_string(), Json::Str(m.dev.to_string())),
                ("ino".to_string(), Json::Str(m.ino.to_string())),
                ("kind".to_string(), Json::Str(m.kind.to_string())),
            ];
            if let Some((sec, nsec)) = m.born {
                fields.push(("born".to_string(), Json::Arr(vec![Json::Str(sec.to_string()), Json::Str(nsec.to_string())])));
            }
            Json::Obj(fields)
        })
        .collect();
    Json::Obj(vec![
        ("at".to_string(), Json::Num(at_ms.to_string())),
        ("moves".to_string(), Json::Arr(rows)),
    ])
}

// The moves file shape: every row a whole move, so a newer writer's extra field is never dropped.
pub(crate) fn moves_shape(doc: &Json) -> bool {
    use crate::persistdoc::{count, fields};
    fields(doc, &["at", "moves"], &[])
        && count(doc, "at")
        && doc
            .get("moves")
            .and_then(Json::as_array)
            .is_some_and(|rows| {
                rows.len() == moves_of(doc).len()
                    && rows
                        .iter()
                        .all(|row| fields(row, &["from", "to", "dev", "ino", "kind"], &["born"]))
            })
}

// Sample input: {"at":1789426926000,"moves":[{"from":"/home/gm/a.txt","to":"/home/gm/Work/a.txt","dev":"66306","ino":"41","kind":"32768","born":["1789426900","5000"]}]}
// Reject incomplete rows and malformed birth pairs; accept legacy rows without birth time.
pub fn moves_of(doc: &Json) -> Vec<Move> {
    let rows = doc.get("moves").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default();
    rows.iter()
        .filter_map(|row| {
            let from = row.get("from").and_then(Json::as_str)?;
            let to = row.get("to").and_then(Json::as_str)?;
            let dev = number(row, "dev")?;
            let ino = number(row, "ino")?;
            let kind = number(row, "kind")? as u32;
            let born = match row.get("born") {
                None => None,
                Some(pair) => Some(born_of_row(pair)?),
            };
            if from.is_empty() || to.is_empty() {
                return None;
            }
            Some(Move { from: from.to_string(), to: to.to_string(), dev, ino, kind, born })
        })
        .collect()
}

fn number(row: &Json, name: &str) -> Option<u64> {
    row.get(name).and_then(token)
}

// Sample input: "41", an identity token written as a string.
fn token(value: &Json) -> Option<u64> {
    value.as_str().and_then(|text| text.parse::<u64>().ok())
}

// Sample input: ["1789426900","5000"], seconds then nanoseconds.
fn born_of_row(pair: &Json) -> Option<(u64, u32)> {
    match pair.as_array()? {
        [sec, nsec] => Some((token(sec)?, u32::try_from(token(nsec)?).ok()?)),
        _ => None,
    }
}

// What a shelf move leaves behind for a later `z`: the engine's own Moved steps, which carry the
// landing path a collision renamed and the identity it recorded at the moment it landed.
pub fn moves_from(steps: &[Step]) -> Vec<Move> {
    steps
        .iter()
        .filter_map(|step| match step {
            Step::Moved { from, to, after, .. } => {
                let (dev, ino, kind) = after.parts();
                Some(Move {
                    from: from.to_string_lossy().to_string(),
                    to: to.to_string_lossy().to_string(),
                    dev,
                    ino,
                    kind,
                    born: after.born(),
                })
            }
            _ => None,
        })
        .collect()
}

// flea shelf undo: what z presses. The newer of the two is what one press reverses, so pressing it
// twice after a move and a clear walks back through both, newest first.
pub fn undo() -> i32 {
    let (moves, summon) = match (Moves::user(), Summon::user()) {
        (Ok(moves), Ok(summon)) => (moves, summon),
        (Err(e), _) | (_, Err(e)) => return failed(&e),
    };
    let moved_at = moves.at_ms();
    let cleared_at = newest_pile(&summon);
    if moved_at == 0 && cleared_at == 0 {
        println!("none 0");
        return 0;
    }
    if cleared_at > moved_at {
        return match crate::summon::restore_at(0) {
            Ok(count) => {
                println!("pile {}", count);
                0
            }
            Err(e) => failed(&e),
        };
    }
    reverse(&moves)
}

fn newest_pile(summon: &Summon) -> u64 {
    summon.piles().first().and_then(|pile| pile.get("at").and_then(Json::as_f64)).unwrap_or(0.0) as u64
}

// Newest first, the order the journal reverses in, so a move that landed two items in the same name
// unwinds in the order it wound.
fn reverse(moves: &Moves) -> i32 {
    let shelf = match Shelf::user() {
        Ok(shelf) => shelf,
        Err(e) => return failed(&e),
    };
    let steps = match moves.take() {
        Ok(steps) => steps,
        Err(e) => return failed(&e),
    };
    let mut back: Vec<&Move> = Vec::new();
    let mut stopped = None;
    for step in steps.iter().rev() {
        match put_back(step) {
            Ok(()) => back.push(step),
            Err(e) => {
                stopped = Some(e);
                break;
            }
        }
    }
    let count = back.len();
    let settled = settle_back(&shelf, &back);
    if let Some(e) = stopped {
        eprintln!("flea: {}", e);
        return 2;
    }
    if let Err(e) = settled {
        eprintln!("flea: the files are back and the shelf is not ({})", e);
        return 2;
    }
    println!("move {}", count);
    0
}

// The same refusal the pane's own undo makes: the item at the landing path has to be the one the
// move put there, or something else has taken that name since and this would carry it off.
fn put_back(step: &Move) -> Result<(), String> {
    let to = Path::new(&step.to);
    let meta = fs::symlink_metadata(to)
        .map_err(|e| format!("{} is not where the move left it: {}", step.to, io_message(&e)))?;
    let landed = ItemIdentity::record(&meta);
    let (dev, ino, kind) = landed.parts();
    if dev != step.dev || ino != step.ino || kind != step.kind || born_differs(step.born, landed.born()) {
        return Err(format!("{} was replaced since the move, so undo left it in place", step.to));
    }
    move_back(to, Path::new(&step.from)).map_err(|e| format!("{} could not go back ({})", step.from, e.msg))
}

// A move took its rows off the pile and re-pointed the pinned ones, so undoing it does both in
// reverse: a pin follows the file home, and every other row goes back on the shelf.
fn settle_back(shelf: &Shelf, back: &[&Move]) -> Result<(), String> {
    if back.is_empty() {
        return Ok(());
    }
    let repoint: Vec<(String, String)> = back.iter().map(|m| (m.to.clone(), m.from.clone())).collect();
    shelf.repoint(&repoint)?;
    // Reversed again, because the walk home takes the newest first and the rows go back on in the
    // order they were on in.
    let paths: Vec<String> = back.iter().rev().map(|m| m.from.clone()).collect();
    shelf.add(&paths)
}

fn read_doc(path: &Path) -> Json {
    crate::persistdoc::read(path, moves_shape).unwrap_or_else(|_| crate::persistdoc::empty())
}

// Said rather than returned, the way summon says it: the write either happened or it did not, and an
// unlock that failed is not the answer to that question.
fn give_back(path: &Path, lock: fs::File) {
    if let Err(e) = uistore::unlock(&lock) {
        eprintln!("flea: {} could not be unlocked: {}", path.display(), io_message(&e));
    }
}

fn failed(why: &str) -> i32 {
    eprintln!("flea: {}", why);
    2
}

#[cfg(test)]
#[path = "shelfundo_tests.rs"]
mod tests;
