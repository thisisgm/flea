//! Saved network labels and mount aliases retain URI identity across asynchronous bridge delivery.
use super::mounts::{self, Place};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alias {
    pub path: String,
    pub label: String,
    pub key: String,
}
#[derive(Default, Debug)]
pub struct Aliases {
    pub rows: Vec<Alias>,
}
impl Aliases {
    pub fn remember(&mut self, path: &str, label: &str, uri: &str, saved: &[Place]) {
        let key = mounts::normalize(uri);
        let label = saved
            .iter()
            .rev()
            .find(|p| mounts::normalize(&p.uri) == key)
            .map_or(label, |p| p.label.as_str());
        let alias = Alias {
            path: path.into(),
            label: label.into(),
            key,
        };
        if let Some(old) = self.rows.iter_mut().find(|a| a.path == path) {
            *old = alias;
        } else {
            self.rows.push(alias);
        }
    }
    pub fn relabel(&mut self, saved: &[Place]) -> bool {
        let mut changed = false;
        for row in &mut self.rows {
            if let Some(place) = saved
                .iter()
                .rev()
                .find(|p| mounts::normalize(&p.uri) == row.key)
            {
                if row.label != place.label {
                    row.label.clone_from(&place.label);
                    changed = true;
                }
            }
        }
        changed
    }
    pub fn at(&self, path: &str) -> Option<&Alias> {
        self.rows
            .iter()
            .rev()
            .filter(|a| mounts::inside(path, &a.path))
            .max_by_key(|a| a.path.len())
    }
}

// A removal changes only the matching normalized URI's own lines.
pub fn remove_bookmark(body: &str, uri: &str) -> String {
    let target = mounts::normalize(uri);
    body.split('\n')
        .filter(|line| {
            let trimmed = line.trim();
            let one = trimmed.split_once(' ').map_or(trimmed, |(uri, _)| uri);
            target.is_empty() || one.is_empty() || mounts::normalize(one) != target
        })
        .collect::<Vec<_>>()
        .join("\n")
}
