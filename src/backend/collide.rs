// Names a paste or a drop would land on: the question asked first, and the one choice the transfer carries.
use crate::backend::icons::Names;
use crate::backend::menu_actions::Selected;
use crate::backend::mime::Db;
use crate::backend::ops::free_copy_path;
use crate::backend::opsdispatch::Ops;
use crate::backend::opsreq::{usable_dest, OpMsg, ALREADY_THERE};
use crate::backend::undo::{ItemIdentity, Step};
use crate::json::{escape, field_str, field_usize};
use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// The card lists this many names and says how many more there are, so the answer never carries more.
pub const SHOWN: usize = 3;
// The word Duplicate already uses, so a kept copy and a duplicate are named by one rule.
const KEEP_WORD: &str = "copy";
pub const TRASH_REFUSED: &str = "the item already there could not be moved to Trash, so nothing was replaced";
pub const HOLDS_SOURCE: &str = "the item already there holds the one being moved in, so it was not replaced";
pub const LINKS_HERE: &str = "the incoming link points at the item already there, so it was not replaced";
pub const NO_FREE_NAME: &str = "every copy name for this item is already taken";
// copyfile's own word for a cancelled item, which is what the transfer counts a cancel by.
pub(crate) const CANCELLED: &str = "cancelled";
// What a failed put-back adds to the item's error, a cancel's included, so the old item is never reported as untouched.
pub(crate) const STILL_IN_TRASH: &str = "; the item it replaced is still in the Trash";
// Sample output: "the link at /d/b.txt could not be removed: stale; the item it replaced is still in the Trash".
pub(crate) fn kept_in_trash(error: &str) -> String {
    format!("{error}{STILL_IN_TRASH}")
}

// The operator's one answer, for every name the question listed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Collide {
    Refuse,
    Keep,
    Replace,
    Skip,
    Merge,
}

impl Collide {
    // Anything that is not one of the four choices refuses, so a malformed word can never replace.
    fn from_word(word: &str) -> Collide {
        match word {
            "keep" => Collide::Keep,
            "replace" => Collide::Replace,
            "skip" => Collide::Skip,
            "merge" => Collide::Merge,
            _ => Collide::Refuse,
        }
    }
}

// What the question saw: each colliding source, and the item that held its name in dest at that moment.
pub struct Question {
    pub(crate) id: usize,
    pub(crate) dest: PathBuf,
    pub(crate) seen: HashMap<PathBuf, ItemIdentity>,
    // Sources landing on a name an earlier source of the batch takes under dest's case and FAT rules, asked like a name dest holds.
    pub(crate) twins: HashSet<PathBuf>,
    pub(crate) menu: Option<MenuCapture>,
    // The card's answer this question was asked under, so the merge walk that follows it compares the names the transfer will write.
    pub(crate) fat: Option<super::fatnames::Choice>,
}

// A menu's selection as its question saw it: Copy to closes its dialog, which expires the live one, before the answer lands.
#[derive(Clone)]
pub(crate) struct MenuCapture {
    id: usize,
    pub(crate) items: Vec<Selected>,
    destination: Option<Selected>,
}

// The transfer request's fields, before the question they name has been looked up.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ask {
    pub(crate) word: Option<String>,
    pub(crate) id: usize,
    pub(crate) inner: Option<String>,
    pub(crate) fat: Option<super::fatnames::Choice>,
}

impl Ask {
    // Sample input: {"c":"transfer","op":"copy","paths":["/home/gm/a.png"],"dest":"/home/gm/Pictures","collide":"replace","collideId":7}
    // Sample input: {"c":"transfer","op":"copy","paths":["/home/gm/trip"],"dest":"/home/gm/Pictures","collide":"merge","collideId":8,"inner":"keep"}
    pub fn parse(line: &str) -> Ask {
        Ask { word: field_str(line, "collide"), id: field_usize(line, "collideId").unwrap_or(0),
              inner: field_str(line, "inner"), fat: super::fatnames::Choice::parse(line) }
    }

    // The choice covers only the question this request names, asked about this same destination.
    pub fn policy(self, question: Option<Question>, dest: &Path) -> Policy {
        let (seen, twins) = match question {
            Some(q) if self.answers(&q, dest) => (q.seen, q.twins),
            _ => (HashMap::new(), HashSet::new()),
        };
        let inner = self.inner.as_deref().map(Collide::from_word);
        let on_fat = super::fatnames::answered_on_fat(self.fat, dest);
        Policy { choice: self.word.as_deref().map(Collide::from_word), inner, fat: self.fat, on_fat,
                 seen: Arc::new(seen), twins: Arc::new(twins), ..Policy::default() }
    }

    fn answers(&self, question: &Question, dest: &Path) -> bool {
        self.id != 0 && question.id == self.id && question.dest == dest
    }
}

// The selection a menu transfer runs on: the one its question captured, else the menu's live one.
pub(crate) fn menu_sources(ops: &Ops, id: usize, dest: &str, collide: &Ask) -> Result<(Vec<Selected>, Option<Selected>), String> {
    let captured = ops.question.as_ref().filter(|q| collide.answers(q, Path::new(dest))).and_then(|q| q.menu.as_ref());
    if let Some(menu) = captured.filter(|menu| menu.id == id) {
        return Ok((menu.items.clone(), menu.destination.clone()));
    }
    ops.menuactions.as_ref().ok_or_else(|| super::menu_actions::SELECTION_CHANGED.to_string())
        .and_then(|menu| Ok((menu.selection(id)?, menu.provider_destination(id, Path::new(dest))?)))
}

// An expired selection captures nothing, so the question answers no collision and the transfer its own error.
fn capture(ops: &Ops, id: usize, dest: &str) -> Option<MenuCapture> {
    let menu = ops.menuactions.as_ref()?;
    Some(MenuCapture { id, items: menu.selection(id).ok()?, destination: menu.provider_destination(id, Path::new(dest)).ok()? })
}

// No choice at all is today's transfer exactly; any choice also turns on the same-folder rule.
#[derive(Clone, Default)]
pub struct Policy {
    choice: Option<Collide>,
    inner: Option<Collide>,
    fat: Option<super::fatnames::Choice>,
    // Whether dest sits on a FAT-family filesystem, read once per transfer: every item lands in one folder.
    on_fat: bool,
    seen: Arc<HashMap<PathBuf, ItemIdentity>>,
    twins: Arc<HashSet<PathBuf>>,
    batch: Vec<PathBuf>,
    // Every source of the batch and every folder above it, resolved on the first Replace, so one lookup says whether a trash would take a source.
    held: OnceCell<HashSet<PathBuf>>,
}

// Where one item goes; replace says what holds that path now goes to Trash first.
#[derive(Debug, PartialEq)]
pub enum Place {
    Land { to: PathBuf, replace: bool },
    Skip,
    Refuse(String),
}

impl Policy {
    // Replace checks each item against the whole batch, so the batch is kept only when there is a Replace to check.
    pub fn for_batch(mut self, paths: &[String]) -> Policy {
        if self.choice == Some(Collide::Replace) || self.choice == Some(Collide::Merge) {
            self.batch = paths.iter().map(PathBuf::from).collect();
        }
        self
    }

    // Rename on a FAT destination also covers the names inside the folders it copies: the walk below the top writes them under the same rule.
    pub(crate) fn renames_inside(&self) -> bool {
        super::fatnames::renames(self.fat, self.on_fat)
    }

    // A merge transfer folds folders together; every other choice copies or moves top-level items.
    pub(crate) fn is_merge(&self) -> bool {
        self.choice == Some(Collide::Merge)
    }

    // The second card's answer as its own policy over the same question, so inner names keep the race story.
    pub(crate) fn inner(&self) -> Policy {
        Policy { choice: self.inner, inner: None, fat: self.fat, on_fat: self.on_fat, seen: Arc::clone(&self.seen), twins: Arc::clone(&self.twins), batch: self.batch.clone(), held: OnceCell::new() }
    }

    // The question listed this source and its name still holds the item it saw there, or it is a twin that Skip leaves, as the sweep counted, or whose earlier twin landed.
    pub(crate) fn covers(&self, src: &Path, dst: &Path) -> bool {
        covers(&self.seen, src, dst) || (self.twins.contains(src) && (self.choice == Some(Collide::Skip) || dst.symlink_metadata().is_ok()))
    }

    // Skip's test for the sweep beside the copy, which leaves out of the batch's total what place will leave in place.
    pub fn skipping(&self) -> Option<Skipping> {
        (self.choice == Some(Collide::Skip)).then(|| Skipping { seen: Arc::clone(&self.seen), twins: Arc::clone(&self.twins) })
    }

    // here says the item already lives in dest; a name the question did not see is left to the exclusive create, which refuses it.
    pub fn place(&self, src: &Path, dst: PathBuf, here: bool, moving: bool) -> Place {
        // A FAT answer remaps the leaf before any collision rule sees it; a skip never lands at all.
        let Some((dst, here)) = super::fatnames::apply_dst(self.fat, self.on_fat, dst, here) else {
            return Place::Skip;
        };
        let Some(choice) = self.choice else {
            return if here { Place::Refuse(ALREADY_THERE.to_string()) } else { Place::Land { to: dst, replace: false } };
        };
        if here {
            // A move onto itself changes nothing, and a copy into its own folder is a Duplicate.
            return if moving { Place::Skip } else { keep_both(dst) };
        }
        if !self.covers(src, &dst) {
            return Place::Land { to: dst, replace: false };
        }
        match choice {
            Collide::Refuse => Place::Land { to: dst, replace: false },
            Collide::Keep => keep_both(dst),
            Collide::Skip => Place::Skip,
            Collide::Replace => match self.guard(&dst, src) {
                Some(reason) => Place::Refuse(reason.to_string()),
                None => Place::Land { to: dst, replace: true },
            },
            Collide::Merge => super::merge::land_unmerged(dst),
        }
    }

    // What a trash of dst would take with it: any source this batch names, or the item an incoming link resolves to.
    fn guard(&self, dst: &Path, src: &Path) -> Option<&'static str> {
        let there = resolved_parent(dst)?;
        if self.held.get_or_init(|| held_sources(&self.batch)).contains(&there) {
            return Some(HOLDS_SOURCE);
        }
        super::collidelink::links_into(src, &there).then_some(LINKS_HERE)
    }
}

// The question listed this source and its name still holds the item the question saw there: the one test every choice waits on.
pub(crate) fn covers(seen: &HashMap<PathBuf, ItemIdentity>, src: &Path, dst: &Path) -> bool {
    seen.get(src).is_some_and(|seen| ItemIdentity::inspect(dst).is_ok_and(|current| seen.same_item(&current)))
}

// What a Skip leaves in place, shared with the sweep's own thread so the first byte never waits on its lstat per name.
pub struct Skipping {
    seen: Arc<HashMap<PathBuf, ItemIdentity>>,
    twins: Arc<HashSet<PathBuf>>,
}

impl Skipping {
    pub fn leaves(&self, src: &Path, dst: &Path) -> bool {
        covers(&self.seen, src, dst) || self.twins.contains(src)
    }
}

// Each source and its ancestors; an ancestor already in the set had its own ancestors added with it.
fn held_sources(batch: &[PathBuf]) -> HashSet<PathBuf> {
    let mut held = HashSet::new();
    // A selection shares a folder or a few, so each folder is resolved once rather than once per item.
    let mut folders: HashMap<&Path, Option<PathBuf>> = HashMap::new();
    for src in batch {
        let (Some(parent), Some(name)) = (src.parent(), src.file_name()) else { continue };
        let folder = folders.entry(parent).or_insert_with(|| parent.canonicalize().ok());
        let Some(resolved) = folder.as_ref().map(|folder| folder.join(name)) else { continue };
        for ancestor in resolved.ancestors() {
            if !held.insert(ancestor.to_path_buf()) {
                break;
            }
        }
    }
    held
}

fn keep_both(dst: PathBuf) -> Place {
    match free_copy_path(&dst, KEEP_WORD) {
        Some(free) => Place::Land { to: free, replace: false },
        None => Place::Refuse(NO_FREE_NAME.to_string()),
    }
}

// The parent resolved and the last component left as it is, so a symlink names itself and not its target.
fn resolved_parent(path: &Path) -> Option<PathBuf> {
    Some(path.parent()?.canonicalize().ok()?.join(path.file_name()?))
}

// The item already lives in dest: its name there is the item itself, through a symlinked folder too.
pub fn already_there(src: &Path, name: &str, dst: &Path, dest_real: &Path) -> bool {
    already_there_in(src, name, dst, dest_real, &mut Parents::default())
}

pub(crate) fn already_there_in(src: &Path, name: &str, dst: &Path, dest_real: &Path, parents: &mut Parents) -> bool {
    let src_here = match src.parent() {
        Some(parent) => parents.resolve(parent).join(name),
        None => src.to_path_buf(),
    };
    dst == src || dest_real.join(name) == src_here
}

// The put-back gate without the restore, so a deferred batch marks now and restores at settle.
pub(crate) fn put_back_vacant(dst: &Path, steps: &[Step], at: usize) -> bool {
    matches!(dst.symlink_metadata(), Err(e) if e.kind() == std::io::ErrorKind::NotFound) && steps.len() == at + 1
}

// A cancel, bare or with the put-back that failed after it: either way the batch stops, and only the bare one is a skip.
pub(crate) fn cancelled(msg: &str) -> bool {
    msg.strip_prefix(CANCELLED).is_some_and(|rest| rest.is_empty() || rest.starts_with(STILL_IN_TRASH))
}

// One colliding source as the card draws it: its name, and the kind its mark is chosen from.
struct Shown {
    name: String,
    dir: bool,
    mode: u32,
}

// The question itself, read-only: which sources would land on a name dest already holds, under their FAT-renamed names.
fn ask(id: usize, paths: &[String], dest: &str, fat: Option<super::fatnames::Choice>) -> (Question, Vec<Shown>) {
    let mut question = Question { id, dest: PathBuf::from(dest), seen: HashMap::new(), twins: HashSet::new(), menu: None, fat };
    let mut shown = Vec::new();
    // An unusable destination asks nothing: the transfer that follows answers its own error.
    let Ok(dest_path) = usable_dest(dest) else { return (question, shown) };
    let on_fat = super::fatnames::answered_on_fat(fat, &dest_path);
    let dest_real = dest_path.canonicalize().unwrap_or_else(|_| dest_path.clone());
    let (mut parents, table, mut claimed) = (Parents::default(), Table::read(&dest_path), super::fatnames::casefold::Claimed::new(&dest_path));
    for raw in paths {
        let src = Path::new(raw);
        if !src.is_absolute() {
            continue;
        }
        let Some(name) = src.file_name().and_then(|n| n.to_str()) else { continue };
        let Ok(meta) = src.symlink_metadata() else { continue };
        let Some((dst, leaf)) = super::fatnames::ask_leaf(fat, on_fat, &dest_path, name) else { continue };
        // Same-folder items never ask: a copy keeps both and a move stays put, see Policy::place.
        if already_there_in(src, &leaf, &dst, &dest_real, &mut parents) {
            continue;
        }
        let twin = claimed.taken(&leaf);
        let fresh = match identity_in(&dst, &table) {
            Ok(there) => question.seen.insert(src.to_path_buf(), there).is_none(),
            Err(_) if twin => question.twins.insert(src.to_path_buf()),
            Err(_) => continue,
        };
        if fresh && shown.len() < SHOWN {
            shown.push(Shown { name: leaf, dir: meta.is_dir(), mode: meta.mode() });
        }
    }
    (question, shown)
}

// Sample output: {"t":"collisions","id":7,"total":1,"names":[{"n":"a.png","d":false,"i":"image-x-generic"}],"mergeable":false}
fn collisions_line(id: usize, total: usize, shown: &[Shown], mergeable: bool, mime: &Db, icons: &Names) -> String {
    let names: Vec<String> = shown.iter().map(|s| format!(r#"{{"n":"{}","d":{},"i":"{}"}}"#,
        escape(&s.name), s.dir, escape(icons.icon_for(mime.lookup(&s.name), s.dir, s.mode)))).collect();
    format!(r#"{{"t":"collisions","id":{},"total":{},"names":[{}],"mergeable":{}}}"#, id, total, names.join(","), mergeable)
}

// A test's hold on the next question's thread, released once the requests that must come first are sent.
#[cfg(test)]
thread_local! {
    pub(crate) static HOLD: std::cell::RefCell<Option<std::sync::mpsc::Receiver<()>>> = const { std::cell::RefCell::new(None) };
}

// Measured 0.9 s for 100,000 local sources, a stall per network round trip on a mount, so the question runs beside the loop.
pub(crate) fn ask_beside(ops: &mut Ops, id: usize, menu_id: usize, named: Vec<String>, dest: &str, fat: Option<super::fatnames::Choice>, (mime, icons): (&Arc<Db>, &Arc<Names>)) {
    // Captured here and not on the thread, because the close that expires the live selection may be the very next request.
    let menu = if menu_id == 0 { None } else { capture(ops, menu_id, dest) };
    let paths = match (&menu, menu_id) {
        (Some(menu), _) => menu.items.iter().map(|item| item.path.to_string_lossy().to_string()).collect(),
        (None, 0) => named,
        (None, _) => Vec::new(),
    };
    ops.asked += 1;
    let (turn, tx, dest, mime, icons) = (ops.asked, ops.tx.clone(), dest.to_string(), Arc::clone(mime), Arc::clone(icons));
    #[cfg(test)]
    let hold = HOLD.with(|slot| slot.borrow_mut().take());
    std::thread::spawn(move || {
        #[cfg(test)]
        if let Some(hold) = hold { let _ = hold.recv(); }
        // The FAT names answer comes before any collision is asked, so nothing is asked about a name that is about to change.
        if let Some(line) = fat.is_none().then(|| super::fatnames::answer(id, &paths, &dest, &mime, &icons)).flatten() {
            let _ = tx.send(OpMsg::Meta { line });
            return;
        }
        let (mut question, shown) = ask(id, &paths, &dest, fat);
        let mergeable = super::merge::mergeable(&paths, &dest);
        question.menu = menu;
        let line = collisions_line(id, question.seen.len() + question.twins.len(), &shown, mergeable, &mime, &icons);
        let _ = tx.send(OpMsg::Asked { turn, question, line });
    });
}

// The latest question asked is the one kept and answered, so an earlier one that finishes later cannot stand in for it; the one transfer that names it spends it.
pub(crate) fn landed(ops: &mut Ops, turn: usize, question: Question) -> bool {
    let latest = turn == ops.asked;
    if latest {
        ops.question = Some(question);
    }
    latest
}

#[path = "collide_table.rs"]
mod table;
pub(crate) use table::{identity_in, Parents, Table};

#[cfg(test)]
#[path = "collide_tests.rs"]
pub(crate) mod tests;
#[cfg(test)]
#[path = "collide_replace_tests.rs"]
mod replace_tests;
#[cfg(test)]
#[path = "replacebatch_tests.rs"]
mod replacebatch_tests;
