//! One 3D view's state: its camera and home, the turntable rules, plates, layers and wireframe, driven by named actions.
use super::camera::{self, Framing, Home, View};
use super::motion::{DragTrack, Motion, ORBIT_KEY, ORBIT_KEY_MS, RESET_MS, ZOOM_KEY, ZOOM_KEY_MS};
use super::scene::{PlateRect, Scene};

/// Where the view is shown; only Quick Look turns on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    QuickLook,
    Column,
    /// The terminal preview: 15 degree orbit steps on the keys and no turntable.
    Terminal,
}

/// The moves the preview context's keys name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    OrbitLeft,
    OrbitRight,
    ZoomIn,
    ZoomOut,
    Reset,
    ToggleTurn,
    Wireframe,
    PlatePrevious,
    PlateNext,
    LayerUp,
    LayerDown,
}

/// The key table action a 3D view answers, by the names the preview context already binds plus the new ones.
pub fn action(name: &str) -> Option<Action> {
    Some(match name {
        "seekBack" => Action::OrbitLeft,
        "seekForward" => Action::OrbitRight,
        "zoomIn" => Action::ZoomIn,
        "zoomOut" => Action::ZoomOut,
        "zoomFit" | "modelReset" => Action::Reset,
        "playPause" => Action::ToggleTurn,
        "toggleWrap" | "wireframe" => Action::Wireframe,
        "tabPrevious" | "platePrevious" => Action::PlatePrevious,
        "tabNext" | "plateNext" => Action::PlateNext,
        "pageUp" | "layerUp" => Action::LayerUp,
        "pageDown" | "layerDown" => Action::LayerDown,
        _ => return None,
    })
}

/// A view's state, on the caller's clock in seconds.
#[derive(Clone, Debug)]
pub struct Viewer {
    pub surface: Surface,
    pub view: View,
    pub home: View,
    pub rect: PlateRect,
    pub framing: Framing,
    pub plate: usize,
    plates: usize,
    /// The G-code layer the view stands at, counted from 0; `None` for a mesh.
    pub layer: Option<usize>,
    layers: usize,
    pub wire: bool,
    /// The person paused the turntable with p; it stays off until p again.
    pub turn_off: bool,
    motion: Motion,
    drag: DragTrack,
    limits: (f32, f32),
    size: (f32, f32),
    shape: Home,
}

impl Viewer {
    /// A view of `scene`'s plate `plate` in a `w` by `h` view, `None` when the plate holds nothing to draw.
    pub fn new(surface: Surface, scene: &Scene, plate: usize, w: f32, h: f32) -> Option<Self> {
        let plates = scene.plate_count();
        let plate = plate.min(plates - 1);
        let shape = Home::new(scene, plate)?;
        Some(Self::from_home(surface, shape, (plate, plates), scene.layers.len(), w, h))
    }

    /// A view from a start measured beforehand, standing at plate `plate` of `plates`, so nothing walks the model on the UI thread.
    pub fn from_home(surface: Surface, shape: Home, (plate, plates): (usize, usize), layers: usize, w: f32, h: f32) -> Self {
        let home = shape.view(w, h);
        Self {
            surface,
            view: home,
            home,
            rect: shape.rect,
            framing: shape.framing,
            plate,
            plates,
            layer: (layers > 0).then(|| layers - 1),
            layers,
            wire: false,
            turn_off: false,
            motion: Motion::default(),
            drag: DragTrack::default(),
            limits: camera::zoom_limits(shape.radius, h),
            size: (w, h),
            shape,
        }
    }

    /// Whether the turntable may run: Quick Look, not paused by p, and the compositor animating.
    pub fn may_turn(&self, animations: bool) -> bool {
        self.surface == Surface::QuickLook && !self.turn_off && animations
    }

    /// Quick Look opened or was shown again: the turn starts from rest when it may.
    pub fn opened(&mut self, now: f64, animations: bool) {
        if self.may_turn(animations) {
            self.motion.start_spin(now);
        }
    }

    pub fn spinning(&self) -> bool {
        self.motion.spinning()
    }

    pub fn moving(&self) -> bool {
        self.motion.moving()
    }

    /// Hyprland's animations went off: the turn stops where it is, unlike the person's pause it is not remembered.
    pub fn still(&mut self) {
        self.motion.stop_spin();
    }

    /// The window lost focus or the preview was hidden: nothing advances and no frame is wanted.
    pub fn pause(&mut self) {
        self.motion.pause();
    }

    pub fn resume(&mut self, now: f64) {
        self.motion.resume(now);
    }

    /// Advances to `now`; answers whether another frame is wanted.
    pub fn advance(&mut self, now: f64) -> bool {
        self.motion.advance(&mut self.view, now)
    }

    /// The view was resized: home is framed again, and a view still at home follows it.
    pub fn resize(&mut self, w: f32, h: f32) {
        if (w, h) == self.size {
            return;
        }
        let at_home = self.view == self.home;
        self.home = self.shape.view(w, h);
        self.limits = camera::zoom_limits(self.shape.radius, h);
        if at_home {
            self.view = self.home;
        } else {
            self.view.cx += (w - self.size.0) / 2.;
            self.view.cy += (h - self.size.1) / 2.;
        }
        self.size = (w, h);
    }

    /// A press on the model: the turn stops where it is.
    pub fn press(&mut self, now: f64) {
        self.motion.interact();
        self.drag = DragTrack::default();
        self.drag.record(now, &self.view);
    }

    /// A left drag orbits by (dx, dy) pixels; a pan drag moves the turn axis with the pointer.
    pub fn drag(&mut self, dx: f32, dy: f32, pan: bool, now: f64) {
        if pan {
            self.view.pan(dx, dy);
        } else {
            self.view.orbit(dx, dy);
            self.drag.record(now, &self.view);
        }
    }

    /// Release carries an orbit on with inertia when the pointer was still moving.
    pub fn release(&mut self, pan: bool, now: f64) {
        if !pan {
            if let Some((v_az, v_el)) = self.drag.release(now) {
                self.motion.glide(v_az, v_el, now);
            }
        }
        self.drag = DragTrack::default();
    }

    /// The wheel zooms toward the pointer; `delta` is pixels or lines, positive as the wheel turns toward the person, which zooms out.
    pub fn wheel(&mut self, x: f32, y: f32, delta: f32, lines: bool, _now: f64) {
        self.motion.interact();
        let factor = (delta * if lines { 0.05 } else { 0.0016 }).exp();
        self.view.zoom_at(x, y, 1. / factor, self.limits);
    }

    /// A double-click frames the model again.
    pub fn double_click(&mut self, now: f64) {
        self.motion.interact();
        self.motion.tween(self.view, self.home, RESET_MS, now);
    }

    /// Applies a key action; answers whether the view changed and wants a frame.
    pub fn act(&mut self, action: Action, now: f64, animations: bool) -> bool {
        let mut to = self.view;
        match action {
            Action::OrbitLeft | Action::OrbitRight => {
                self.motion.interact();
                to.az += if action == Action::OrbitLeft { ORBIT_KEY } else { -ORBIT_KEY };
                self.motion.tween(self.view, to, ORBIT_KEY_MS, now);
            }
            Action::ZoomIn | Action::ZoomOut => {
                self.motion.interact();
                let factor = if action == Action::ZoomIn { ZOOM_KEY } else { 1. / ZOOM_KEY };
                to.zoom_at(self.size.0 / 2., self.size.1 / 2., factor, self.limits);
                self.motion.tween(self.view, to, ZOOM_KEY_MS, now);
            }
            Action::Reset => {
                self.motion.interact();
                self.motion.tween(self.view, self.home, RESET_MS, now);
            }
            Action::ToggleTurn => {
                if self.surface != Surface::QuickLook {
                    return false;
                }
                if self.motion.spinning() {
                    self.motion.stop_spin();
                    self.turn_off = true;
                } else {
                    self.turn_off = false;
                    if !animations {
                        return false;
                    }
                    self.motion.interact();
                    self.motion.start_spin(now);
                }
            }
            Action::Wireframe => {
                if self.layers > 0 || self.surface == Surface::Column {
                    return false;
                }
                self.wire = !self.wire;
            }
            Action::PlatePrevious | Action::PlateNext => return self.plates > 1 && self.surface != Surface::Column,
            Action::LayerUp | Action::LayerDown => {
                let Some(layer) = self.layer else { return false };
                let next = if action == Action::LayerUp { (layer + 1).min(self.layers - 1) } else { layer.saturating_sub(1) };
                if next == layer {
                    return false;
                }
                self.layer = Some(next);
            }
        }
        true
    }

    /// The plate a plate step moves to, wrapping as the bar's words do; the caller loads it.
    pub fn stepped_plate(&self, action: Action) -> usize {
        match action {
            Action::PlateNext => (self.plate + 1) % self.plates.max(1),
            Action::PlatePrevious => (self.plate + self.plates.max(1) - 1) % self.plates.max(1),
            _ => self.plate,
        }
    }

    /// The layer row's position, 0 to 1.
    pub fn layer_share(&self) -> Option<f32> {
        let layer = self.layer?;
        Some(if self.layers > 1 { (layer + 1) as f32 / self.layers as f32 } else { 1. })
    }

    /// Moves the layer to the one under a click or drag at `share` of the row.
    pub fn seek_layer(&mut self, share: f32) -> bool {
        if self.layers == 0 {
            return false;
        }
        let next = ((share.clamp(0., 1.) * self.layers as f32).ceil() as usize).clamp(1, self.layers) - 1;
        let changed = self.layer != Some(next);
        self.layer = Some(next);
        changed
    }

    pub fn layers(&self) -> usize {
        self.layers
    }

    pub fn plates(&self) -> usize {
        self.plates
    }
}
