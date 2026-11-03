//! The clipboard marks: a copied row carries copy and a cut row scissors, following the clipboard; a port of ui/js/ClipMarks.js with one keyed lookup per selection, named by its session revision, and an emptied clipboard drops its lookup once.
use std::collections::HashSet;

// Sample input: a cut over one picture at revision 7.
#[derive(Default)]
pub struct Marks {
    revision: Option<u64>,
    moving: bool,
    set: HashSet<String>,
}

impl Marks {
    pub fn new() -> Self {
        Self::default()
    }

    // One keyed lookup per selection, rebuilt only when its revision changes; the paths are borrowed, never cloned to test equality.
    pub fn set_for(&mut self, revision: u64, paths: &[String], moving: bool) {
        if self.revision == Some(revision) {
            return;
        }
        self.revision = Some(revision);
        self.moving = moving;
        self.set.clear();
        self.set.extend(paths.iter().cloned());
    }

    pub fn cached(&self) -> bool {
        self.revision.is_some()
    }

    pub fn revision(&self) -> Option<u64> {
        self.revision
    }

    // Sample input: "/home/gm/Pictures/phone/IMG_4121.jpg" in a copy reads "copy".
    pub fn mark_for(&mut self, path: &str) -> &'static str {
        if self.set.contains(path) {
            if self.moving {
                "scissors"
            } else {
                "copy"
            }
        } else {
            ""
        }
    }

    // An empty clipboard costs nothing: its paths are never joined or searched.
    pub fn mark_for_row(
        &mut self,
        base: &str,
        name: &str,
        revision: u64,
        paths: &[String],
        moving: bool,
    ) -> &'static str {
        if paths.is_empty() {
            self.release();
            return "";
        }
        if name.is_empty() {
            return "";
        }
        self.set_for(revision, paths, moving);
        let path = row_path(base, name);
        self.mark_for(&path)
    }

    // An emptied clipboard drops its lookup once, so a cut of a large directory frees it and later rows allocate nothing.
    pub fn release(&mut self) {
        self.revision = None;
        self.set.clear();
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }
}

// The pane's own join under the listing's base, so a search result resolves against the scope its search walked; base "/home/gm/Pictures/phone" with "IMG_4121.jpg" reads the full path.
pub fn row_path(base: &str, name: &str) -> String {
    let mut path = String::new();
    row_path_into(&mut path, base, name);
    path
}

// The same join into a caller buffer, so a paint loop allocates nothing once warm.
pub fn row_path_into(path: &mut String, base: &str, name: &str) {
    path.clear();
    if name.is_empty() {
        return;
    }
    if base != "/" {
        path.push_str(base);
    }
    path.push('/');
    path.push_str(name);
}

// A sent cut empties a cut clipboard, so the marks go with it; anything else keeps it (a cut spent by a cut paste empties).
pub fn spent(paths: &[String], moving: bool, spends_cut: bool) -> Option<(Vec<String>, bool)> {
    if spends_cut && moving {
        None
    } else {
        Some((paths.to_vec(), moving))
    }
}
