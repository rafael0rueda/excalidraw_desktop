//! The selection box, its handles, and resizing and rotating with them.

use crate::geometry::{self, bounds_of, rotate, Bounds};
use crate::Modifiers;
use rough::Point;
use scene::edit::{num, update, update_num, update_points, Updates};
use scene::{Element, Kind};
use serde_json::json;
use std::f64::consts::{PI, TAU};

/// Handle size and offsets, in screen pixels (Excalidraw's for a mouse).
const HANDLE_PX: f64 = 8.0;
const PADDING_PX: f64 = 4.0;
const ROTATION_GAP_PX: f64 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Handle {
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
    Rotation,
}

impl Handle {
    const RESIZE: [Handle; 8] = [
        Handle::NW,
        Handle::N,
        Handle::NE,
        Handle::E,
        Handle::SE,
        Handle::S,
        Handle::SW,
        Handle::W,
    ];

    /// Which sides it moves: (-1 left / 0 / 1 right, -1 top / 0 / 1 bottom).
    fn sides(self) -> (i8, i8) {
        match self {
            Handle::N => (0, -1),
            Handle::S => (0, 1),
            Handle::E => (1, 0),
            Handle::W => (-1, 0),
            Handle::NE => (1, -1),
            Handle::NW => (-1, -1),
            Handle::SE => (1, 1),
            Handle::SW => (-1, 1),
            Handle::Rotation => (0, 0),
        }
    }

    pub fn is_corner(self) -> bool {
        matches!(self, Handle::NE | Handle::NW | Handle::SE | Handle::SW)
    }
}

/// Where the selection box and its handles are, in the scene.
#[derive(Clone, Debug)]
pub struct Handles {
    /// The box's corners, clockwise from top left: rotated with a single
    /// rotated element, axis-aligned around several.
    pub corners: [Point; 4],
    pub handles: Vec<(Handle, Point)>,
    /// The size of a handle square, in scene units.
    pub size: f64,
}

impl Handles {
    pub fn at(&self, p: Point, zoom: f64) -> Option<Handle> {
        let reach = self.size / 2.0 + 2.0 / zoom;
        self.handles
            .iter()
            .find(|(_, c)| (p[0] - c[0]).abs() <= reach && (p[1] - c[1]).abs() <= reach)
            .map(|(h, _)| *h)
    }
}

pub fn handles(selected: &[&Element], zoom: f64) -> Option<Handles> {
    let pad = PADDING_PX / zoom;
    let (local, frame): ([f64; 4], Box<dyn Fn(Point) -> Point>) = match selected {
        [] => return None,
        [e] => {
            let b = geometry::local_box(e);
            let e = (*e).clone();
            (b, Box::new(move |p| geometry::to_scene(&e, p)))
        }
        many => {
            let b = bounds_of(many.iter().copied())?;
            (b, Box::new(|p| p))
        }
    };
    let [x0, y0, x1, y1] = [
        local[0] - pad,
        local[1] - pad,
        local[2] + pad,
        local[3] + pad,
    ];
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map(&frame);
    let mut handles: Vec<(Handle, Point)> = Handle::RESIZE
        .iter()
        .map(|h| {
            let (sx, sy) = h.sides();
            let x = match sx {
                -1 => x0,
                1 => x1,
                _ => cx,
            };
            let y = match sy {
                -1 => y0,
                1 => y1,
                _ => cy,
            };
            (*h, frame([x, y]))
        })
        .collect();
    // A box too small for side handles keeps only its corners, as Excalidraw's does.
    let small = (x1 - x0) * zoom < HANDLE_PX * 4.0 || (y1 - y0) * zoom < HANDLE_PX * 4.0;
    if small {
        handles.retain(|(h, _)| h.is_corner());
    }
    // A two-point line is resized by its ends in Excalidraw, not a box; until
    // that editor exists (phase 3) it has no box handles, only rotation.
    if let [e] = selected {
        if e.linear.as_ref().is_some_and(|l| l.points.len() <= 2) {
            handles.clear();
        }
    }
    handles.push((Handle::Rotation, frame([cx, y0 - ROTATION_GAP_PX / zoom])));
    Some(Handles {
        corners,
        handles,
        size: HANDLE_PX / zoom,
    })
}

/// The point an element's local box is centred on, relative to its `x, y`.
fn center_local(e: &Element) -> Point {
    geometry::center(geometry::local_box(e))
}

fn sign(x: f64) -> f64 {
    if x < 0.0 {
        -1.0
    } else {
        1.0
    }
}

pub(crate) struct Resize {
    originals: Vec<Element>,
    handle: Handle,
    /// The box being resized: the element's own (local) box for one, the
    /// scene box around several.
    frame: Bounds,
    /// How far from the edge it moves the pointer grabbed the handle (the
    /// handles sit outside the box), so the first move does not jump.
    grab: Point,
}

impl Resize {
    pub fn new(originals: Vec<Element>, handle: Handle, p: Point) -> Self {
        let (frame, q) = match originals.as_slice() {
            [e] => (geometry::local_box(e), geometry::to_local(e, p)),
            many => (bounds_of(many.iter()).unwrap_or([0.0; 4]), p),
        };
        let (sx, sy) = handle.sides();
        let edge = |side: i8, lo: f64, hi: f64, q: f64| match side {
            -1 => q - lo,
            1 => q - hi,
            _ => 0.0,
        };
        let grab = [
            edge(sx, frame[0], frame[2], q[0]),
            edge(sy, frame[1], frame[3], q[1]),
        ];
        Self {
            originals,
            handle,
            frame,
            grab,
        }
    }

    /// Scale factors, and the fixed point they scale about, in the frame's
    /// coordinates, for the pointer at `q` (also in them).
    fn scale(&self, q: Point, m: Modifiers, keep_ratio: bool) -> (f64, f64, Point) {
        let q = [q[0] - self.grab[0], q[1] - self.grab[1]];
        let b = self.frame;
        let c = geometry::center(b);
        let (sx_side, sy_side) = self.handle.sides();
        let axis = |side: i8, lo: f64, hi: f64, mid: f64, q: f64| -> (f64, f64) {
            match side {
                0 => (1.0, mid),
                _ => {
                    let (from, to) = if m.alt {
                        (mid, if side > 0 { hi } else { lo })
                    } else if side > 0 {
                        (lo, hi)
                    } else {
                        (hi, lo)
                    };
                    let span = to - from;
                    (if span == 0.0 { 1.0 } else { (q - from) / span }, from)
                }
            }
        };
        let (mut sx, fx) = axis(sx_side, b[0], b[2], c[0], q[0]);
        let (mut sy, fy) = axis(sy_side, b[1], b[3], c[1], q[1]);
        if keep_ratio {
            if self.handle.is_corner() {
                let s = sx.abs().max(sy.abs());
                (sx, sy) = (s * sign(sx), s * sign(sy));
            } else if sx_side != 0 {
                sy = sx.abs();
            } else {
                sx = sy.abs();
            }
        }
        (sx, sy, [fx, fy])
    }

    pub fn updates(&self, p: Point, m: Modifiers) -> Vec<(String, Updates)> {
        match self.originals.as_slice() {
            [e] => {
                // Images keep their shape unless Shift; text always does.
                let keep = match e.kind {
                    Kind::Image => !m.shift,
                    Kind::Text => true,
                    _ => m.shift,
                };
                let q = geometry::to_local(e, p);
                let (sx, sy, fixed) = self.scale(q, m, keep);
                vec![(
                    e.id.clone(),
                    scaled(
                        e,
                        sx,
                        sy,
                        fixed,
                        |local| geometry::to_scene(e, local),
                        e.angle,
                    ),
                )]
            }
            many => {
                let (sx, sy, fixed) = self.scale(p, m, m.shift);
                many.iter()
                    .map(|e| {
                        // Each element keeps its own rotation; its centre moves
                        // with the scaling about the fixed point.
                        let c = geometry::rotation_center(e);
                        let c2 = [
                            fixed[0] + (c[0] - fixed[0]) * sx,
                            fixed[1] + (c[1] - fixed[1]) * sy,
                        ];
                        let cl = center_local(e);
                        let anchor = move |_: Point| c2;
                        (
                            e.id.clone(),
                            scaled(e, sx.abs(), sy.abs(), cl, anchor, e.angle),
                        )
                    })
                    .collect()
            }
        }
    }
}

/// Updates that scale `e` by `sx, sy` about `fixed` (local coordinates),
/// with `fixed` landing on `anchor(fixed)` in the scene.
fn scaled(
    e: &Element,
    sx: f64,
    sy: f64,
    fixed: Point,
    anchor: impl Fn(Point) -> Point,
    angle: f64,
) -> Updates {
    let map = |p: Point| {
        [
            fixed[0] + (p[0] - fixed[0]) * sx,
            fixed[1] + (p[1] - fixed[1]) * sy,
        ]
    };
    let mut out: Updates = Vec::new();
    // Where the new element's local origin is, in the old local frame, and
    // the new local box.
    let (origin, new_box) = if let Some(l) = &e.linear {
        let pts: Vec<Point> = l.points.iter().map(|p| map(*p)).collect();
        let o = pts[0];
        let pts: Vec<Point> = pts.iter().map(|p| [p[0] - o[0], p[1] - o[1]]).collect();
        let b = box_of(&pts);
        out.push(update_points(&pts));
        (o, b)
    } else if let Some(f) = &e.free_draw {
        let pts: Vec<Point> = f.points.iter().map(|p| map(*p)).collect();
        let b = box_of(&pts);
        out.push(update_points(&pts));
        ([0.0, 0.0], b)
    } else {
        let a = map([0.0, 0.0]);
        let z = map([e.width, e.height]);
        let (w, h) = ((z[0] - a[0]).abs(), (z[1] - a[1]).abs());
        out.push(update_num("width", w));
        out.push(update_num("height", h));
        if let Some(t) = &e.text {
            out.push(update_num("fontSize", t.font_size * sx.abs().min(sy.abs())));
        }
        if let Some(img) = &e.image {
            // Dragged past the opposite side: the image flips, as in Excalidraw.
            let flip = [img.scale[0] * sign(sx), img.scale[1] * sign(sy)];
            if flip != img.scale {
                out.push(update("scale", json!([num(flip[0]), num(flip[1])])));
            }
        }
        ([a[0].min(z[0]), a[1].min(z[1])], [0.0, 0.0, w, h])
    };
    // The fixed point stays put in the scene: solve for the new x, y.
    let fixed_world = anchor(fixed);
    let fixed_new = [map(fixed)[0] - origin[0], map(fixed)[1] - origin[1]];
    let c = geometry::center(new_box);
    let r = rotate(
        [fixed_new[0] - c[0], fixed_new[1] - c[1]],
        [0.0, 0.0],
        angle,
    );
    out.push(update_num("x", fixed_world[0] - c[0] - r[0]));
    out.push(update_num("y", fixed_world[1] - c[1] - r[1]));
    out
}

fn box_of(pts: &[Point]) -> Bounds {
    pts.iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |b, p| {
            [
                b[0].min(p[0]),
                b[1].min(p[1]),
                b[2].max(p[0]),
                b[3].max(p[1]),
            ]
        },
    )
}

pub(crate) struct Rotate {
    originals: Vec<Element>,
    center: Point,
    start: f64,
}

impl Rotate {
    pub fn new(originals: Vec<Element>, p: Point) -> Self {
        let center = match originals.as_slice() {
            [e] => geometry::rotation_center(e),
            many => geometry::center(bounds_of(many.iter()).unwrap_or([0.0; 4])),
        };
        let start = (p[1] - center[1]).atan2(p[0] - center[0]);
        Self {
            originals,
            center,
            start,
        }
    }

    pub fn updates(&self, p: Point, m: Modifiers) -> Vec<(String, Updates)> {
        let mut delta = (p[1] - self.center[1]).atan2(p[0] - self.center[0]) - self.start;
        // `SHIFT_LOCKING_ANGLE`: 15° steps.
        let step = PI / 12.0;
        let single = self.originals.len() == 1;
        self.originals
            .iter()
            .map(|e| {
                let mut angle = e.angle + delta;
                if m.shift {
                    if single {
                        angle = (angle / step).round() * step;
                    } else {
                        delta = (delta / step).round() * step;
                        angle = e.angle + delta;
                    }
                }
                let angle = angle.rem_euclid(TAU);
                let c = rotate(geometry::rotation_center(e), self.center, angle - e.angle);
                let cl = center_local(e);
                (
                    e.id.clone(),
                    vec![
                        update_num("angle", angle),
                        update_num("x", c[0] - cl[0]),
                        update_num("y", c[1] - cl[1]),
                    ],
                )
            })
            .collect()
    }
}
