//! TabBar.qml's accessible strip: each painted tab is a named tab and New tab a button, both pressable.
use super::{atomic::Press, bar::Frame, bar::HEIGHT, bar::HIT_MIN, bar::PAD_X};
use crate::state::Shell;
use gpui::{
    accesskit::{Action, Node, NodeId, Rect},
    point, px, size, A11ySubtreeBuilder, App, Bounds, Pixels, Role, WeakEntity, Window,
};

pub(crate) struct Controls {
    pub shell: WeakEntity<Shell>,
    pub side: usize,
    pub identities: Vec<u64>,
    pub new_label: String,
}

/// What prepaint hands the accessible children and their press listeners.
#[derive(Default)]
pub struct Painted {
    bounds: Bounds<Pixels>,
    actions: Vec<(NodeId, Press)>,
    scale: f32,
}

impl Painted {
    pub(super) fn new(bounds: Bounds<Pixels>, scale: f32) -> Self {
        Self {
            bounds,
            scale,
            ..Self::default()
        }
    }
}

impl Frame {
    /// The box of drawn tab `index`, or of New tab one past the last.
    pub(crate) fn control_bounds(
        &self,
        bounds: Bounds<Pixels>,
        index: usize,
    ) -> Option<Bounds<Pixels>> {
        let count = self.labels.len();
        let (slot, width) = if index == count {
            (count, HIT_MIN)
        } else {
            (*self.slots.get(index)?, self.tab_width)
        };
        Some(Bounds::new(
            point(
                bounds.left() + px(PAD_X + slot as f32 * self.tab_width),
                bounds.top(),
            ),
            size(px(width), px(HEIGHT)),
        ))
    }
}

pub(super) fn node(frame: &Frame, index: usize, new_label: &str) -> Node {
    let tab = frame.labels.get(index);
    let mut node = Node::new(if tab.is_some() {
        Role::Tab
    } else {
        Role::Button
    });
    node.set_label(tab.map_or(new_label, |label| label.as_ref()));
    if tab.is_some() {
        node.set_selected(index == frame.current);
    }
    node.add_action(Action::Click);
    node
}

impl Controls {
    /// The strip's controls, from the same snapshot the strip's frame was drawn from.
    pub(super) fn new(shell: WeakEntity<Shell>, host: &Shell) -> Option<Self> {
        let snapshot = host.strip.snapshot.as_ref()?;
        Some(Self {
            shell,
            side: snapshot.side,
            identities: snapshot.identities.clone(),
            new_label: host.locale.catalog.tr("keys.action.tabNew"),
        })
    }

    /// Drawn control `index` presses its tab's identity; one past the last tab is New tab.
    pub(super) fn press(&self, index: usize) -> Press {
        Press {
            side: self.side,
            identity: self.identities.get(index).copied(),
            close: false,
        }
    }

    pub(super) fn children(
        &self,
        frame: &Frame,
        painted: &mut Painted,
        builder: &mut A11ySubtreeBuilder,
    ) {
        for index in 0..=frame.labels.len() {
            let Some(bounds) = frame.control_bounds(painted.bounds, index) else {
                continue;
            };
            let press = self.press(index);
            let id = builder.synthetic_node_id((self.side, press.identity));
            let mut child = node(frame, index, &self.new_label);
            child.set_bounds(device_bounds(bounds, painted.scale));
            if builder.push_child(id, child) {
                painted.actions.push((id, press));
            }
        }
    }

    /// A press acts as a left tap on the same tab or New tab, resolved by identity at press time.
    pub(super) fn actions(&self, painted: &mut Painted, window: &mut Window) {
        for (id, press) in painted.actions.drain(..) {
            let shell = self.shell.clone();
            window.on_a11y_action(id, Action::Click, move |_, window, cx: &mut App| {
                let _ = shell.update(cx, |shell, cx| shell.tab_a11y_press(press, window, cx));
            });
        }
    }
}

pub(super) fn device_bounds(bounds: Bounds<Pixels>, scale: f32) -> Rect {
    let [left, top, right, bottom] = [bounds.left(), bounds.top(), bounds.right(), bounds.bottom()]
        .map(|value| f64::from(f32::from(value) * scale));
    Rect::new(left, top, right, bottom)
}

impl Shell {
    /// A screen reader press acts as a left tap on the control it names, resolved by identity now.
    pub(super) fn tab_a11y_press(
        &mut self,
        press: Press,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let hit = self.tab_pressed_hit(press);
        self.tab_tap_hit(hit, false, window, cx);
    }
}
