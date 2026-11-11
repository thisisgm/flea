// The collisions question a paste or a drop asks first, read-only, and the line the card draws it from.
use super::{already_there_in, identity_in, Parents, Question, Table, SHOWN};
use crate::backend::icons::Names;
use crate::backend::mime::Db;
use crate::backend::opsreq::usable_dest;
use crate::json::escape;
use flea::bytename::ExactText as _;
use std::collections::{HashMap, HashSet};
use std::os::unix::fs::MetadataExt;

// One colliding source as the card draws it: its name, the exact name it lands on, and the kind its mark is chosen from.
pub(super) struct Shown {
    pub(super) name: String,
    pub(super) exact: String,
    pub(super) dir: bool,
    pub(super) mode: u32,
}

// The question itself, read-only: which sources would land on a name dest already holds, under their FAT-renamed names.
pub(super) fn ask(id: usize, paths: &[String], dest: &str, fat: Option<crate::backend::fatnames::Choice>) -> (Question, Vec<Shown>) {
    let mut question = Question { id, dest: flea::bytename::exact(dest), seen: HashMap::new(), twins: HashSet::new(), menu: None, fat };
    let mut shown = Vec::new();
    // An unusable destination asks nothing: the transfer that follows answers its own error.
    let Ok(dest_path) = usable_dest(dest) else { return (question, shown) };
    let on_fat = crate::backend::fatnames::answered_on_fat(fat, &dest_path);
    let dest_real = dest_path.canonicalize().unwrap_or_else(|_| dest_path.clone());
    let (mut parents, table, mut claimed) = (Parents::default(), Table::read(&dest_path), crate::backend::fatnames::casefold::Claimed::new(&dest_path));
    for raw in paths {
        let src = flea::bytename::view(raw); let src = &*src;
        if !src.is_absolute() {
            continue;
        }
        let Some(name) = src.file_name().map(|n| n.exact_text().into_owned()) else {
            continue;
        };
        let Ok(meta) = src.symlink_metadata() else {
            continue;
        };
        let Some((dst, leaf)) = crate::backend::fatnames::ask_leaf(fat, on_fat, &dest_path, &name) else {
            continue;
        };
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
            shown.push(Shown { name: flea::bytename::display(&leaf).into_owned(), exact: leaf.clone(), dir: meta.is_dir(), mode: meta.mode() });
        }
    }
    (question, shown)
}

// Sample output: {"t":"collisions","id":7,"total":1,"names":[{"n":"a.png","d":false,"i":"image-x-generic"}],"mergeable":false}
// A name that is not UTF-8 also carries its exact text in "x", so a client finds and locates that file and not its lossy twin.
pub(super) fn collisions_line(id: usize, total: usize, shown: &[Shown], mergeable: bool, mime: &Db, icons: &Names) -> String {
    let names: Vec<String> = shown.iter().map(|s| format!(r#"{{"n":"{}","d":{},"i":"{}"{}}}"#,
        escape(&s.name), s.dir, escape(icons.icon_for(mime.lookup(&s.name), s.dir, s.mode)),
        if s.exact == s.name { String::new() } else { format!(r#","x":"{}""#, escape(&s.exact)) })).collect();
    format!(r#"{{"t":"collisions","id":{},"total":{},"names":[{}],"mergeable":{}}}"#, id, total, names.join(","), mergeable)
}
