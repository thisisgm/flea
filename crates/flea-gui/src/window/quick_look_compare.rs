//! Compare in the Quick Look card: two regular files, one diff asked of the pane's backend (Ops.js openCompare).
use super::{token, Ask};
use crate::{
    g2::pv1::{Body, Subject},
    state::Shell,
};
use flea::{
    json::escape,
    protocol::RowFacts,
    ui_logic_pv1::{compare, kinds::State, rect::Shape},
};
use gpui::{Context, Window};

impl Shell {
    /// Ops.js openCompare: the pane's two selected regular files open the Compare card; false for any other selection.
    pub fn open_compare(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pane) = self.panes[side]
            .as_ref()
            .filter(|p| !p.pane.busy() && !p.stopped)
        else {
            return false;
        };
        let picked: Vec<RowFacts> = pane
            .selected
            .iter()
            .filter_map(|&index| pane.pane.held.row(index).cloned())
            .collect();
        let modes: Vec<u32> = picked
            .iter()
            .map(|row| if row.dir { 0o040000 } else { row.mode })
            .collect();
        if picked.len() != pane.selected.len() || !compare::can_compare(&modes) {
            return false;
        }
        let base = pane.pane.path().to_owned();
        let [a, b] =
            [&picked[0], &picked[1]].map(|row| crate::navigation::history::join(&base, &row.name));
        self.open_compare_paths(side, a, b, &picked[0], window, cx);
        true
    }

    /// The Compare card for two paths; the bar's mark is `mark`'s, the first file's row.
    pub(crate) fn open_compare_paths(
        &mut self,
        side: usize,
        a: String,
        b: String,
        mark: &RowFacts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let catalog = self.locale.catalog.clone();
        let title = compare::title(compare::leaf(&a), compare::leaf(&b), None, &catalog);
        let subject = Subject {
            name: compare::leaf(&a).into(),
            icon: if mark.icon.is_empty() {
                "text-x-generic".into()
            } else {
                mark.icon.clone()
            },
            mode: mark.mode,
            state: State::Text,
            shape: Shape::unknown(State::Unsupported),
            body: Body::Refused {
                title,
                sentence: String::new(),
            },
        };
        let token = token();
        let line = format!(
            r#"{{"c":"compare","token":{token},"a":"{}","b":"{}"}}"#,
            escape(&a),
            escape(&b)
        );
        let host = self.quick_look_host(side, window, cx);
        host.token = token;
        host.row = None;
        host.answer.clear();
        host.patch.clear();
        host.ask = Ask::Compare { a, b };
        self.show(subject, false, Some(line), window, cx);
    }
}
