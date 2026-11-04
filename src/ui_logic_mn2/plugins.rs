//! Plugin-contributed menu rows, behind the narrowest join seam: at BASE the plugin producer is not merged, so this module defines the plain-data shape the join fills in (one row per contributed action, already limited to enabled and healthy plugins; a disabled or failed plugin contributes nothing, never an empty or broken row).

/// One `menu-action` contribution the join read off an enabled, started plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contribution {
    pub plugin: String,
    pub action: String,
    pub label: String,
    pub glyph: String,
    // File menu, background menu, or both; the only menus contributions land in.
    pub menus: Vec<Menu>,
    // The block's `when`: the row shows only while every selected item matches it.
    pub when: When,
}

/// The `when` block of one `menu-action` item (`design-hig/specs/plugins-guide/reference.md`, menu-action).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct When {
    // MIME globs every selected item must match; empty means no kind limit.
    pub mime: Vec<String>,
    // The smallest and largest selection the row shows for; None is no cap.
    pub min_items: usize,
    pub max_items: Option<usize>,
    // Whether a folder may be among the selection; false keeps files only.
    pub folders: bool,
}

/// One selected path as the host describes it: its MIME kind and its folder flag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub mime: String,
    pub folder: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    File,
    Background,
}

impl Menu {
    pub fn word(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Background => "background",
        }
    }
}

// One glob over one kind, `*` for a run and `?` for one byte, the same linear shape `src/backend/actionmanifest.rs` matches manifest MIME globs with.
pub fn mime_match(pattern: &str, value: &str) -> bool {
    let (p, v) = (pattern.as_bytes(), value.as_bytes());
    let (mut pi, mut vi, mut star, mut retry) = (0, 0, None, 0);
    while vi < v.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == v[vi]) {
            pi += 1;
            vi += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            pi += 1;
            retry = vi;
        } else if let Some(at) = star {
            retry += 1;
            vi = retry;
            pi = at + 1;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

// The join maps st2 here: `contributing()` keeps enabled plugins with no failure, each `menu-action` kind's `menuActions` block item becomes one contribution with its block glyph and its block `when` (defaulting to the file menu when the block names none), an unreadable block contributes nothing, and a row shows only on the kinds it declares, so an image-only row never lands on notes.txt.
pub fn eligible<'a>(
    contributions: &'a [Contribution],
    background: bool,
    items: &[Item],
) -> Vec<&'a Contribution> {
    let want = if background {
        Menu::Background
    } else {
        Menu::File
    };
    contributions
        .iter()
        .filter(|c| {
            c.menus.contains(&want)
                && c.when.min_items <= items.len()
                && match c.when.max_items {
                    Some(max) => items.len() <= max,
                    None => true,
                }
                && items.iter().all(|item| {
                    (c.when.folders || !item.folder)
                        && (c.when.mime.is_empty()
                            || c.when.mime.iter().any(|glob| mime_match(glob, &item.mime)))
                })
        })
        .collect()
}

// The dispatch a chosen row fires: the plugin and its action in one leaf.
pub fn leaf(contribution: &Contribution) -> String {
    format!("{}:{}", contribution.plugin, contribution.action)
}
