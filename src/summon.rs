// The ways back to the shelf that are not the bar mark: the keybind's own toggle, and the last five
// piles a clear left behind. Summon rule 3: the product is whole with any one way in, so none of
// these may depend on another, and rule 4: one shelf plus the last five piles, no switcher.
use crate::error::io_message;
use crate::jsondoc::{self, Json};
use crate::shelf::{now_ms, Shelf};
use crate::uistore;
use std::fs;
use std::path::{Path, PathBuf};

const DIR: &str = "omarchy/flea-shelf";
const PILES: &str = "piles.json";
const SUMMON: &str = "summon.json";
const LOCK: &str = "summon.lock";
// Rule 4: the last five, because clearing is what multi-shelf was really protecting against.
const KEPT_PILES: usize = 5;

pub struct Summon {
    piles: PathBuf,
    summon: PathBuf,
    lock: PathBuf,
}

impl Summon {
    pub fn user() -> Result<Summon, String> {
        Ok(Summon::at(&uistore::state_home()?))
    }

    pub fn at(state_dir: &Path) -> Summon {
        let dir = state_dir.join(DIR);
        Summon { piles: dir.join(PILES), summon: dir.join(SUMMON), lock: dir.join(LOCK) }
    }

    // The bind writes a count rather than a state, so the card and the file can never disagree about
    // whether the shelf is open: every write is one ring of the bell. The read and the write are one
    // step under the lock, or two presses in flight both read the same count and one press is lost.
    pub fn ring(&self) -> Result<u64, String> {
        uistore::make_dir(self.summon.parent().ok_or("the shelf has no folder to write in")?)?;
        let lock = self.take()?;
        crate::persistdoc::read(&self.summon, crate::persistdoc::summon)?;
        let next = self.rings() + 1;
        let doc = Json::Obj(vec![("summon".to_string(), Json::Num(next.to_string()))]);
        let written = uistore::replace(&self.summon, &jsondoc::render(&doc));
        self.give_back(lock);
        written?;
        Ok(next)
    }

    fn take(&self) -> Result<fs::File, String> {
        uistore::take_lock(&self.lock)
    }

    // Said rather than returned: the write either happened or it did not, and an unlock that failed
    // is not the answer to that question. The lock goes when this process does, either way.
    fn give_back(&self, lock: fs::File) {
        if let Err(e) = uistore::unlock(&lock) {
            eprintln!("flea: {} could not be unlocked: {}", self.lock.display(), io_message(&e));
        }
    }

    pub fn rings(&self) -> u64 {
        read_doc(&self.summon, crate::persistdoc::summon).get("summon").and_then(Json::as_f64).map(|n| n as u64).unwrap_or(0)
    }

    // Read by this file's own tests, which write a pile file the verbs then have to survive.
    #[cfg(test)]
    pub fn piles_file(&self) -> &Path {
        &self.piles
    }

    // Newest first. The five the board draws is what keep() writes, not what this trims on the way out.
    pub fn piles(&self) -> Vec<Json> {
        read_doc(&self.piles, crate::persistdoc::piles)
            .get("piles")
            .and_then(Json::as_array)
            .map(<[Json]>::to_vec)
            .unwrap_or_default()
    }

    pub fn keep(&self, items: Vec<Json>, at_ms: u64) -> Result<(), String> {
        if items.is_empty() {
            return Ok(());
        }
        uistore::make_dir(self.piles.parent().ok_or("the shelf has no folder to write in")?)?;
        let lock = self.take()?;
        crate::persistdoc::read(&self.piles, crate::persistdoc::piles)?;
        let mut piles = self.piles();
        piles.insert(0, Json::Obj(vec![("at".to_string(), Json::Num(at_ms.to_string())), ("items".to_string(), Json::Arr(items))]));
        piles.truncate(KEPT_PILES);
        let written = self.write(piles);
        self.give_back(lock);
        written
    }

    // Taking a pile out of the history is what restoring it means: the same pile cannot be restored
    // twice from one entry, and what it replaces takes its place in the list.
    pub fn restore_pile(&self, index: usize) -> Result<Vec<Json>, String> {
        uistore::make_dir(self.piles.parent().ok_or("the shelf has no folder to write in")?)?;
        let lock = self.take()?;
        crate::persistdoc::read(&self.piles, crate::persistdoc::piles)?;
        let mut piles = self.piles();
        if index >= piles.len() {
            self.give_back(lock);
            return Err("that pile is not one this shelf kept".to_string());
        }
        let taken = piles.remove(index);
        let written = self.write(piles);
        self.give_back(lock);
        written?;
        Ok(taken.get("items").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default())
    }

    fn write(&self, piles: Vec<Json>) -> Result<(), String> {
        let doc = Json::Obj(vec![("piles".to_string(), Json::Arr(piles))]);
        uistore::replace(&self.piles, &jsondoc::render(&doc))
    }
}

fn read_doc(path: &Path, shape: crate::persistdoc::Shape) -> Json {
    crate::persistdoc::read(path, shape).unwrap_or_else(|_| crate::persistdoc::empty())
}

// flea shelf clear: the pile becomes the last pile, which is the whole of rule 6 on the Keys board.
pub fn clear() -> i32 {
    let (shelf, summon) = match pair() {
        Ok(pair) => pair,
        Err(code) => return code,
    };
    let taken = match shelf.clear() {
        Ok(taken) => taken,
        Err(e) => return failed(&e),
    };
    let count = taken.len();
    // Two files, so the second write failing has to undo the first: a pile that could not be kept
    // goes back on the shelf rather than being lost between them.
    if let Err(e) = summon.keep(taken.clone(), now_ms()) {
        match shelf.put(taken) {
            // The pile lock was given up between the two writes, so anything added in that window
            // is named rather than quietly replaced by the pile going back.
            Ok(added) if !added.is_empty() => {
                eprintln!("flea: the pile went back over {} the shelf had taken since", added.len());
            }
            Ok(_) => {}
            Err(back) => eprintln!("flea: the pile could not be kept and could not be put back ({})", back),
        }
        return failed(&e);
    }
    println!("{}", count);
    0
}

// flea shelf restore [n]: the newest kept pile by default, or the nth the card's menu offered.
pub fn restore(rest: &[String]) -> i32 {
    let index = match chosen_index(rest) {
        Ok(index) => index,
        Err(e) => {
            eprintln!("flea: {}", e);
            return 2;
        }
    };
    match restore_at(index) {
        Ok(count) => {
            println!("{}", count);
            0
        }
        Err(e) => failed(&e),
    }
}

// The restore itself, which `flea shelf undo` also reaches when a clear is the newer of the two
// things it could reverse. It answers with the count rather than printing it, because the two verbs
// do not say the same sentence about it.
pub fn restore_at(index: usize) -> Result<usize, String> {
    // Not through pair(): that one reports by printing, and this one answers its caller instead.
    let (shelf, summon) = match (Shelf::user(), Summon::user()) {
        (Ok(shelf), Ok(summon)) => (shelf, summon),
        (Err(e), _) | (_, Err(e)) => return Err(e),
    };
    let pile = summon.restore_pile(index)?;
    let count = pile.len();
    // The history has already given the pile up, so a shelf that will not take it has to give it back.
    let was = match shelf.put(pile.clone()) {
        Ok(was) => was,
        Err(e) => {
            if let Err(back) = summon.keep(pile, now_ms()) {
                eprintln!("flea: {}", pile_lost(&back));
            }
            return Err(e);
        }
    };
    if let Err(e) = summon.keep(was, now_ms()) {
        return Err(format!("the pile is on the shelf, and what it replaced could not be kept ({})", e));
    }
    Ok(count)
}

// Both halves refused the pile: the shelf would not take it and the history would not keep it either.
fn pile_lost(back: &str) -> String {
    format!("the pile could not be restored, and it could not go back into the history ({})", back)
}

// The card's menu numbers its rows from one, so the argument is 1-based and the history is not.
pub fn chosen_index(rest: &[String]) -> Result<usize, String> {
    let Some(asked) = rest.first() else { return Ok(0) };
    match asked.parse::<usize>() {
        Ok(n) if n >= 1 => Ok(n - 1),
        _ => Err(format!("shelf restore takes a pile number from 1, and {} is not one", asked)),
    }
}

// flea shelf piles: what the card's Recent piles rows say, the time then how many it held.
pub fn piles() -> i32 {
    let summon = match Summon::user() {
        Ok(summon) => summon,
        Err(e) => return failed(&e),
    };
    for pile in summon.piles() {
        let at = pile.get("at").and_then(Json::as_f64).unwrap_or(0.0) as u64;
        let count = pile.get("items").and_then(Json::as_array).map(<[Json]>::len).unwrap_or(0);
        println!("{} {}", at, count);
    }
    0
}

pub fn toggle() -> i32 {
    match Summon::user().and_then(|summon| summon.ring()) {
        Ok(_) => 0,
        Err(e) => failed(&e),
    }
}

// flea shelf bind: the chord that opens the shelf, so the empty card names the bind only once it is
// installed. The user's own hypr config owns that line; this only reads it. Under Omarchy's Lua
// config `hyprctl binds` reports every bind as `dispatcher: __lua` with a number for its argument,
// so the command a bind runs is not in the compositor's own answer at all: the config text is.
pub fn bind() -> i32 {
    let Ok(path) = crate::userfile::config_home().map(|dir| dir.join("hypr/bindings.lua")) else {
        return 0;
    };
    let text = fs::read_to_string(path).unwrap_or_default();
    if let Some(chord) = summon_chord(&text) {
        println!("{}", chord);
    }
    0
}

// Sample input, one line of ~/.config/hypr/bindings.lua in Omarchy's own house syntax:
// o.bind("SUPER + D", "Drop shelf", "flea shelf toggle")
pub fn summon_chord(config: &str) -> Option<String> {
    for line in config.lines() {
        let line = line.trim();
        if line.starts_with("--") || !line.contains("shelf toggle") {
            continue;
        }
        let Some(chord) = line.split('"').nth(1) else { continue };
        let spelled: String = chord.split('+').map(|part| part.trim().to_lowercase()).collect::<Vec<_>>().join("+");
        if !spelled.is_empty() {
            return Some(spelled);
        }
    }
    None
}

fn pair() -> Result<(Shelf, Summon), i32> {
    match (Shelf::user(), Summon::user()) {
        (Ok(shelf), Ok(summon)) => Ok((shelf, summon)),
        (Err(e), _) | (_, Err(e)) => Err(failed(&e)),
    }
}

fn failed(why: &str) -> i32 {
    eprintln!("flea: {}", why);
    2
}

#[cfg(test)]
#[path = "summon_tests.rs"]
mod tests;
