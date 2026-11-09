//! A folder that is no longer there: the hidden tabs inside it open the nearest folder still there.
use super::state::{join, Tab, TabState};

// Sample: "/w/p/open-me" gone to "/w" answers "/w/p", the folder under the parent that took the rest with it.
pub fn vanished_root(path: &str, parent: &str) -> Option<String> {
    let tail = path.strip_prefix(parent)?;
    let tail = if parent == "/" {
        tail
    } else {
        tail.strip_prefix('/')?
    };
    let first = tail.split('/').next().filter(|name| !name.is_empty())?;
    Some(join(parent, first))
}

impl TabState {
    // Hidden tabs inside a folder that is no longer there open the nearest folder still there; their history stays, as the pane's does.
    pub fn land_gone(&mut self, root: &str, parent: &str) -> bool {
        let Some(tabs) = self.tabs.as_mut() else {
            return false;
        };
        let mut moved = false;
        for (i, tab) in tabs.items.iter_mut().enumerate() {
            let inside = tab.path == root
                || tab
                    .path
                    .strip_prefix(root)
                    .is_some_and(|tail| tail.starts_with('/'));
            if i == tabs.index || !inside {
                continue;
            }
            let (back, forward) = (
                std::mem::take(&mut tab.back),
                std::mem::take(&mut tab.forward),
            );
            *tab = Tab {
                identity: tab.identity,
                back,
                forward,
                view: tab.view,
                hidden: tab.hidden,
                order: tab.order,
                ..Tab::at(parent)
            };
            moved = true;
        }
        moved
    }
}
