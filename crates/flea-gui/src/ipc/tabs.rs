//! Tab strip readers, and the taken acknowledgment another window sends when it opened a tab lifted from this one.
use super::Adapter;
use crate::tabs::{
    bar::{self, Bar},
    state::TabState,
};
use flea::ipc::wire::{Reply, Request};

pub struct TabsAdapter<'a> {
    pub state: &'a TabState,
    pub bar: &'a Bar,
    pub path: &'a str,
    pub home: &'a str,
    pub home_label: &'a str,
    // Whether the strip is drawn, and the window y it is drawn at.
    pub visible: bool,
    pub top: f32,
}

// Half away from zero for the positive window coordinates a centre takes, as the shipped readers round.
fn round(value: f32) -> i64 {
    (value + 0.5).floor() as i64
}

impl TabsAdapter<'_> {
    // "x y" in window pixels at tab i's centre, empty when the strip is not drawn or no tab sits there.
    fn centre(&self, argument: Option<&String>) -> String {
        let count = self.state.count();
        let index = argument
            .and_then(|value| value.trim().parse::<i64>().ok())
            .and_then(|index| usize::try_from(index).ok())
            .filter(|index| *index < count);
        let Some(index) = index.filter(|_| self.visible) else {
            return String::new();
        };
        let order = self.bar.layout_order(count);
        let slot = order.iter().position(|at| *at == index).unwrap_or(index);
        let width = self.bar.tab_width(count);
        let x = bar::PAD_X + slot as f32 * width + width / 2.;
        let y = self.top + bar::HEIGHT / 2.;
        format!("{} {}", round(x), round(y))
    }
}

impl Adapter for TabsAdapter<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        let value = match function {
            "tabCount" => self.state.count().to_string(),
            "tabIndex" => self.state.current().to_string(),
            "tabLabels" => self
                .state
                .labels(self.path, self.home, self.home_label)
                .join("|"),
            "tabBarVisible" => self.visible.to_string(),
            "tabCentre" => self.centre(arguments.first()),
            _ => return None,
        };
        Some(Reply::value(value))
    }
}

// The token a fleatab.taken call carries, once the registry has validated it.
pub fn taken(request: &Request) -> Option<String> {
    let Request::Call {
        target,
        function,
        arguments,
    } = request
    else {
        return None;
    };
    (target == "fleatab" && function == "taken" && arguments.len() == 1)
        .then(|| arguments[0].clone())
}

pub fn answer(taken: bool) -> Reply {
    Reply::value(taken.to_string())
}

// The acknowledgment this window sends a lift's source, run off the UI thread since it waits on a socket.
pub fn send_taken(pid: &str, token: &str) -> Reply {
    let Ok(pid) = pid.parse::<u32>() else {
        return Reply::error(2, "flea ipc: the source pid is not a number");
    };
    let root = match flea::ipc::endpoint::runtime() {
        Ok(root) => root,
        Err(error) => return Reply::error(255, format!("flea ipc: {error}")),
    };
    flea::ipc::client::execute(
        &root,
        &flea::ipc::client::Options {
            pid: Some(pid),
            request: Request::Call {
                target: "fleatab".into(),
                function: "taken".into(),
                arguments: vec![token.into()],
            },
        },
    )
}

/// The pane's swapState with the strip's own counters beside them, under one key.
pub struct SwapWithStrip<'a> {
    pub pane: &'a dyn Adapter,
    pub strip: String,
    /// The positions the view draws under a standing filter, listingWindowState's shownTotal.
    pub shown: usize,
}

impl Adapter for SwapWithStrip<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        let mut reply = self.pane.call(function, arguments)?;
        // Sample input: {"held":40,"loaded":40,"windowSize":40,"total":120,"shownTotal":120}, ipc/pane.rs's shape, shownTotal last.
        if function == "listingWindowState" && reply.code == 0 {
            if let Some((head, _)) = reply.stdout.trim_end().rsplit_once(",\"shownTotal\":") {
                reply = Reply::value(format!("{head},\"shownTotal\":{}}}", self.shown));
            }
        }
        if function == "swapState" && reply.code == 0 {
            if let Some(body) = reply.stdout.trim_end().strip_suffix('}') {
                reply = Reply::value(format!("{body},\"strip\":{}}}", self.strip));
            }
        }
        Some(reply)
    }
}

/// Native geometry and labels come from the completed strip, including during a held pane swap.
pub struct MountedTabsAdapter<'a> {
    pub model: TabsAdapter<'a>,
    pub frame: Option<&'a bar::Frame>,
    pub identities: &'a [u64],
    pub bounds: Option<gpui::Bounds<gpui::Pixels>>,
    /// Live tear-off catchers, lifts and compositor client queries, for the GPUI-only tabTearoffState.
    pub tearoff: [usize; 3],
}

impl Adapter for MountedTabsAdapter<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        // GPUI-only: the tabtearoff.qml catcher layer (namespace flea-tab-tearoff) and TabDragGeometry's one client query per lift.
        if function == "tabTearoffState" {
            let [catchers, lifts, queries] = self.tearoff;
            return Some(Reply::value(format!(
                r#"{{"catchers":{catchers},"namespace":"flea-tab-tearoff","lifts":{lifts},"clientQueries":{queries}}}"#
            )));
        }
        // A drawn strip answers its painted labels; an undrawn one (one tab, the dual view) the model's (Ipc.qml tabLabels).
        if let (Some(frame), "tabLabels") = (self.frame, function) {
            let labels = frame
                .labels
                .iter()
                .map(|label| label.as_ref())
                .collect::<Vec<_>>()
                .join("|");
            return Some(Reply::value(labels));
        }
        if function != "tabCentre" {
            return self.model.call(function, arguments);
        }
        let value = (|| {
            let frame = self.frame?;
            let bounds = self.bounds.filter(|_| self.model.visible)?;
            let index = arguments.first()?.trim().parse::<usize>().ok()?;
            let identity = self.model.state.tabs.as_ref()?.items.get(index)?.identity;
            let drawn = self
                .identities
                .iter()
                .position(|drawn| *drawn == identity)?;
            let slot = *frame.slots.get(drawn)?;
            let x = f32::from(bounds.origin.x)
                + bar::PAD_X
                + slot as f32 * frame.tab_width
                + frame.tab_width / 2.;
            let y = f32::from(bounds.origin.y) + bar::HEIGHT / 2.;
            Some(format!("{} {}", round(x), round(y)))
        })()
        .unwrap_or_default();
        Some(Reply::value(value))
    }
}
