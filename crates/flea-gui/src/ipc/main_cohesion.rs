//! ui/Ipc.qml cohesionControls over the main window: the toolbar, the crumbs and the rail rows it drew, and the list row holding the keyboard.
use super::{path::PathAdapter, Adapter};
use crate::{
    g2::rail_places::rows::Laid,
    path::chrome::{Control, Frame},
};
use flea::{ipc::wire::Reply, jsondoc::Json};

/// Toolbar names in GLYPHS order: ChromeBar.qml's untranslated v0.3.12 names, followed by the copy mark's.
const NAMES: [&str; 11] = [
    "Back",
    "Parent folder",
    "Search",
    "Filter",
    "Sort",
    "List view",
    "Columns view",
    "Grid view",
    "dual",
    "sliders",
    "Copy path",
];

pub(crate) struct MainCohesion<'a> {
    pub frame: &'a Frame,
    pub path: &'a PathAdapter<'a>,
    /// The rail's last paint while the rail is on screen.
    pub rail: Option<&'a Laid>,
    /// The cursor row's "x y width height" while the list holds the keyboard.
    pub focus: Option<String>,
    pub lane: Option<Json>,
}

struct Entry<'a> {
    kind: &'a str,
    name: &'a str,
    rect: (f32, f32, f32, f32),
    /// The toolbar button's own states; a crumb, a rail row or the focused row takes an idle, enabled one.
    state: Control,
    hoverable: bool,
    via: &'a str,
}

/// ui/js/ControlReader.js's record for one control.
fn entry(e: Entry) -> Json {
    let number = |value: f32| Json::Num(((value + 0.5).floor() as i64).to_string());
    let (x, y, w, h) = e.rect;
    Json::Obj(
        [
            ("kind", Json::Str(e.kind.into())),
            ("name", Json::Str(e.name.into())),
            ("x", number(x)),
            ("y", number(y)),
            ("w", number(w)),
            ("h", number(h)),
            ("surface", Json::Str(String::new())),
            ("exposed", Json::Bool(w > 0. && h > 0.)),
            ("targetX", number(x + w / 2.)),
            ("targetY", number(y + h / 2.)),
            ("focused", Json::Bool(e.state.focused)),
            ("hovered", Json::Bool(e.state.hovered)),
            ("pressed", Json::Bool(e.state.pressed)),
            ("hoverable", Json::Bool(e.hoverable)),
            ("hoverLive", Json::Bool(e.hoverable)),
            ("pressLive", Json::Bool(e.kind != "Row")),
            ("pressable", Json::Bool(true)),
            ("enabled", Json::Bool(e.state.enabled)),
            ("value", Json::Str(String::new())),
            ("primary", Json::Bool(false)),
            ("destructive", Json::Bool(false)),
            ("member", Json::Bool(false)),
            ("via", Json::Str(e.via.into())),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect(),
    )
}

fn plain<'a>(kind: &'a str, rect: (f32, f32, f32, f32), hoverable: bool) -> Entry<'a> {
    let mut state = Control::default();
    state.enabled = true;
    Entry {
        kind,
        name: "",
        rect,
        state,
        hoverable,
        via: "",
    }
}

impl MainCohesion<'_> {
    fn controls(&self) -> Vec<Json> {
        let mut controls = Vec::new();
        for (at, control) in self.frame.controls.iter().enumerate() {
            let Some(b) = control.bounds.filter(|_| control.visible) else {
                continue;
            };
            controls.push(entry(Entry {
                name: NAMES[at],
                state: *control,
                ..plain(
                    "ChromeButton",
                    (
                        b.origin.x.into(),
                        b.origin.y.into(),
                        b.size.width.into(),
                        b.size.height.into(),
                    ),
                    true,
                )
            }));
        }
        let view = self.path.view;
        if !view.editing() && !view.inputs.dual {
            let crumbs = view.strips[0].crumbs().len();
            controls.extend(
                (0..crumbs)
                    .filter_map(|at| self.path.crumb_box(0, at).map(|rect| (at, rect)))
                    .map(|(at, rect)| {
                        let mut row = plain("Crumb", rect, true);
                        let flags = view
                            .paint
                            .borrow()
                            .pointer
                            .pointer(&format!("crumb:0:{at}"));
                        row.state.hovered = flags.0;
                        row.state.pressed = flags.1;
                        row.name = &view.strips[0].crumbs()[at].text;
                        entry(row)
                    }),
            );
        }
        // Sidebar.qml rows take a press and draw no hover of their own, as the reader reports them.
        if let Some(laid) = self.rail {
            controls.extend(laid.rows.iter().map(|row| {
                let r = row.rect;
                entry(plain("SidebarRow", (r.x, r.y, r.width, r.height), false))
            }));
        }
        controls.extend(self.lane.iter().cloned());
        controls
    }

    // rowRect's reply, for example "232 96 640 28"; any other shape reads as no focused row.
    fn focus(&self) -> Json {
        let rect: Option<Vec<f32>> = self.focus.as_ref().map(|text| {
            text.split_whitespace()
                .filter_map(|n| n.parse().ok())
                .collect()
        });
        match rect.as_deref() {
            Some(&[x, y, w, h]) => {
                let mut row = plain("Row", (x, y, w, h), true);
                row.state.focused = true;
                row.via = "item";
                entry(row)
            }
            _ => Json::Null,
        }
    }
}

impl Adapter for MainCohesion<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        (function == "cohesionControls").then(|| {
            let controls = self.controls();
            let focus = controls
                .iter()
                .find(|row| row.get("focused").and_then(Json::as_bool) == Some(true))
                .cloned()
                .unwrap_or_else(|| self.focus());
            let value = Json::Obj(vec![
                ("captureReader".into(), Json::Str("gpui-paint-v1".into())),
                ("focus".into(), focus),
                ("controls".into(), Json::Arr(controls)),
            ]);
            Reply::value(flea::jsondoc::render(&value).trim_end())
        })
    }
}

/// What cohesionControls takes from the window before the toolbar is borrowed: the focused row and the rail's paint.
pub(crate) struct Inputs {
    pub laid: Option<std::rc::Rc<std::cell::RefCell<Laid>>>,
    focus: Option<String>,
}

impl Inputs {
    pub(crate) fn adapter<'a>(
        &self,
        frame: &'a Frame,
        path: &'a PathAdapter<'a>,
        rail: Option<&'a Laid>,
    ) -> MainCohesion<'a> {
        MainCohesion {
            frame,
            path,
            rail,
            focus: self.focus.clone(),
            lane: None,
        }
    }
}

impl crate::state::Shell {
    pub(crate) fn cohesion_inputs(
        &self,
        host: &crate::window::pane::PaneHost,
        states: &crate::list::RowStates,
        rows: &super::rows::RowsAdapter,
        cx: &gpui::App,
    ) -> Inputs {
        use crate::window::startup::View;
        let listed = matches!(host.drawn_view, Some(View::List | View::Dual));
        let focus = (states.focused && listed)
            .then(|| rows.call("rowRect", &[host.pane.cursor.to_string()]))
            .flatten()
            .map(|reply| reply.stdout.trim().to_owned());
        let laid = self
            .rail
            .as_ref()
            .filter(|join| join.host.read(cx).rail.shown())
            .map(|join| join.places.read(cx).laid.clone());
        Inputs { laid, focus }
    }
}
