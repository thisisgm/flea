//! A transfer's failed sources are selected again once their folder settles, as ui/PaneWire.qml locateRetry.
use super::{Command, Navigator};
use crate::pane::Pane;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Retry {
    pub paths: Vec<String>,
    pub folder: String,
    pub locate: Option<u64>,
    // The listing commit the ask was made against; a re-read before the answer makes its indices stale.
    pub commits: usize,
}

impl Navigator {
    // transferdone's retryPaths; an empty list or a later transfer drops what an earlier one owed.
    pub fn retry_failed(&mut self, pane: &Pane, paths: &[String]) {
        self.retry = (!paths.is_empty()).then(|| Retry {
            paths: paths.to_vec(),
            folder: pane.path().to_owned(),
            locate: None,
            commits: 0,
        });
        self.locate_retry(pane);
    }

    // Asked once the listing is idle in the folder the transfer left; a folder change forgets it.
    pub(super) fn locate_retry(&mut self, pane: &Pane) {
        let Some(retry) = self.retry.as_ref() else {
            return;
        };
        if retry.locate.is_some() || self.busy(pane) || pane.state.search {
            return;
        }
        if retry.folder != pane.path() {
            self.retry = None;
            return;
        }
        let paths = retry.paths.clone();
        let id = self.next_locate();
        if let Some(retry) = self.retry.as_mut() {
            retry.locate = Some(id);
            retry.commits = pane.swap.counters.commits;
        }
        self.emit(Command::LocateMany { paths, id });
    }

    // The matching reply marks the found rows and parks the cursor on the first; their paths name the line.
    pub fn take_retry(
        &mut self,
        pane: &mut Pane,
        directory: &str,
        id: u64,
        matches: &[(String, usize)],
        selected: &mut Vec<usize>,
    ) -> Option<Vec<String>> {
        if self.retry.as_ref()?.locate != Some(id) {
            return None;
        }
        let mut retry = self.retry.take()?;
        let stale = retry.commits != pane.swap.counters.commits || pane.busy();
        if directory != retry.folder || directory != pane.path() {
            return Some(Vec::new());
        }
        if stale {
            retry.locate = None;
            self.retry = Some(retry);
            self.locate_retry(pane);
            return Some(Vec::new());
        }
        if matches.is_empty() {
            return Some(Vec::new());
        }
        selected.clear();
        selected.extend(matches.iter().map(|(_, index)| *index));
        pane.cursor = matches[0].1;
        pane.reveal_cursor();
        Some(matches.iter().map(|(path, _)| path.clone()).collect())
    }
}
