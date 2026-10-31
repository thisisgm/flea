//! One path as the pieces a click can land on, and what fits when the strip is narrower than the path.
use std::collections::HashMap;

/// text is what is drawn, its separator included, so the pieces concatenate to the line they replace.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Crumb {
    pub text: String,
    pub path: String,
    pub last: bool,
    pub elided: bool,
}

impl Crumb {
    fn new(text: String, path: String) -> Self {
        Self {
            text,
            path,
            last: false,
            elided: false,
        }
    }

    /// A parent opens on one tap; the leaf and the collapsed marker name nowhere new.
    pub fn parent(&self) -> bool {
        !self.last && !self.elided
    }

    pub fn chars(&self) -> usize {
        self.text.chars().count()
    }
}

/// A remembered display root: a saved network place's label drawn in place of its mount path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Alias {
    pub path: String,
    pub label: String,
    pub key: String,
}

// The home test is the whole component, so a sibling like /home/gmx is not inside /home/gm.
fn inside(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

pub fn clean_root(path: &str) -> &str {
    let mut text = path;
    while text.len() > 1 && text.ends_with('/') {
        text = &text[..text.len() - 1];
    }
    text
}

// Only a crumb with another after it carries a separator, so the last gives its own back.
fn finish(mut out: Vec<Crumb>) -> Vec<Crumb> {
    let many = out.len() > 1;
    if let Some(end) = out.last_mut() {
        if many {
            end.text.pop();
        }
        end.last = true;
    }
    out
}

pub fn crumbs(path: &str, home: &str, alias: Option<&Alias>) -> Vec<Crumb> {
    if let Some(alias) = alias {
        let root = clean_root(&alias.path);
        if !root.is_empty() && !alias.label.is_empty() && inside(path, root) {
            // The label is one crumb even when it holds a "/"; only the real suffix adds targets.
            let parts: Vec<&str> = path[root.len()..]
                .split('/')
                .filter(|part| !part.is_empty())
                .collect();
            let sep = if parts.is_empty() { "" } else { "/" };
            let mut out = vec![Crumb::new(format!("{}{sep}", alias.label), root.to_owned())];
            let mut walked = root.to_owned();
            for part in parts {
                walked = format!("{walked}/{part}");
                out.push(Crumb::new(format!("{part}/"), walked.clone()));
            }
            return finish(out);
        }
    }
    let in_home = !home.is_empty() && inside(path, home);
    let display = if in_home {
        format!("~{}", &path[home.len()..])
    } else {
        path.to_owned()
    };
    let parts: Vec<&str> = display.split('/').collect();
    let mut walked = if in_home {
        home.to_owned()
    } else {
        String::new()
    };
    // The leading "~" and the leading "/" are crumbs of their own, naming home and the root.
    let first = if parts.len() > 1 {
        format!("{}/", parts[0])
    } else {
        parts[0].to_owned()
    };
    let target = if walked.is_empty() {
        "/".to_owned()
    } else {
        walked.clone()
    };
    let mut out = vec![Crumb::new(first, target)];
    for part in parts.iter().skip(1).filter(|part| !part.is_empty()) {
        walked = format!("{walked}/{part}");
        out.push(Crumb::new(format!("{part}/"), walked.clone()));
    }
    finish(out)
}

// Issue 237: the deepest remembered display root wins, so nested spaces on one host stay distinct.
pub fn alias_at(path: &str, aliases: &[Alias]) -> Option<Alias> {
    let mut best: Option<&Alias> = None;
    for alias in aliases {
        let root = clean_root(&alias.path);
        if !root.is_empty()
            && inside(path, root)
            && best.is_none_or(|kept| root.len() > clean_root(&kept.path).len())
        {
            best = Some(alias);
        }
    }
    best.filter(|alias| !alias.label.is_empty())
        .map(|alias| Alias {
            path: clean_root(&alias.path).to_owned(),
            label: alias.label.clone(),
            key: alias.key.clone(),
        })
}

pub fn remember_alias(aliases: &[Alias], path: &str, label: &str, key: Option<&str>) -> Vec<Alias> {
    let root = clean_root(path);
    if root.is_empty() || label.is_empty() {
        return aliases.to_vec();
    }
    let id = key.filter(|key| !key.is_empty()).unwrap_or(root);
    let mut out: Vec<Alias> = aliases
        .iter()
        .filter(|alias| alias.key != id && clean_root(&alias.path) != root)
        .cloned()
        .collect();
    out.push(Alias {
        path: root.to_owned(),
        label: label.to_owned(),
        key: id.to_owned(),
    });
    out
}

/// A rail entry as the alias rename reads it: a persisted source may rename, a live mount may not.
pub struct Place<'a> {
    pub uri: &'a str,
    pub path: &'a str,
    pub label: &'a str,
    pub saved: bool,
}

pub fn labels_by_key(
    entries: &[Place<'_>],
    key_for: impl Fn(&str) -> String,
) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for entry in entries {
        let address = if entry.uri.is_empty() {
            entry.path
        } else {
            entry.uri
        };
        if entry.saved && !address.is_empty() && !entry.label.is_empty() {
            out.insert(key_for(address), entry.label.to_owned());
        }
    }
    out
}

// A saved-place rename changes the visible root and never a real path or a navigation target.
pub fn relabel_aliases(aliases: &[Alias], labels: &HashMap<String, String>) -> Vec<Alias> {
    aliases
        .iter()
        .map(
            |alias| match labels.get(&alias.key).filter(|label| !label.is_empty()) {
                Some(label) => Alias {
                    label: label.clone(),
                    ..alias.clone()
                },
                None => alias.clone(),
            },
        )
        .collect()
}

// Below this a leaf gives up more to the ellipsis than the ellipsis saves, so it is drawn whole.
pub const LEAF_FLOOR: usize = 6;
const ELLIPSIS: char = '\u{2026}';

pub fn crumb_chars(list: &[Crumb]) -> usize {
    list.iter().map(Crumb::chars).sum()
}

// What the crumbs before it leave the leaf, in characters.
fn leaf_room(list: &[Crumb], budget: usize) -> isize {
    let leaf = list.last().map_or(0, Crumb::chars);
    budget as isize - (crumb_chars(list) - leaf) as isize
}

// The leaf is the one crumb that cannot be dropped, so when even it does not fit it takes the ellipsis in its middle.
fn elide_leaf(mut list: Vec<Crumb>, budget: usize) -> Vec<Crumb> {
    let room = leaf_room(&list, budget);
    let Some(end) = list.last_mut() else {
        return list;
    };
    let length = end.chars();
    if room >= length as isize || length <= LEAF_FLOOR {
        return list;
    }
    let keep = LEAF_FLOOR.max(room.max(0) as usize);
    let head = keep / 2;
    let tail = keep - 1 - head;
    let chars: Vec<char> = end.text.chars().collect();
    let mut cut: String = chars[..head].iter().collect();
    cut.push(ELLIPSIS);
    cut.extend(&chars[length - tail..]);
    end.text = cut;
    list
}

// Chrome rule 2: a path too long reads as its root, one collapsed marker and the segments nearest you, whole crumbs only.
pub fn fit_crumbs(list: Vec<Crumb>, budget: usize) -> Vec<Crumb> {
    if list.len() < 4 || crumb_chars(&list) <= budget {
        return elide_leaf(list, budget);
    }
    let marker = Crumb {
        text: format!("{ELLIPSIS}/"),
        path: String::new(),
        last: false,
        elided: true,
    };
    let count = list.len();
    let mut shown = vec![
        list[0].clone(),
        marker,
        list[count - 2].clone(),
        list[count - 1].clone(),
    ];
    // The room the marker leaves goes back to the crumbs nearest the root, in walking order.
    for (at, crumb) in list.iter().enumerate().take(count - 2).skip(1) {
        let width = crumb_chars(&shown) + crumb.chars();
        if width > budget {
            break;
        }
        shown.insert(at, crumb.clone());
    }
    // A parent that leaves the leaf less than it needs and less than its floor goes the way the middle went.
    if shown.len() == 4 {
        let left = leaf_room(&shown, budget);
        if left < shown[3].chars() as isize && left < LEAF_FLOOR as isize {
            shown.remove(2);
        }
    }
    elide_leaf(shown, budget)
}

// PickerChrome.qml's strip: a path too long gives up its parents from the left, a character at a time, and keeps the leaf whole.
pub fn fit_start(list: Vec<Crumb>, budget: usize) -> Vec<Crumb> {
    if list.len() < 2 || crumb_chars(&list) <= budget {
        return elide_leaf(list, budget);
    }
    let count = list.len();
    let leaf = list[count - 1].clone();
    // A leaf that fills the strip leaves its parents no width, so it is drawn alone.
    if leaf.chars() >= budget {
        return elide_leaf(vec![leaf], budget);
    }
    // The ellipsis takes one character of the room the leaf leaves.
    let mut room = budget.saturating_sub(leaf.chars()).saturating_sub(1);
    let mut kept = Vec::new();
    for crumb in list[..count - 1].iter().rev() {
        if crumb.chars() <= room {
            room -= crumb.chars();
            kept.push(crumb.clone());
            continue;
        }
        // The first parent that does not fit whole keeps its tail behind the ellipsis and still opens where it names.
        let tail: String = crumb.text.chars().skip(crumb.chars() - room).collect();
        kept.push(Crumb {
            text: format!("{ELLIPSIS}{tail}"),
            ..crumb.clone()
        });
        break;
    }
    kept.reverse();
    kept.push(leaf);
    elide_leaf(kept, budget)
}
