// The external viewer's gallery: which rows an image Enter asks paths for, and whether the reply may still open.

/// The paths request an image Enter sends, captured against the held listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ask {
    pub images: usize,
    pub listing: u64,
    pub scoped: bool,
    pub selected: bool,
    pub rows: Vec<usize>,
}

impl Ask {
    pub fn request(&self) -> String {
        let mut line = format!(r#"{{"c":"paths","images":{},"listing":{},"scoped":{},"selected":{}"#, self.images, self.listing, self.scoped, self.selected);
        if self.scoped {
            let rows: Vec<String> = self.rows.iter().map(usize::to_string).collect();
            line.push_str(&format!(r#","rows":[{}]"#, rows.join(",")));
        }
        line.push('}');
        line
    }
}

/// Several marks select the marked scope, intersected with a filter; one mark or none takes the filter or the whole listing.
pub fn ask(cursor: usize, listing: u64, marks: &[usize], shown: Option<&[usize]>) -> Ask {
    let selected = marks.len() > 1;
    let rows: Vec<usize> = match (selected, shown) {
        (true, Some(shown)) => marks.iter().copied().filter(|m| shown.contains(m)).collect(),
        (true, None) => marks.to_vec(),
        (false, Some(shown)) => shown.to_vec(),
        (false, None) => Vec::new(),
    };
    let scoped = selected || shown.is_some();
    let images = if selected { rows.first().copied().unwrap_or(cursor) } else { cursor };
    Ask { images, listing, scoped, selected, rows }
}

/// The pane facts a reply is checked against before it may open anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
    pub listing: u64,
    pub path: String,
    pub in_flight: bool,
}

/// What a delivered paths reply does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Delivered {
    Open { first: String, paths: Vec<String> },
    Fallback,
    Nothing,
}

/// A departed numbering, a navigation or a listing in flight opens nothing; only an empty unscoped reply falls back to the file alone.
pub fn deliver(ask: &Ask, asked: &Held, now: &Held, paths: Vec<String>) -> Delivered {
    if now.in_flight || now.listing != asked.listing || now.path != asked.path || ask.listing != asked.listing {
        return Delivered::Nothing;
    }
    match paths.first() {
        Some(first) => Delivered::Open { first: first.clone(), paths },
        None if !ask.scoped => Delivered::Fallback,
        None => Delivered::Nothing,
    }
}

/// The paths claim an image Enter takes; another owner's claim refuses with the existing notice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claim {
    Free,
    Images,
    Other,
}

pub fn may_ask(paths_claim: Claim, clip_pending: bool) -> bool {
    paths_claim == Claim::Free && !clip_pending
}

pub fn busy_text(i18n: &crate::i18n::I18n) -> String {
    i18n.tr("dialogs.file.open.resolving")
}
