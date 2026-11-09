//! The 3D view's orthographic camera: an isometric start framed to the model or the printer's plate, orbit, pan and zoom.
use super::scene::{PlateRect, Scene};
use super::vec::{add, cross, dot, norm, scale, sub, V3};
use std::f32::consts::PI;

/// The isometric start: 45 degrees round, 35.26 degrees up.
pub const ISO_AZ: f32 = PI / 4.;
pub fn iso_el() -> f32 {
    (1. / 2f32.sqrt()).atan()
}
/// The pitch stops this short of straight down or up, so the plate never flips.
pub const PITCH_LIMIT: f32 = 89. * PI / 180.;
/// Radians of orbit per pixel dragged.
pub const ORBIT_PER_PX: f32 = 0.0085;
/// The share of the view an STL and its plate's sweep take.
pub const MODEL_FILL: f32 = 0.94;
/// The share of the view a printer's whole plate takes, so its edge stays inset from the card, the bar and the strip.
pub const PLATE_FILL: f32 = 0.88;
/// The share of a thumbnail the rendered model takes.
pub const THUMB_FILL: f32 = 0.92;
// The light sits at the viewer's upper left and turns with the camera: right, up and toward the viewer.
const LIGHT: V3 = [-0.55, 0.65, 0.5];
// The zoom limits, as the distances a 32 degree perspective camera would stand at in model radii.
const ZOOM_NEAR: f32 = 0.7;
const ZOOM_FAR: f32 = 14.;
const HALF_FOV: f32 = 16. * PI / 180.;
// Samples of the circle the plate sweeps while it turns, which the start frames so a turn never clips.
const SWEEP: usize = 36;

/// The view's right, up and toward-the-viewer axes, a right-handed frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Basis {
    pub r: V3,
    pub u: V3,
    pub d: V3,
}

/// An orthographic view: `az` turns about the vertical axis through `target`, `el` is the pitch above the plate; `scale` is pixels per millimetre and (`cx`, `cy`) is where `target` lands in the view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub az: f32,
    pub el: f32,
    pub scale: f32,
    pub cx: f32,
    pub cy: f32,
    pub target: V3,
}

impl View {
    pub fn basis(&self) -> Basis {
        let (sa, ca, se, ce) = (self.az.sin(), self.az.cos(), self.el.sin(), self.el.cos());
        let d = [ce * ca, ce * sa, se];
        let r = [-sa, ca, 0.];
        Basis { r, u: cross(d, r), d }
    }

    /// The light in world space for this view.
    pub fn light(&self) -> V3 {
        let b = self.basis();
        norm(add(add(scale(b.r, LIGHT[0]), scale(b.u, LIGHT[1])), scale(b.d, LIGHT[2])))
    }

    /// The point's place in the view in pixels and its depth toward the viewer in millimetres.
    pub fn project(&self, basis: &Basis, p: V3) -> [f32; 3] {
        let q = sub(p, self.target);
        [self.cx + self.scale * dot(q, basis.r), self.cy - self.scale * dot(q, basis.u), dot(q, basis.d)]
    }

    /// A drag of (dx, dy) pixels: the turn follows the pointer without limit, the pitch stops at 89 degrees.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.az -= dx * ORBIT_PER_PX;
        self.el = clamp_pitch(self.el + dy * ORBIT_PER_PX);
    }

    /// A pan of (dx, dy) pixels moves the turn axis with the pointer.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let b = self.basis();
        self.target = add(self.target, scale(sub(scale(b.u, dy), scale(b.r, dx)), 1. / self.scale));
    }

    /// Zooms by `factor` (above 1 brings the model closer) keeping the point under (px, py) where it is, within the limits a perspective camera would keep between 0.7 and 14 model radii.
    pub fn zoom_at(&mut self, px: f32, py: f32, factor: f32, limits: (f32, f32)) {
        let wanted = (self.scale * factor).clamp(limits.0, limits.1);
        if !wanted.is_finite() || wanted <= 0. {
            return;
        }
        let b = self.basis();
        let shift = 1. / self.scale - 1. / wanted;
        self.target = add(self.target, scale(sub(scale(b.r, px - self.cx), scale(b.u, py - self.cy)), shift));
        self.scale = wanted;
    }
}

pub fn clamp_pitch(el: f32) -> f32 {
    el.clamp(-PITCH_LIMIT, PITCH_LIMIT)
}

/// The scale limits for a model of half-diagonal `radius` in a view `height` pixels tall.
pub fn zoom_limits(radius: f32, height: f32) -> (f32, f32) {
    let r = radius.max(f32::EPSILON);
    let at = |distance: f32| height / (2. * distance * r * HALF_FOV.tan());
    (at(ZOOM_FAR), at(ZOOM_NEAR))
}

/// Scales until the points' projected box fills `fill` of a `w` by `h` view, then centres that box; `target` is the turn axis, the box's centre when `None`.
pub fn frame(points: &[V3], az: f32, el: f32, w: f32, h: f32, fill: f32, target: Option<V3>) -> View {
    extent(points, az, el, fill, target).fit(w, h)
}

/// The points' box as seen at (`az`, `el`), measured once so the view can be framed again at any size without them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Extent {
    az: f32,
    el: f32,
    fill: f32,
    target: V3,
    x: (f32, f32),
    y: (f32, f32),
}

/// Measures `points` for `frame`.
pub fn extent(points: &[V3], az: f32, el: f32, fill: f32, target: Option<V3>) -> Extent {
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for p in points {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    let target = target.unwrap_or([(lo[0] + hi[0]) / 2., (lo[1] + hi[1]) / 2., (lo[2] + hi[2]) / 2.]);
    let unit = View { az, el, scale: 1., cx: 0., cy: 0., target };
    let b = unit.basis();
    let (mut x0, mut x1, mut y0, mut y1) = (f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY);
    for &p in points {
        let [x, y, _] = unit.project(&b, p);
        (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
    }
    Extent { az, el, fill, target, x: (x0, x1), y: (y0, y1) }
}

impl Extent {
    /// The view that fills `fill` of a `w` by `h` view with the box, centred.
    pub fn fit(&self, w: f32, h: f32) -> View {
        let ((x0, x1), (y0, y1)) = (self.x, self.y);
        let span = |a: f32, b: f32| if b > a { b - a } else { f32::EPSILON };
        let s = self.fill * (w / span(x0, x1)).min(h / span(y0, y1));
        let s = if s.is_finite() && s > 0. { s } else { 1. };
        View { az: self.az, el: self.el, scale: s, cx: w / 2. - s * (x1 + x0) / 2., cy: h / 2. - s * (y1 + y0) / 2., target: self.target }
    }
}

/// How a file's start is framed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Framing {
    /// No printer named: the model and the circle its plate sweeps, turning about the plate's centre.
    Model,
    /// A printer's whole plate with the model on it, as a slicer opens a project.
    Plate,
}

/// The start view for plate `plate` of `scene` in a `w` by `h` view, with the plate it stands on.
pub fn home(scene: &Scene, plate: usize, w: f32, h: f32) -> Option<(View, PlateRect, Framing)> {
    let shape = Home::new(scene, plate)?;
    Some((shape.view(w, h), shape.rect, shape.framing))
}

/// Everything the start view needs from the model, measured once off the UI thread; framing it at a new size walks no points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Home {
    pub rect: PlateRect,
    pub framing: Framing,
    /// The model's half-diagonal, the radius the zoom limits are measured in.
    pub radius: f32,
    extent: Extent,
}

impl Home {
    pub fn new(scene: &Scene, plate: usize) -> Option<Self> {
        let (lo, hi) = scene.bounds(plate)?;
        let rect = super::scene::plate_rect(lo, hi, scene.bed, scene.offset);
        let mut points = Vec::new();
        scene.for_each_point(plate, |p| points.push(p));
        let framing = if scene.bed.is_some() { Framing::Plate } else { Framing::Model };
        let extent = match framing {
            Framing::Plate => {
                points.extend(rect.corners());
                extent(&points, ISO_AZ, iso_el(), PLATE_FILL, None)
            }
            Framing::Model => {
                let [px, py] = rect.centre();
                let reach = (rect.w * rect.w + rect.h * rect.h).sqrt() / 2.;
                points.extend((0..SWEEP).map(|k| {
                    let a = k as f32 * 2. * PI / SWEEP as f32;
                    [px + reach * a.cos(), py + reach * a.sin(), 0.]
                }));
                extent(&points, ISO_AZ, iso_el(), MODEL_FILL, Some([px, py, (lo[2] + hi[2]) / 2.]))
            }
        };
        Some(Self { rect, framing, radius: radius(lo, hi), extent })
    }

    /// The start view in a `w` by `h` view.
    pub fn view(&self, w: f32, h: f32) -> View {
        self.extent.fit(w, h)
    }
}

/// The thumbnail view: the plate's model alone, isometric, framed tight.
pub fn thumbnail(scene: &Scene, plate: usize, w: f32, h: f32) -> Option<View> {
    let mut points = Vec::new();
    scene.for_each_point(plate, |p| points.push(p));
    (!points.is_empty()).then(|| frame(&points, ISO_AZ, iso_el(), w, h, THUMB_FILL, None))
}

/// Half the bounding box's diagonal, the model radius the zoom limits are measured in.
pub fn radius(lo: V3, hi: V3) -> f32 {
    let d = sub(hi, lo);
    dot(d, d).sqrt() / 2.
}
