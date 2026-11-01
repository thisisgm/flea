//! A crumb strip's fitted pieces, where a press lands on them, and the one gesture the strip answers.
use super::crumbs::{crumbs, fit_crumbs, fit_start, Alias, Crumb};

/// Where a press lands: a crumb by index, or the strip's ground where no crumb is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Crumb(usize),
    Ground,
}

/// What a press asks of the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tap {
    Open(String),
    Edit,
    Nothing,
}

// What a press met: a crumb of one fit, or the ground, which outlives every fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Met {
    Crumb(u64, usize),
    Ground,
}

/// One strip's fitted crumbs, rebuilt only when the path, home, alias or room changes, and laid out on their shaped widths.
#[derive(Default)]
pub struct Strip {
    crumbs: Vec<Crumb>,
    edges: Vec<f32>,
    key: (String, String, Option<Alias>, usize),
    revision: u64,
    last_press: Option<Met>,
    pub hovered: Option<usize>,
    /// How many times this strip built its crumbs; a hidden strip builds none.
    pub built: u64,
    plain: bool,
    start: bool,
}

impl Strip {
    pub fn crumbs(&self) -> &[Crumb] {
        &self.crumbs
    }

    /// Bumped by every rebuild, so a cache of shaped crumbs knows its pieces moved.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// budget is the room in characters, as the oracle's fit counts it; place then lays the crumbs out in pixels.
    pub fn fit(&mut self, path: &str, home: &str, alias: Option<&Alias>, budget: usize) -> bool {
        self.fit_crumbs(path, home, alias, budget, false)
    }

    /// The picker's strip: parents elide from the left and the leaf stays whole, as PickerChrome.qml draws it.
    pub fn fit_start(
        &mut self,
        path: &str,
        home: &str,
        alias: Option<&Alias>,
        budget: usize,
    ) -> bool {
        self.fit_crumbs(path, home, alias, budget, true)
    }

    fn fit_crumbs(
        &mut self,
        path: &str,
        home: &str,
        alias: Option<&Alias>,
        budget: usize,
        start: bool,
    ) -> bool {
        if self.plain || self.start != start {
            self.clear();
            self.plain = false;
            self.start = start;
        }
        let (kept_path, kept_home, kept_alias, kept_budget) = &self.key;
        if self.revision > 0
            && kept_path == path
            && kept_home == home
            && kept_alias.as_ref() == alias
            && *kept_budget == budget
        {
            return false;
        }
        self.key = (path.to_owned(), home.to_owned(), alias.cloned(), budget);
        let list = crumbs(path, home, alias);
        self.crumbs = if start {
            fit_start(list, budget)
        } else {
            fit_crumbs(list, budget)
        };
        self.edges.clear();
        self.revision += 1;
        self.built += 1;
        self.hovered = None;
        true
    }

    /// ChromeBar.qml's Path mode is one foreground, inert line with left elision.
    pub fn fit_path(&mut self, path: &str, home: &str, budget: usize) -> bool {
        if self.plain && self.key.0 == path && self.key.1 == home && self.key.3 == budget {
            return false;
        }
        let in_home = !home.is_empty()
            && (path == home
                || path
                    .strip_prefix(home)
                    .is_some_and(|tail| tail.starts_with('/')));
        let mut text = if in_home {
            format!("~{}", &path[home.len()..])
        } else {
            path.to_owned()
        };
        let count = text.chars().count();
        if count > budget {
            text = if budget == 0 {
                String::new()
            } else {
                format!(
                    "\u{2026}{}",
                    text.chars().skip(count - budget + 1).collect::<String>()
                )
            };
        }
        self.crumbs = vec![Crumb {
            text,
            path: path.to_owned(),
            last: true,
            elided: false,
        }];
        self.edges.clear();
        self.key = (path.to_owned(), home.to_owned(), None, budget);
        self.plain = true;
        self.revision += 1;
        self.built += 1;
        self.hovered = None;
        true
    }

    /// A hidden strip holds no crumbs, so a single view rebuilds nothing on every move.
    pub fn clear(&mut self) {
        if self.revision == 0 && self.crumbs.is_empty() {
            return;
        }
        self.crumbs.clear();
        self.edges.clear();
        self.key = Default::default();
        self.revision += 1;
        self.hovered = None;
    }

    /// Lays the crumbs side by side on their shaped widths, in order, reusing the kept buffer.
    pub fn place(&mut self, widths: impl IntoIterator<Item = f32>) {
        self.edges.clear();
        let mut at = 0.;
        self.edges.push(at);
        for width in widths.into_iter().take(self.crumbs.len()) {
            at += width.max(0.);
            self.edges.push(at);
        }
    }

    /// The placed crumbs' drawn width, in pixels.
    pub fn width(&self) -> f32 {
        self.edges.last().copied().unwrap_or(0.)
    }

    /// Crumb i's left edge from the strip's first crumb and its drawn width, in pixels, once placed.
    pub fn extent(&self, index: usize) -> Option<(f32, f32)> {
        let (&left, &right) = (self.edges.get(index)?, self.edges.get(index + 1)?);
        Some((left, right - left))
    }

    /// The placed strip's right edge from its first crumb, where the Git label's gap starts.
    pub fn end(&self) -> f32 {
        self.edges.last().copied().unwrap_or(0.)
    }

    /// x is measured from the strip's first crumb in pixels; an unplaced strip is all ground.
    pub fn hit(&self, x: f32) -> Hit {
        if x < 0. {
            return Hit::Ground;
        }
        // edges is sorted, so the crumb is the last one whose left edge is at or before x.
        let after = self.edges.partition_point(|&edge| edge <= x);
        if after == 0 || after >= self.edges.len() {
            return Hit::Ground;
        }
        Hit::Crumb(after - 1)
    }

    fn met(&self, hit: Hit) -> Met {
        match hit {
            Hit::Crumb(index) => Met::Crumb(self.revision, index),
            Hit::Ground => Met::Ground,
        }
    }

    /// taps is the platform's click count; a second tap only pairs with a first on the same crumb of the same fit.
    pub fn press(&mut self, hit: Hit, taps: usize, modifiers: u8) -> Tap {
        let met = self.met(hit);
        let taps = if taps >= 2 && self.last_press != Some(met) {
            1
        } else {
            taps
        };
        self.last_press = Some(met);
        let crumb = match hit {
            Hit::Crumb(index) => self.crumbs.get(index),
            Hit::Ground => None,
        };
        let row = if crumb.is_some_and(Crumb::parent) {
            "parent"
        } else {
            "inert"
        };
        let Ok(taps) = u8::try_from(taps) else {
            return Tap::Nothing;
        };
        match flea::keys::pointer("chrome", row, "left", modifiers, taps)
            .map(|binding| binding.effect)
        {
            Some("goToCrumb") => crumb.map_or(Tap::Nothing, |crumb| Tap::Open(crumb.path.clone())),
            Some("pathBar") => Tap::Edit,
            _ => Tap::Nothing,
        }
    }

    /// Hovering a parent lights it; the marker and the leaf have nothing to light.
    pub fn hover(&mut self, hit: Option<Hit>) -> bool {
        let next = match hit {
            Some(Hit::Crumb(index))
                if self.crumbs.get(index).is_some_and(|crumb| !crumb.elided) =>
            {
                Some(index)
            }
            _ => None,
        };
        let changed = next != self.hovered;
        self.hovered = next;
        changed
    }
}
