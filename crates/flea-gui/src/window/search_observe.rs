//! Query reads, prepared-row paint observations and the scenes that showed them; keyDeliveryState is in key_delivery.rs.
use crate::{
    g2::file_search::{
        presented::{Scene, Shown},
        Search,
    },
    pane::presentation::Content,
    state::Shell,
    window::pane::PaneHost,
};
pub(crate) struct QueryReader<'a> {
    pub search: &'a super::Runtime,
    pub focused: bool,
    pub filter_typing: bool,
    pub no_match: Option<String>,
}

impl crate::ipc::Adapter for QueryReader<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<flea::ipc::wire::Reply> {
        use flea::{ipc::wire::Reply, ui_logic_file_search::Mode};
        let owner: &Search = &self.search.owner;
        let value = match function {
            "focusView" if self.focused && self.filter_typing => "filter".into(),
            "focusView" if self.focused && owner.typing() => "search".into(),
            "searchMode" => match owner.session.mode {
                Mode::Off => "",
                Mode::Typing => "typing",
                Mode::Results => "results",
            }
            .into(),
            "searchQuery" => owner.session.query.clone(),
            "noMatchHeadline" => self.no_match.clone().unwrap_or_default(),
            // The strip's laid-out box while search holds the header slot, "x y width height".
            "searchRect" => match self.search.strip.borrow().bounds {
                Some(b) if owner.active() => [b.origin.x, b.origin.y, b.size.width, b.size.height]
                    .map(|value| f32::from(value).round().to_string())
                    .join(" "),
                _ => String::new(),
            },
            _ => return None,
        };
        Some(Reply::value(value))
    }
}

/// Metadata captured by the List's own paint, released after paint without reentering its owner.
pub(crate) struct Draw {
    pub side: usize,
    pub generation: usize,
    pub listing: usize,
    pub clock: crate::pane::clock::Clock,
    pub shell: gpui::WeakEntity<Shell>,
}

impl Draw {
    pub fn painted(&self, rows: &[crate::list::Shown], cx: &mut gpui::App) {
        if self.generation == 0 || !rows.iter().any(|row| row.prepared.is_some()) {
            return;
        }
        let (side, generation, listing) = (self.side, self.generation, self.listing);
        let at = self.clock.elapsed().as_millis() as u64;
        let shell = self.shell.clone();
        cx.defer(move |cx| {
            let _ = shell.update(cx, |shell, cx| {
                let Some(host) = shell.panes[side].as_mut() else {
                    return;
                };
                if host.pane.held.listing != listing || host.pane.swap.session.drops(generation) {
                    return;
                }
                let before = host.search.owner.session.stamps.first_rows_at;
                host.search.owner.rows_drawn(generation, at);
                if before != host.search.owner.session.stamps.first_rows_at {
                    cx.notify();
                }
            });
        });
    }
}

impl PaneHost {
    /// Some while the pane holds the newest search generation's own rows, never a held older answer.
    pub(crate) fn search_scene(&self) -> Option<(u64, bool)> {
        let stamps = self.search.owner.session.stamps;
        (self.search.owner.active()
            && stamps.gen != 0
            && self.search.rows_generation as u64 == stamps.gen
            && self.pane.swap.session.generation as u64 == stamps.gen)
            .then_some((stamps.gen, stamps.settled_at != 0))
    }
}

impl Shell {
    /// Every pane's search hears the window's feedback coverage around each expiry, terminal and close.
    pub(crate) fn sync_search_coverage(&mut self) {
        let (blind, oldest) = self.presentation.search_coverage();
        let observed = self.presentation.observer.is_some();
        for host in self.panes.iter_mut().flatten() {
            let current = host.search.owner.session.stamps.gen;
            host.search
                .owner
                .presented
                .coverage(current, blind as u64, oldest, observed);
        }
    }

    /// A presented submission whose scene painted the search's rows credits that generation, top painted row first.
    pub(crate) fn search_presented(
        &mut self,
        side: usize,
        content: &Content,
        frame: u64,
        time: gpui::PresentationTime,
    ) {
        let (Some((gen, settled)), Some(&row)) =
            (content.search_scene, content.painted_rows.iter().min())
        else {
            return;
        };
        let (Some(found), Some(host)) = (content.held.row(row), self.panes[side].as_mut()) else {
            return;
        };
        let scene = Scene {
            gen,
            settled,
            row,
            name: found.name.clone(),
        };
        let shown = Shown {
            frame,
            at_ns: time.timestamp.map(|at| at.as_nanos() as u64),
            clock: time.clock_id,
        };
        let current = host.search.owner.session.stamps.gen;
        host.search.owner.presented.record(current, &scene, shown);
    }
}
