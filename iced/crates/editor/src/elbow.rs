//! Elbow arrows: Excalidraw 0.18.1's `element/elbowArrow.ts` routing, ported
//! as written, with what it needs from `heading.ts`, `binding.ts` and
//! `Shape.ts`: an A* search over a grid made of the shapes' padded boxes,
//! bends made dear, so the arrow goes round shapes in few right angles.
//!
//! Not ported: dragging a segment of an elbow arrow (`fixedSegments`). An
//! arrow routed here loses any fixed segments a file gave it, and is
//! routed fresh, as Excalidraw does when none are fixed.
//!
//! Outline distances and intersections are the binding module's, whose
//! rectangles have square corners and ellipses 64 sides, so a bound end can
//! sit a fraction of a unit from where Excalidraw puts it.

use crate::binding;
use crate::geometry::rotate;
use rough::Point;
use scene::{Element, Kind};

pub type Bounds = [f64; 4];

/// `FIXED_BINDING_DISTANCE`, `BASE_PADDING`, `DEDUP_TRESHOLD`, `PRECISION`.
pub const FIXED_BINDING_DISTANCE: f64 = 5.0;
const BASE_PADDING: f64 = 40.0;
const DEDUP_THRESHOLD: f64 = 1.0;
const PRECISION: f64 = 1e-4;
const MAX_POS: f64 = 1e6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heading {
    Up,
    Right,
    Down,
    Left,
}
use Heading::*;

impl Heading {
    fn flip(self) -> Self {
        match self {
            Up => Down,
            Right => Left,
            Down => Up,
            Left => Right,
        }
    }
}

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}
fn dist(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn m_dist(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).abs() + (a[1] - b[1]).abs()
}
fn normalize(v: Point) -> Point {
    let l = v[0].hypot(v[1]);
    if l == 0.0 {
        [0.0, 0.0]
    } else {
        [v[0] / l, v[1] / l]
    }
}
/// `pointScaleFromOrigin`.
fn scale_from(p: Point, mid: Point, k: f64) -> Point {
    [mid[0] + (p[0] - mid[0]) * k, mid[1] + (p[1] - mid[1]) * k]
}
/// JavaScript's `round(x, 6)` as Excalidraw's `round` does it.
fn round6(x: f64) -> f64 {
    let p = 1e6;
    ((x + f64::EPSILON) * p).round() / p
}

/// `vectorToHeading`.
fn vector_to_heading([x, y]: Point) -> Heading {
    let (ax, ay) = (x.abs(), y.abs());
    if x > ay {
        Right
    } else if x <= -ay {
        Left
    } else if y > ax {
        Down
    } else {
        Up
    }
}

/// `pointInsideBounds`: strictly inside.
fn inside_bounds(p: Point, b: Bounds) -> bool {
    p[0] > b[0] && p[0] < b[2] && p[1] > b[1] && p[1] < b[3]
}

fn center_of(b: Bounds) -> Point {
    [b[0] + (b[2] - b[0]) / 2.0, b[1] + (b[3] - b[1]) / 2.0]
}

fn element_center(e: &Element) -> Point {
    [e.x + e.width / 2.0, e.y + e.height / 2.0]
}

/// `aabbForElement`: the rotated element's box, grown by up, right, down,
/// left.
pub fn aabb_for_element(e: &Element, offset: Option<[f64; 4]>) -> Bounds {
    let c = element_center(e);
    let corners = [[e.x, e.y], [e.x + e.width, e.y], [e.x + e.width, e.y + e.height], [e.x, e.y + e.height]].map(|p| rotate(p, c, e.angle));
    let b = [
        corners.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min),
        corners.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min),
        corners.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max),
        corners.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max),
    ];
    match offset {
        Some([up, right, down, left]) => [b[0] - left, b[1] - up, b[2] + right, b[3] + down],
        None => b,
    }
}

/// `offsetFromHeading`: `head` on the heading's side, `side` elsewhere;
/// up, right, down, left.
fn offset_from_heading(h: Heading, head: f64, side: f64) -> [f64; 4] {
    match h {
        Up => [head, side, side, side],
        Right => [side, head, side, side],
        Down => [side, side, head, side],
        Left => [side, side, side, head],
    }
}

/// `triangleIncludesPoint`.
fn in_triangle([a, b, c]: [Point; 3], p: Point) -> bool {
    let sign = |p1: Point, p2: Point, p3: Point| (p1[0] - p3[0]) * (p2[1] - p3[1]) - (p2[0] - p3[0]) * (p1[1] - p3[1]);
    let (d1, d2, d3) = (sign(p, a, b), sign(p, b, c), sign(p, c, a));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

/// `headingForDiamond`.
fn heading_for_diamond(a: Point, b: Point) -> Heading {
    let angle = (b[1] - a[1]).atan2(b[0] - a[0]).to_degrees();
    if !(45.0..315.0).contains(&angle) {
        Up
    } else if angle < 135.0 {
        Right
    } else if angle < 225.0 {
        Down
    } else {
        Left
    }
}

/// `headingForPointFromElement`: which side of `e` the point is off.
fn heading_from_element(e: &Element, aabb: Bounds, p: Point) -> Heading {
    const CONE: f64 = 2.0;
    let mid = center_of(aabb);
    if e.kind == Kind::Diamond {
        if p[0] < e.x {
            return Left;
        } else if p[1] < e.y {
            return Up;
        } else if p[0] > e.x + e.width {
            return Right;
        } else if p[1] > e.y + e.height {
            return Down;
        }
        let at = |q: Point| rotate(scale_from(q, mid, CONE), mid, e.angle);
        let top = at([e.x + e.width / 2.0, e.y]);
        let right = at([e.x + e.width, e.y + e.height / 2.0]);
        let bottom = at([e.x + e.width / 2.0, e.y + e.height]);
        let left = at([e.x, e.y + e.height / 2.0]);
        return if in_triangle([top, right, mid], p) {
            heading_for_diamond(top, right)
        } else if in_triangle([right, bottom, mid], p) {
            heading_for_diamond(right, bottom)
        } else if in_triangle([bottom, left, mid], p) {
            heading_for_diamond(bottom, left)
        } else {
            heading_for_diamond(left, top)
        };
    }
    let tl = scale_from([aabb[0], aabb[1]], mid, CONE);
    let tr = scale_from([aabb[2], aabb[1]], mid, CONE);
    let bl = scale_from([aabb[0], aabb[3]], mid, CONE);
    let br = scale_from([aabb[2], aabb[3]], mid, CONE);
    if in_triangle([tl, tr, mid], p) {
        Up
    } else if in_triangle([tr, br, mid], p) {
        Right
    } else if in_triangle([br, bl, mid], p) {
        Down
    } else {
        Left
    }
}

/// `getHeadingForElbowArrowSnap` (by way of `getBindPointHeading`): the
/// way the arrow leaves `p`.
fn bind_point_heading(p: Point, other: Point, hovered: Option<&Element>, orig: Point) -> Heading {
    let Some(e) = hovered else {
        return vector_to_heading(sub(other, p));
    };
    let d = binding::distance_to(e, p);
    let aabb = aabb_for_element(e, Some([d; 4]));
    // `getDistanceForBinding`, with no zoom given: at 100%.
    if binding::distance_to(e, orig) > binding::max_binding_gap(e, 1.0) {
        return vector_to_heading(sub(p, element_center(e)));
    }
    heading_from_element(e, aabb, p)
}

fn is_rectanguloid(e: &Element) -> bool {
    matches!(e.kind, Kind::Rectangle | Kind::Diamond | Kind::Image | Kind::Iframe | Kind::Embeddable | Kind::Frame | Kind::MagicFrame)
        || (e.kind == Kind::Text && e.text.as_ref().is_some_and(|t| t.container_id.is_none()))
}

/// `avoidRectangularCorner`: a point off a corner, moved to just off the
/// nearer side.
fn avoid_rectangular_corner(e: &Element, p: Point) -> Point {
    let c = element_center(e);
    let q = rotate(p, c, -e.angle);
    let d = FIXED_BINDING_DISTANCE;
    let (x, y, w, h) = (e.x, e.y, e.width, e.height);
    let back = |pt: Point| rotate(pt, c, e.angle);
    if q[0] < x && q[1] < y {
        if q[1] - y > -d {
            return back([x - d, y]);
        }
        return back([x, y - d]);
    } else if q[0] < x && q[1] > y + h {
        if q[0] - x > -d {
            return back([x, y + h + d]);
        }
        return back([x - d, y + h]);
    } else if q[0] > x + w && q[1] > y + h {
        if q[0] - x < w + d {
            return back([x + w, y + h + d]);
        }
        return back([x + w + d, y + h]);
    } else if q[0] > x + w && q[1] < y {
        if q[0] - x < w + d {
            return back([x + w, y - d]);
        }
        return back([x + w + d, y]);
    }
    p
}

/// `headingToMidBindPoint`.
fn heading_to_mid_bind_point(p: Point, e: &Element, aabb: Bounds) -> Point {
    let c = center_of(aabb);
    let at = match vector_to_heading(sub(p, c)) {
        Up => [(aabb[0] + aabb[2]) / 2.0 + 0.1, aabb[1]],
        Right => [aabb[2], (aabb[1] + aabb[3]) / 2.0 + 0.1],
        Down => [(aabb[0] + aabb[2]) / 2.0 - 0.1, aabb[3]],
        Left => [aabb[0], (aabb[1] + aabb[3]) / 2.0 - 0.1],
    };
    rotate(at, c, e.angle)
}

/// `bindPointToSnapToElementOutline`: the arrow's end `p`, snapped to just
/// outside `e`'s outline.
pub fn snap_to_outline(p: Point, e: &Element) -> Point {
    let aabb = aabb_for_element(e, None);
    let p = if is_rectanguloid(e) { avoid_rectangular_corner(e, p) } else { p };
    let c = center_of(aabb);
    let far = {
        let n = normalize(sub(p, c));
        let len = e.width.max(e.height);
        [c[0] + n[0] * len, c[1] + n[1] * len]
    };
    let intersection = binding::intersect_outline(e, (c, far)).into_iter().next();
    let current = dist(p, c);
    let full = dist(intersection.unwrap_or(p), c).max(PRECISION);
    let ratio = round6(current / full);
    if ratio > 0.9 {
        let close = intersection.is_some_and(|i| (p[0] - i[0]).powi(2) + (p[1] - i[1]).powi(2) < PRECISION);
        if current - full > FIXED_BINDING_DISTANCE || close {
            return p;
        }
        let from = intersection.unwrap_or(c);
        let n = normalize(sub(p, from));
        let k = if ratio > 1.0 { FIXED_BINDING_DISTANCE } else { -FIXED_BINDING_DISTANCE };
        return [from[0] + n[0] * k, from[1] + n[1] * k];
    }
    heading_to_mid_bind_point(p, e, aabb)
}

/// `snapToMid`: near the middle of a side, exactly on it.
fn snap_to_mid(e: &Element, p: Point) -> Point {
    let tolerance = 0.05;
    let (x, y, w, h) = (e.x, e.y, e.width, e.height);
    let c = [x + w / 2.0 - 0.1, y + h / 2.0 - 0.1];
    let q = rotate(p, c, -e.angle);
    let vt = (tolerance * h).clamp(5.0, 80.0);
    let ht = (tolerance * w).clamp(5.0, 80.0);
    let d = FIXED_BINDING_DISTANCE;
    let back = |pt: Point| rotate(pt, c, e.angle);
    if q[0] <= x + w / 2.0 && q[1] > c[1] - vt && q[1] < c[1] + vt {
        back([x - d, c[1]])
    } else if q[1] <= y + h / 2.0 && q[0] > c[0] - ht && q[0] < c[0] + ht {
        back([c[0], y - d])
    } else if q[0] >= x + w / 2.0 && q[1] > c[1] - vt && q[1] < c[1] + vt {
        back([x + w + d, c[1]])
    } else if q[1] >= y + h / 2.0 && q[0] > c[0] - ht && q[0] < c[0] + ht {
        back([c[0], y + h + d])
    } else {
        p
    }
}

/// `normalizeFixedPoint`: a ratio of exactly a half is nudged off it.
pub fn normalize_fixed_point(fp: [f64; 2]) -> [f64; 2] {
    if (fp[0] - 0.5).abs() < 1e-4 || (fp[1] - 0.5).abs() < 1e-4 {
        fp.map(|r| if (r - 0.5).abs() < 1e-4 { 0.5001 } else { r })
    } else {
        fp
    }
}

/// `getGlobalFixedPointForBindableElement`.
pub fn fixed_point_to_scene(fp: [f64; 2], e: &Element) -> Point {
    let [fx, fy] = normalize_fixed_point(fp);
    rotate([e.x + e.width * fx, e.y + e.height * fy], element_center(e), e.angle)
}

/// `calculateFixedPointForElbowArrowBinding`: where on `e` (as fractions
/// of its size) the end at `p` binds.
pub fn fixed_point_for(p: Point, e: &Element) -> [f64; 2] {
    let snapped = snap_to_outline(p, e);
    let mid = [e.x + e.width / 2.0, e.y + e.height / 2.0];
    let q = rotate(snapped, mid, -e.angle);
    normalize_fixed_point([(q[0] - e.x) / e.width, (q[1] - e.y) / e.height])
}

/// One end of the arrow, as routing sees it.
pub struct End<'a> {
    /// Where the end is, in the scene, as given.
    pub point: Point,
    /// The shape it is bound to, and the binding's `fixedPoint`.
    pub bound: Option<(&'a Element, [f64; 2])>,
    /// While drawing or dragging: the shape it is over.
    pub hovered: Option<&'a Element>,
    pub arrowhead: bool,
}

/// While dragging, the shape an end is over, or else the one it is bound
/// to (`getHoveredElement(…) || startElement`); otherwise the bound one.
fn effective<'a>(end: &End<'a>, dragging: bool) -> Option<&'a Element> {
    if dragging {
        end.hovered.or(end.bound.map(|b| b.0))
    } else {
        end.bound.map(|b| b.0)
    }
}

/// `getGlobalPoint`.
fn global_point(end: &End, dragging: bool) -> Point {
    if dragging {
        return match effective(end, dragging) {
            Some(h) => snap_to_mid(h, snap_to_outline(end.point, h)),
            None => end.point,
        };
    }
    if let Some((e, fp)) = end.bound {
        let fixed = fixed_point_to_scene(fp, e);
        return if (binding::distance_to(e, fixed) - FIXED_BINDING_DISTANCE).abs() > 0.01 { snap_to_outline(end.point, e) } else { fixed };
    }
    end.point
}

fn common_aabb(boxes: &[Bounds]) -> Bounds {
    [
        boxes.iter().map(|b| b[0]).fold(f64::INFINITY, f64::min),
        boxes.iter().map(|b| b[1]).fold(f64::INFINITY, f64::min),
        boxes.iter().map(|b| b[2]).fold(f64::NEG_INFINITY, f64::max),
        boxes.iter().map(|b| b[3]).fold(f64::NEG_INFINITY, f64::max),
    ]
}

/// `generateDynamicAABBs`: the boxes the route keeps out of, around each
/// end, split so they do not overlap.
#[allow(clippy::too_many_arguments)] // as Excalidraw's
fn dynamic_aabbs(a: Bounds, b: Bounds, common: Bounds, start_diff: [f64; 4], end_diff: [f64; 4], no_side_hack: bool, start_el: Option<Bounds>, end_el: Option<Bounds>) -> [Bounds; 2] {
    let s = start_el.unwrap_or(a);
    let e = end_el.unwrap_or(b);
    let [su, sr, sd, sl] = start_diff;
    let [eu, er, ed, el] = end_diff;
    let first = [
        if a[0] > b[2] {
            if a[1] > b[3] || a[3] < b[1] {
                ((s[0] + e[2]) / 2.0).min(a[0] - sl)
            } else {
                (s[0] + e[2]) / 2.0
            }
        } else if a[0] > b[0] {
            a[0] - sl
        } else {
            common[0] - sl
        },
        if a[1] > b[3] {
            if a[0] > b[2] || a[2] < b[0] {
                ((s[1] + e[3]) / 2.0).min(a[1] - su)
            } else {
                (s[1] + e[3]) / 2.0
            }
        } else if a[1] > b[1] {
            a[1] - su
        } else {
            common[1] - su
        },
        if a[2] < b[0] {
            if a[1] > b[3] || a[3] < b[1] {
                ((s[2] + e[0]) / 2.0).max(a[2] + sr)
            } else {
                (s[2] + e[0]) / 2.0
            }
        } else if a[2] < b[2] {
            a[2] + sr
        } else {
            common[2] + sr
        },
        if a[3] < b[1] {
            if a[0] > b[2] || a[2] < b[0] {
                ((s[3] + e[1]) / 2.0).max(a[3] + sd)
            } else {
                (s[3] + e[1]) / 2.0
            }
        } else if a[3] < b[3] {
            a[3] + sd
        } else {
            common[3] + sd
        },
    ];
    let second = [
        if b[0] > a[2] {
            if b[1] > a[3] || b[3] < a[1] {
                ((e[0] + s[2]) / 2.0).min(b[0] - el)
            } else {
                (e[0] + s[2]) / 2.0
            }
        } else if b[0] > a[0] {
            b[0] - el
        } else {
            common[0] - el
        },
        if b[1] > a[3] {
            if b[0] > a[2] || b[2] < a[0] {
                ((e[1] + s[3]) / 2.0).min(b[1] - eu)
            } else {
                (e[1] + s[3]) / 2.0
            }
        } else if b[1] > a[1] {
            b[1] - eu
        } else {
            common[1] - eu
        },
        if b[2] < a[0] {
            if b[1] > a[3] || b[3] < a[1] {
                ((e[2] + s[0]) / 2.0).max(b[2] + er)
            } else {
                (e[2] + s[0]) / 2.0
            }
        } else if b[2] < a[2] {
            b[2] + er
        } else {
            common[2] + er
        },
        if b[3] < a[1] {
            if b[0] > a[2] || b[2] < a[0] {
                ((e[3] + s[1]) / 2.0).max(b[3] + ed)
            } else {
                (e[3] + s[1]) / 2.0
            }
        } else if b[3] < a[3] {
            b[3] + ed
        } else {
            common[3] + ed
        },
    ];
    let c = common_aabb(&[first, second]);
    if !no_side_hack && first[2] - first[0] + second[2] - second[0] > c[2] - c[0] + 1e-11 && first[3] - first[1] + second[3] - second[1] > c[3] - c[1] + 1e-11 {
        let (ecx, ecy) = ((second[0] + second[2]) / 2.0, (second[1] + second[3]) / 2.0);
        let cross = |u: Point, v: Point| u[0] * v[1] - v[0] * u[1];
        if b[0] > a[2] && a[1] > b[3] {
            let cx = first[2] + (second[0] - first[2]) / 2.0;
            let cy = second[3] + (first[1] - second[3]) / 2.0;
            if cross([a[2] - ecx, a[1] - ecy], [a[0] - ecx, a[3] - ecy]) > 0.0 {
                return [[first[0], first[1], cx, first[3]], [cx, second[1], second[2], second[3]]];
            }
            return [[first[0], cy, first[2], first[3]], [second[0], second[1], second[2], cy]];
        } else if a[2] < b[0] && a[3] < b[1] {
            let cx = first[2] + (second[0] - first[2]) / 2.0;
            let cy = first[3] + (second[1] - first[3]) / 2.0;
            if cross([a[0] - ecx, a[1] - ecy], [a[2] - ecx, a[3] - ecy]) > 0.0 {
                return [[first[0], first[1], first[2], cy], [second[0], cy, second[2], second[3]]];
            }
            return [[first[0], first[1], cx, first[3]], [cx, second[1], second[2], second[3]]];
        } else if a[0] > b[2] && a[3] < b[1] {
            let cx = second[2] + (first[0] - second[2]) / 2.0;
            let cy = first[3] + (second[1] - first[3]) / 2.0;
            if cross([a[2] - ecx, a[1] - ecy], [a[0] - ecx, a[3] - ecy]) > 0.0 {
                return [[cx, first[1], first[2], first[3]], [second[0], second[1], cx, second[3]]];
            }
            return [[first[0], first[1], first[2], cy], [second[0], cy, second[2], second[3]]];
        } else if a[0] > b[2] && a[1] > b[3] {
            let cx = second[2] + (first[0] - second[2]) / 2.0;
            let cy = second[3] + (first[1] - second[3]) / 2.0;
            if cross([a[0] - ecx, a[1] - ecy], [a[2] - ecx, a[3] - ecy]) > 0.0 {
                return [[cx, first[1], first[2], first[3]], [second[0], second[1], cx, second[3]]];
            }
            return [[first[0], cy, first[2], first[3]], [second[0], second[1], second[2], cy]];
        }
    }
    [first, second]
}

/// `getDonglePosition`: where the route leaves its box.
fn dongle(b: Bounds, h: Heading, p: Point) -> Point {
    match h {
        Up => [p[0], b[1]],
        Right => [b[2], p[1]],
        Down => [p[0], b[3]],
        Left => [b[0], p[1]],
    }
}

struct Node {
    f: f64,
    g: f64,
    closed: bool,
    visited: bool,
    parent: Option<usize>,
    addr: [i64; 2],
    pos: Point,
}

struct Grid {
    rows: i64,
    cols: i64,
    data: Vec<Node>,
}

impl Grid {
    fn at(&self, [col, row]: [i64; 2]) -> Option<usize> {
        if col < 0 || col >= self.cols || row < 0 || row >= self.rows {
            return None;
        }
        Some((row * self.cols + col) as usize)
    }

    /// `pointToGridNode`: the node exactly at `p`, by column then row.
    fn node_at(&self, p: Point) -> Option<usize> {
        for col in 0..self.cols {
            for row in 0..self.rows {
                if let Some(i) = self.at([col, row]) {
                    if self.data[i].pos == p {
                        return Some(i);
                    }
                }
            }
        }
        None
    }
}

/// JavaScript's `Set` then sort: insertion order kept for equal values.
fn sorted_unique(values: Vec<f64>) -> Vec<f64> {
    let mut out: Vec<f64> = Vec::new();
    for v in values {
        if !out.iter().any(|o| *o == v || (o.is_nan() && v.is_nan())) {
            out.push(v);
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// `calculateGrid`.
fn calculate_grid(aabbs: &[Bounds; 2], start: Point, sh: Heading, end: Point, eh: Heading, common: Bounds) -> Grid {
    let mut horizontal = Vec::new();
    let mut vertical = Vec::new();
    if matches!(sh, Left | Right) {
        vertical.push(start[1]);
    } else {
        horizontal.push(start[0]);
    }
    if matches!(eh, Left | Right) {
        vertical.push(end[1]);
    } else {
        horizontal.push(end[0]);
    }
    for b in aabbs {
        horizontal.push(b[0]);
        horizontal.push(b[2]);
        vertical.push(b[1]);
        vertical.push(b[3]);
    }
    horizontal.push(common[0]);
    horizontal.push(common[2]);
    vertical.push(common[1]);
    vertical.push(common[3]);
    let ys = sorted_unique(vertical);
    let xs = sorted_unique(horizontal);
    let mut data = Vec::with_capacity(xs.len() * ys.len());
    for (row, y) in ys.iter().enumerate() {
        for (col, x) in xs.iter().enumerate() {
            data.push(Node { f: 0.0, g: 0.0, closed: false, visited: false, parent: None, addr: [col as i64, row as i64], pos: [*x, *y] });
        }
    }
    Grid { rows: ys.len() as i64, cols: xs.len() as i64, data }
}

/// `estimateSegmentCount`.
fn estimate_segments(start: Point, end: Point, sh: Heading, eh: Heading) -> f64 {
    let (s, e) = (start, end);
    match (eh, sh) {
        (Right, Right) => {
            if s[0] >= e[0] {
                4.0
            } else if s[1] == e[1] {
                0.0
            } else {
                2.0
            }
        }
        (Right, Up) => if s[1] > e[1] && s[0] < e[0] { 1.0 } else { 3.0 },
        (Right, Down) => if s[1] < e[1] && s[0] < e[0] { 1.0 } else { 3.0 },
        (Right, Left) => if s[1] == e[1] { 4.0 } else { 2.0 },
        (Left, Right) => if s[1] == e[1] { 4.0 } else { 2.0 },
        (Left, Up) => if s[1] > e[1] && s[0] > e[0] { 1.0 } else { 3.0 },
        (Left, Down) => if s[1] < e[1] && s[0] > e[0] { 1.0 } else { 3.0 },
        (Left, Left) => {
            if s[0] <= e[0] {
                4.0
            } else if s[1] == e[1] {
                0.0
            } else {
                2.0
            }
        }
        (Up, Right) => if s[1] > e[1] && s[0] < e[0] { 1.0 } else { 3.0 },
        (Up, Up) => {
            if s[1] >= e[1] {
                4.0
            } else if s[0] == e[0] {
                0.0
            } else {
                2.0
            }
        }
        (Up, Down) => if s[0] == e[0] { 4.0 } else { 2.0 },
        (Up, Left) => if s[1] > e[1] && s[0] > e[0] { 1.0 } else { 3.0 },
        (Down, Right) => if s[1] < e[1] && s[0] < e[0] { 1.0 } else { 3.0 },
        (Down, Up) => if s[0] == e[0] { 4.0 } else { 2.0 },
        (Down, Down) => {
            if s[1] <= e[1] {
                4.0
            } else if s[0] == e[0] {
                0.0
            } else {
                2.0
            }
        }
        (Down, Left) => if s[1] < e[1] && s[0] > e[0] { 1.0 } else { 3.0 },
    }
}

/// Excalidraw's `BinaryHeap`, on node indices, ordered by `f`, ties as it
/// breaks them.
struct Heap(Vec<usize>);

impl Heap {
    fn sink_down(&mut self, nodes: &[Node], mut idx: usize) {
        let node = self.0[idx];
        while idx > 0 {
            let parent_n = ((idx + 1) >> 1) - 1;
            let parent = self.0[parent_n];
            if nodes[node].f < nodes[parent].f {
                self.0[parent_n] = node;
                self.0[idx] = parent;
                idx = parent_n;
            } else {
                break;
            }
        }
    }

    fn bubble_up(&mut self, nodes: &[Node], mut idx: usize) {
        let len = self.0.len();
        let node = self.0[idx];
        let score = nodes[node].f;
        loop {
            let child2 = (idx + 1) << 1;
            let child1 = child2 - 1;
            let mut swap = None;
            let mut child1_score = 0.0;
            if child1 < len {
                child1_score = nodes[self.0[child1]].f;
                if child1_score < score {
                    swap = Some(child1);
                }
            }
            if child2 < len {
                let child2_score = nodes[self.0[child2]].f;
                if child2_score < if swap.is_none() { score } else { child1_score } {
                    swap = Some(child2);
                }
            }
            match swap {
                Some(s) => {
                    self.0[idx] = self.0[s];
                    self.0[s] = node;
                    idx = s;
                }
                None => break,
            }
        }
    }

    fn push(&mut self, nodes: &[Node], n: usize) {
        self.0.push(n);
        let last = self.0.len() - 1;
        self.sink_down(nodes, last);
    }

    fn pop(&mut self, nodes: &[Node]) -> Option<usize> {
        if self.0.is_empty() {
            return None;
        }
        let result = self.0[0];
        let end = self.0.pop()?;
        if !self.0.is_empty() {
            self.0[0] = end;
            self.bubble_up(nodes, 0);
        }
        Some(result)
    }

    fn rescore(&mut self, nodes: &[Node], n: usize) {
        if let Some(i) = self.0.iter().position(|x| *x == n) {
            self.sink_down(nodes, i);
        }
    }
}

fn neighbor_heading(i: usize) -> Heading {
    match i {
        0 => Up,
        1 => Right,
        2 => Down,
        _ => Left,
    }
}

/// `astar`: the cheapest route, a bend costing the cube of the distance.
fn astar(grid: &mut Grid, start: usize, end: usize, sh: Heading, eh: Heading, aabbs: &[Bounds]) -> Option<Vec<usize>> {
    let bend = m_dist(grid.data[start].pos, grid.data[end].pos);
    let mut open = Heap(Vec::new());
    open.push(&grid.data, start);
    while let Some(current) = open.pop(&grid.data) {
        if grid.data[current].closed {
            continue;
        }
        if current == end {
            let mut path = Vec::new();
            let mut c = current;
            while let Some(p) = grid.data[c].parent {
                path.insert(0, c);
                c = p;
            }
            path.insert(0, start);
            return Some(path);
        }
        grid.data[current].closed = true;
        let [col, row] = grid.data[current].addr;
        let neighbors = [grid.at([col, row - 1]), grid.at([col + 1, row]), grid.at([col, row + 1]), grid.at([col - 1, row])];
        for (i, neighbor) in neighbors.into_iter().enumerate() {
            let Some(n) = neighbor else { continue };
            if grid.data[n].closed {
                continue;
            }
            let half = scale_from(grid.data[n].pos, grid.data[current].pos, 0.5);
            if aabbs.iter().any(|b| inside_bounds(half, *b)) {
                continue;
            }
            let heading = neighbor_heading(i);
            let previous = match grid.data[current].parent {
                Some(p) => vector_to_heading(sub(grid.data[current].pos, grid.data[p].pos)),
                None => sh,
            };
            let reverse = previous.flip() == heading
                || (grid.data[start].addr == grid.data[n].addr && heading == sh)
                || (grid.data[end].addr == grid.data[n].addr && heading == eh);
            if reverse {
                continue;
            }
            let change = previous != heading;
            let g = grid.data[current].g + m_dist(grid.data[n].pos, grid.data[current].pos) + if change { bend.powi(3) } else { 0.0 };
            let visited = grid.data[n].visited;
            if !visited || g < grid.data[n].g {
                let est = estimate_segments(grid.data[n].pos, grid.data[end].pos, heading, eh);
                let h = m_dist(grid.data[end].pos, grid.data[n].pos) + est * bend.powi(2);
                let node = &mut grid.data[n];
                node.visited = true;
                node.parent = Some(current);
                node.g = g;
                node.f = g + h;
                if !visited {
                    open.push(&grid.data, n);
                } else {
                    open.rescore(&grid.data, n);
                }
            }
        }
    }
    None
}

/// `getElbowArrowCornerPoints`: only the corners.
fn corner_points(points: Vec<Point>) -> Vec<Point> {
    if points.len() <= 1 {
        return points;
    }
    let mut prev_h = (points[0][1] - points[1][1]).abs() < (points[0][0] - points[1][0]).abs();
    let last = points.len() - 1;
    let mut out = Vec::new();
    for (i, p) in points.iter().enumerate() {
        if i == 0 || i == last {
            out.push(*p);
            continue;
        }
        let next = points[i + 1];
        let next_h = (p[1] - next[1]).abs() < (p[0] - next[0]).abs();
        let keep = prev_h != next_h;
        prev_h = next_h;
        if keep {
            out.push(*p);
        }
    }
    out
}

/// `removeElbowArrowShortSegments`.
fn remove_short_segments(points: Vec<Point>) -> Vec<Point> {
    if points.len() < 4 {
        return points;
    }
    let last = points.len() - 1;
    (0..points.len()).filter(|&i| i == 0 || i == last || dist(points[i - 1], points[i]) > DEDUP_THRESHOLD).map(|i| points[i]).collect()
}

/// `normalizeArrowElementUpdate`: scene points as the arrow's `x`, `y`
/// and its points from there.
fn normalize_update(global: &[Point]) -> (f64, f64, Vec<Point>) {
    let [ox, oy] = global[0];
    let points = global.iter().map(|p| [(p[0] - ox).clamp(-MAX_POS, MAX_POS), (p[1] - oy).clamp(-MAX_POS, MAX_POS)]).collect();
    (ox.clamp(-MAX_POS, MAX_POS), oy.clamp(-MAX_POS, MAX_POS), points)
}

/// `validateElbowPoints`.
pub fn valid_elbow_points(points: &[Point]) -> bool {
    points.windows(2).all(|w| (w[1][0] - w[0][0]).abs() < DEDUP_THRESHOLD || (w[1][1] - w[0][1]).abs() < DEDUP_THRESHOLD)
}

/// `updateElbowArrowPoints` with no fixed segments: the route between the
/// two ends, as the arrow's new `x`, `y` and points. `dragging` while
/// drawing or dragging an end, when hovered shapes count.
pub fn route(start: &End, end: &End, dragging: bool) -> Option<(f64, f64, Vec<Point>)> {
    let sg = global_point(start, dragging);
    let eg = global_point(end, dragging);
    let start_el = effective(start, dragging);
    let end_el = effective(end, dragging);
    let sh = bind_point_heading(sg, eg, start_el, start.point);
    let eh = bind_point_heading(eg, sg, end_el, end.point);
    let start_point_bounds = [sg[0] - 2.0, sg[1] - 2.0, sg[0] + 2.0, sg[1] + 2.0];
    let end_point_bounds = [eg[0] - 2.0, eg[1] - 2.0, eg[0] + 2.0, eg[1] + 2.0];
    let head = |arrowhead: bool| if arrowhead { FIXED_BINDING_DISTANCE * 6.0 } else { FIXED_BINDING_DISTANCE * 2.0 };
    let start_bounds = start_el.map_or(start_point_bounds, |e| aabb_for_element(e, Some(offset_from_heading(sh, head(start.arrowhead), 1.0))));
    let end_bounds = end_el.map_or(end_point_bounds, |e| aabb_for_element(e, Some(offset_from_heading(eh, head(end.arrowhead), 1.0))));
    let overlap = inside_bounds(sg, end_el.map_or(end_point_bounds, |e| aabb_for_element(e, Some(offset_from_heading(eh, BASE_PADDING, BASE_PADDING)))))
        || inside_bounds(eg, start_el.map_or(start_point_bounds, |e| aabb_for_element(e, Some(offset_from_heading(sh, BASE_PADDING, BASE_PADDING)))));
    let common = common_aabb(&if overlap { [start_point_bounds, end_point_bounds] } else { [start_bounds, end_bounds] });
    let none_hovered = start_el.is_none() && end_el.is_none();
    let diff = |h: Heading, arrowhead: bool| {
        if overlap {
            offset_from_heading(h, if none_hovered { 0.0 } else { BASE_PADDING }, 0.0)
        } else {
            offset_from_heading(h, if none_hovered { 0.0 } else { BASE_PADDING - head(arrowhead) }, BASE_PADDING)
        }
    };
    let aabbs = dynamic_aabbs(
        if overlap { start_point_bounds } else { start_bounds },
        if overlap { end_point_bounds } else { end_bounds },
        common,
        diff(sh, start.arrowhead),
        diff(eh, end.arrowhead),
        overlap,
        start_el.map(|e| aabb_for_element(e, None)),
        end_el.map(|e| aabb_for_element(e, None)),
    );
    let start_dongle_pos = dongle(aabbs[0], sh, sg);
    let end_dongle_pos = dongle(aabbs[1], eh, eg);

    // `routeElbowArrow`.
    let mut grid = calculate_grid(&aabbs, start_dongle_pos, sh, end_dongle_pos, eh, common);
    let start_dongle = grid.node_at(start_dongle_pos);
    let end_dongle = grid.node_at(end_dongle_pos);
    let end_node = grid.node_at(eg);
    if let (Some(n), Some(_)) = (end_node, end_el) {
        grid.data[n].closed = true;
    }
    let start_node = grid.node_at(sg);
    if let (Some(n), Some(_)) = (start_node, start.bound) {
        grid.data[n].closed = true;
    }
    let dongle_overlap = match (start_dongle, end_dongle) {
        (Some(s), Some(e)) => inside_bounds(grid.data[s].pos, aabbs[1]) || inside_bounds(grid.data[e].pos, aabbs[0]),
        _ => false,
    };
    let from = start_dongle.or(start_node)?;
    let to = end_dongle.or(end_node)?;
    let path = astar(&mut grid, from, to, sh, eh, if dongle_overlap { &[] } else { &aabbs });
    let mut points: Vec<Point> = match path {
        Some(p) => p.iter().map(|n| grid.data[*n].pos).collect(),
        None => Vec::new(),
    };
    if !points.is_empty() {
        if start_dongle.is_some() {
            points.insert(0, sg);
        }
        if end_dongle.is_some() {
            points.push(eg);
        }
    }
    let points = corner_points(remove_short_segments(points));
    if points.is_empty() {
        return None;
    }
    Some(normalize_update(&points))
}
