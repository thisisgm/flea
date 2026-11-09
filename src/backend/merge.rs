// Folder merge: everything missing copied or moved in, files that exist in both asked about once.
use crate::backend::collide::{MenuCapture, Place, Policy, Question, SHOWN};
use crate::backend::fatnames::{self, Choice};
use crate::backend::undo::ItemIdentity;
use crate::json::escape;
use flea::bytename::ExactText as _;
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

// One file-level collision the walk found: the source entry and the destination name it would take.
pub struct Found {
    pub src: PathBuf,
    pub dst: PathBuf,
    pub rel: String,
    pub dir: bool,
    pub mode: u32,
}

// A link is copied as the link, so only a real directory on both sides merges without a question.
pub(crate) fn real_dir(path: &Path) -> bool {
    path.symlink_metadata().is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
}

// The transfer loop's own test for a folder pair, beside the question that listed it.
pub(crate) fn is_mergeable(src: &Path, dst: &Path) -> bool {
    real_dir(src) && real_dir(dst)
}

// A menu question's sources as paths, so a merge walk reuses the capture instead of a live selection the dialog already expired.
pub(crate) fn menu_paths(question: &Question) -> Option<Vec<String>> {
    question.menu.as_ref().map(|menu| {
        menu.items
            .iter()
            .map(|item| item.path.exact_text().to_string())
            .collect()
    })
}

// True when a folder lands on a folder, which is the only shape the Merge card offers.
pub(crate) fn mergeable(paths: &[String], dest: &str) -> bool {
    let Ok(dest_path) = crate::backend::opsreq::usable_dest(dest) else {
        return false;
    };
    let dest_real = dest_path
        .canonicalize()
        .unwrap_or_else(|_| dest_path.clone());
    let (mut parents, table) = (
        super::collide::Parents::default(),
        super::collide::Table::read(&dest_path),
    );
    for raw in paths {
        let src = flea::bytename::view(raw);
        let src = &*src;
        if !src.is_absolute() {
            continue;
        }
        let Some(name) = src.file_name().and_then(|n| n.to_str()) else { continue };
        let Ok(meta) = src.symlink_metadata() else { continue };
        let dst = dest_path.join(name);
        if super::collide::already_there_in(src, name, &dst, &dest_real, &mut parents) {
            continue;
        }
        let Ok(there) = super::collide::identity_in(&dst, &table) else { continue };
        if meta.is_dir() && !meta.file_type().is_symlink() && there.is_dir() {
            return true;
        }
    }
    false
}

// Entries a copied tree holds, itself included, so the status counts what landed: from/trip with a.txt and sub/b.txt is 4.
pub(crate) fn count_tree(path: &Path) -> usize {
    // A symlink lands as the link itself: count one and never walk its target.
    if path.symlink_metadata().is_ok_and(|meta| meta.file_type().is_symlink()) {
        return 1;
    }
    let mut count = 1;
    let Ok(entries) = std::fs::read_dir(path) else { return count };
    for entry in entries.flatten() {
        // d_type answers dir-ness for free; a symlink is never a dir here, so none is descended into.
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            count += count_tree(&entry.path());
        } else {
            count += 1;
        }
    }
    count
}

// One walk of the sources against the destination, before anything is written; under Rename the names below a merging folder are the renamed ones.
pub fn walk(paths: &[String], dest: &Path, fat: Option<Choice>) -> Vec<Found> {
    let mut found = Vec::new();
    let Ok(dest_path) = crate::backend::opsreq::usable_dest(&dest.exact_text()) else {
        return found;
    };
    let dest_real = dest_path
        .canonicalize()
        .unwrap_or_else(|_| dest_path.clone());
    let rename = fatnames::renames(fat, fatnames::answered_on_fat(fat, &dest_path));
    // One merging folder shows its own relative names; several share the destination's.
    let single = paths
        .iter()
        .filter(|raw| is_merging_folder(&flea::bytename::view(raw), &dest_path, &dest_real))
        .count()
        == 1;
    for raw in paths {
        let src = flea::bytename::view(raw);
        let src = &*src;
        if !src.is_absolute() {
            continue;
        }
        let Some(name) = src.file_name().and_then(|n| n.to_str()) else { continue };
        let Ok(meta) = src.symlink_metadata() else { continue };
        let dst_top = dest_path.join(name);
        if crate::backend::collide::already_there(src, name, &dst_top, &dest_real) {
            continue;
        }
        if meta.is_dir() && !meta.file_type().is_symlink() && real_dir(&dst_top) {
            walk_into(src, &dst_top, &dest_path, single, rename, &mut found);
        } else if dst_top.symlink_metadata().is_ok() {
            found.push(Found { src: src.to_path_buf(), dst: dst_top.clone(),
                rel: rel_of(&dst_top, &dst_top, &dest_path, false), dir: meta.is_dir(), mode: meta.mode() });
        }
    }
    found
}

// Whether this source folds into the destination twin it names, the walk's own gate below.
fn is_merging_folder(src: &Path, dest_path: &Path, dest_real: &Path) -> bool {
    if !src.is_absolute() {
        return false;
    }
    let Some(name) = src.file_name().and_then(|n| n.to_str()) else { return false };
    let Ok(meta) = src.symlink_metadata() else { return false };
    let dst_top = dest_path.join(name);
    if crate::backend::collide::already_there(src, name, &dst_top, dest_real) {
        return false;
    }
    meta.is_dir() && !meta.file_type().is_symlink() && real_dir(&dst_top)
}

// Every name under a merging folder the destination already holds, except folders merging further down; rename says each lands as Rename names it.
fn walk_into(src_top: &Path, dst_top: &Path, dest: &Path, single: bool, rename: bool, found: &mut Vec<Found>) {
    let Ok(entries) = std::fs::read_dir(src_top) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let (src_inner, dst_inner) = (src_top.join(&name), dst_top.join(fatnames::inside_leaf(&name, rename)));
        // d_type answers dir-ness for free; each colliding entry pays one lstat for the mode the card draws.
        let dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if dst_inner.symlink_metadata().is_err() {
            continue;
        }
        if dir && real_dir(&dst_inner) {
            walk_into(&src_inner, &dst_inner, dest, single, rename, found);
        } else {
            let Ok(meta) = src_inner.symlink_metadata() else { continue };
            found.push(Found { src: src_inner, dst: dst_inner.clone(),
                rel: rel_of(&dst_inner, dst_top, dest, single), dir: meta.is_dir(), mode: meta.mode() });
        }
    }
}

// What the second card draws for one collision: its own folder's relative name, or the destination's when several merge.
fn rel_of(dst_inner: &Path, dst_top: &Path, dest: &Path, single: bool) -> String {
    let base = if single { dst_top } else { dest };
    dst_inner.strip_prefix(base).unwrap_or(dst_inner).to_string_lossy().to_string()
}

// The walk's collisions joined onto the outer question, so one transfer answers both cards.
pub fn combine(mut question: Question, inner: Vec<Found>) -> Question {
    for found in &inner {
        if let Ok(there) = ItemIdentity::inspect(&found.dst) {
            question.seen.insert(found.src.clone(), there);
        }
    }
    question
}

// How many inner entries one merged folder landed, kept both, skipped or failed.
#[derive(Default)]
pub struct Counts {
    pub merged: usize,
    pub kept_both: usize,
    pub skipped: usize,
    pub failed: Vec<String>,
}

// Sample output: {"t":"mergecollisions","id":8,"total":2,"names":[{"n":"IMG_4120.HEIC","d":false,"i":"image-x-generic"}]}
pub fn mergecollisions_line(id: usize, total: usize, names: &str) -> String {
    format!(r#"{{"t":"mergecollisions","id":{},"total":{},"names":[{}]}}"#, id, total, names)
}

fn names_line(found: &[Found], mime: &crate::backend::mime::Db, icons: &crate::backend::icons::Names) -> String {
    found.iter().take(SHOWN).map(|s| format!(r#"{{"n":"{}","d":{},"i":"{}"}}"#,
        escape(&s.rel), s.dir, escape(icons.icon_for(mime.lookup(&s.rel), s.dir, s.mode)))).collect::<Vec<_>>().join(",")
}

// Read-only beside the loop, like collisions; the latest question asked is the one kept.
pub(crate) fn ask_beside(
    ops: &mut crate::backend::opsdispatch::Ops,
    id: usize,
    menu_id: usize,
    named: Vec<String>,
    dest: &str,
    mime: &std::sync::Arc<crate::backend::mime::Db>,
    icons: &std::sync::Arc<crate::backend::icons::Names>,
) {
    // The first card's capture is the source list: its dialog already expired the live one.
    let outer = ops
        .question
        .as_ref()
        .filter(|q| q.dest == *flea::bytename::view(dest));
    let paths = match (outer.and_then(menu_paths), menu_id) {
        (Some(captured), _) => captured,
        (None, 0) => named,
        (None, _) => Vec::new(),
    };
    let base = walk_base(outer);
    let fat = outer.and_then(|q| q.fat);
    ops.asked += 1;
    let (turn, tx, dest, mime, icons) = (ops.asked, ops.tx.clone(), dest.to_string(), std::sync::Arc::clone(mime), std::sync::Arc::clone(icons));
    std::thread::spawn(move || {
        let inner = walk(&paths, &flea::bytename::view(&dest), fat);
        let total = inner.len();
        let names = names_line(&inner, &mime, &icons);
        let mut question = Question {
            id,
            dest: flea::bytename::exact(&dest),
            seen: HashMap::new(),
            menu: None,
            fat,
        };
        if let Some((dest, seen, menu)) = base {
            question.dest = dest;
            question.seen = seen;
            question.menu = menu;
            question = combine(question, inner);
        }
        let line = mergecollisions_line(id, total, &names);
        let _ = tx.send(crate::backend::opsreq::OpMsg::Asked { turn, question, line });
    });
}

// The walk's inputs packaged for its thread, so the question arm keeps one call.
pub(crate) fn walk_base(outer: Option<&Question>) -> Option<(PathBuf, HashMap<PathBuf, ItemIdentity>, Option<MenuCapture>)> {
    outer.map(|q| (q.dest.clone(), q.seen.clone(), q.menu.clone()))
}

// A merge choice that reaches placement lands without replacing: the fold answers folder pairs before this runs.
pub(crate) fn land_unmerged(dst: PathBuf) -> Place {
    Place::Land { to: dst, replace: false }
}

// A keep-both landing names a file the request did not name.
pub(crate) fn kept_name(dst: &Path, name: &str) -> bool {
    dst.file_name().map(|n| n.exact_text()).as_deref() != Some(name)
}

// A merge entry that is not a folder pair takes the second card's answer through the same land below.
pub(crate) fn merge_place(policy: &Policy, src: &Path, dst: PathBuf, moving: bool) -> Place {
    policy.inner().place(src, dst, false, moving)
}

// One merging folder names itself, several name the destination they folded into.
pub(crate) fn merge_into_name(merged: &[String], dest: &Path) -> String {
    match merged {
        [one] => one.clone(),
        _ => dest.file_name().map(|n| n.to_string_lossy().into_owned()).filter(|n| !n.is_empty())
            .unwrap_or_else(|| dest.to_string_lossy().into_owned()),
    }
}

// Sample output: {"t":"transferdone","id":12,"ok":1,"failed":0,"skipped":0,"cancelled":false,"retryPaths":[],"durable":false,"note":"","merged":212,"keptBoth":3,"into":"trip"}
pub(crate) fn transferdone_merge_line(msg: &super::opsreq::OpMsg) -> String {
    let super::opsreq::OpMsg::MergeDone {
        id,
        ok,
        failed,
        skipped,
        cancelled,
        retry,
        durable,
        note,
        merged,
        kept_both,
        into,
        refused,
        ..
    } = msg
    else {
        return String::new();
    };
    let paths: Vec<_> = retry
        .iter()
        .map(|(path, _)| format!("\"{}\"", escape(&path.exact_text())))
        .collect();
    format!(
        r#"{{"t":"transferdone","id":{},"ok":{},"failed":{},"skipped":{},"cancelled":{},"retryPaths":[{}],"durable":{},"note":"{}","merged":{},"keptBoth":{},"into":"{}"{}}}"#,
        id,
        ok,
        failed,
        skipped,
        cancelled,
        paths.join(","),
        durable,
        escape(note),
        merged,
        kept_both,
        escape(into),
        super::opsreq::refused_field(refused)
    )
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;
