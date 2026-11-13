//! Arrows bound to shapes, as Excalidraw 0.18.1 binds them
//! (`element/binding.ts`).
//!
//! A binding is `{ elementId, focus, gap }` on the arrow and
//! `{ id, type: "arrow" }` in the shape's `boundElements`, so files written
//! here bind in Excalidraw too. `focus` (where along the shape the arrow
//! aims, -1 to 1) and the end point's recomputation follow
//! `determineFocusDistance`, `determineFocusPoint` and `updateBoundPoint`
//! line by line. Where those intersect a line with a shape's outline grown
//! by the gap, this uses the outline as a polygon (64 points for an
//! ellipse) where Excalidraw uses curves: the end lands within a fraction of
//! a pixel of where Excalidraw puts it, and Excalidraw recomputes it the
//! next time it moves anything.

use crate::geometry;
use rough::Point;
use scene::{Element, Kind};

/// `FIXED_BINDING_DISTANCE`, `BINDING_HIGHLIGHT_THICKNESS`, `_OFFSET`.
const BINDING_HIGHLIGHT_THICKNESS: f64 = 10.0;
const BINDING_HIGHLIGHT_OFFSET: f64 = 4.0;

/// `isBindableElement`.
pub fn is_bindable(e: &Element) -> bool {
    !e.is_deleted
        && match e.kind {
            Kind::Rectangle
            | Kind::Diamond
            | Kind::Ellipse
            | Kind::Image
            | Kind::Iframe
            | Kind::Embeddable
            | Kind::Frame
            | Kind::MagicFrame => true,
            Kind::Text => e.text.as_ref().is_some_and(|t| t.container_id.is_none()),
            _ => false,
        }
}

/// `maxBindingGap`: how far outside its outline a shape still catches an
/// arrow's end.
pub fn max_binding_gap(e: &Element, zoom: f64) -> f64 {
    let zoom_value = if zoom < 1.0 { zoom } else { 1.0 };
    let shape_ratio = if e.kind == Kind::Diamond {
        1.0 / 2f64.sqrt()
    } else {
        1.0
    };
    let smaller = shape_ratio * e.width.min(e.height);
    16f64
        .max((0.25 * smaller).min(32.0))
        .max(BINDING_HIGHLIGHT_THICKNESS / zoom_value + BINDING_HIGHLIGHT_OFFSET)
}

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}
fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1]]
}
fn scale(a: Point, s: f64) -> Point {
    [a[0] * s, a[1] * s]
}
fn cross(a: Point, b: Point) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn norm(a: Point) -> Point {
    let l = rough::jsmath::hypot(&[a[0], a[1]]);
    if l == 0.0 {
        [0.0, 0.0]
    } else {
        [a[0] / l, a[1] / l]
    }
}
fn dist(a: Point, b: Point) -> f64 {
    rough::jsmath::hypot(&[b[0] - a[0], b[1] - a[1]])
}
fn dist_sq(a: Point, b: Point) -> f64 {
    (b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)
}
/// `Math.sign`: 0 for 0, where `f64::signum` gives 1.
fn js_sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

/// `pointRotateRads`.
fn rotate(p: Point, c: Point, angle: f64) -> Point {
    geometry::rotate(p, c, angle)
}

/// `lineSegmentIntersectionPoints`: where two segments cross, if they do.
fn segments_cross(a: (Point, Point), b: (Point, Point)) -> Option<Point> {
    let r = sub(a.1, a.0);
    let s = sub(b.1, b.0);
    let denom = cross(r, s);
    if denom == 0.0 {
        return None;
    }
    let t = cross(sub(b.0, a.0), s) / denom;
    let u = cross(sub(b.0, a.0), r) / denom;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| add(a.0, scale(r, t)))
}

fn center(e: &Element) -> Point {
    [e.x + e.width / 2.0, e.y + e.height / 2.0]
}

/// `determineFocusDistance`: where, across the shape, the line from `a`
/// through `b` aims: 0 at its centre, ±1 at its edge.
pub fn focus_distance(e: &Element, a: Point, b: Point) -> f64 {
    let c = center(e);
    if a == b {
        return 0.0;
    }
    let ra = rotate(a, c, -e.angle);
    let rb = rotate(b, c, -e.angle);
    let sign = -js_sign(cross(sub(rb, a), sub(rb, c)));
    let interceptor = (
        rb,
        add(
            rb,
            scale(norm(sub(rb, ra)), (e.width * 2.0).max(e.height * 2.0)),
        ),
    );
    let (x, y, w, h) = (e.x, e.y, e.width, e.height);
    let diamond = e.kind == Kind::Diamond;
    let axes = if diamond {
        [
            ([x + w / 2.0, y], [x + w / 2.0, y + h]),
            ([x, y + h / 2.0], [x + w, y + h / 2.0]),
        ]
    } else {
        [([x, y], [x + w, y + h]), ([x + w, y], [x, y + h])]
    };
    let interceptees = if diamond {
        [
            ([x + w / 2.0, y - h], [x + w / 2.0, y + h * 2.0]),
            ([x - w, y + h / 2.0], [x + w * 2.0, y + h / 2.0]),
        ]
    } else {
        [
            ([x - w, y - h], [x + w * 2.0, y + h * 2.0]),
            ([x + w * 2.0, y - h], [x - w, y + h * 2.0]),
        ]
    };
    let mut hits: Vec<Point> = interceptees
        .iter()
        .filter_map(|i| segments_cross(interceptor, *i))
        .collect();
    hits.sort_by(|g, h| dist_sq(*g, b).total_cmp(&dist_sq(*h, b)));
    // As in the original, the axis is picked by position in the sorted list.
    let mut ratios: Vec<f64> = hits
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let half = if diamond {
                dist(axes[i].0, axes[i].1) / 2.0
            } else {
                (w * w + h * h).sqrt() / 2.0
            };
            sign * dist(c, *p) / half
        })
        .collect();
    ratios.sort_by(|g, h| g.abs().total_cmp(&h.abs()));
    ratios.first().copied().unwrap_or(0.0)
}

/// `determineFocusPoint`: the point the arrow aims at, for `focus`, as seen
/// from `adjacent`.
pub fn focus_point(e: &Element, focus: f64, adjacent: Point) -> Point {
    let c = center(e);
    if focus == 0.0 {
        return c;
    }
    let (x, y, w, h) = (e.x, e.y, e.width, e.height);
    let corners = if e.kind == Kind::Diamond {
        [
            [x, y + h / 2.0],
            [x + w / 2.0, y],
            [x + w, y + h / 2.0],
            [x + w / 2.0, y + h],
        ]
    } else {
        [[x, y], [x + w, y], [x + w, y + h], [x, y + h]]
    };
    let cand = corners.map(|p| rotate(add(scale(sub(p, c), focus.abs()), c), c, e.angle));
    let side = |i: usize, j: usize| cross(sub(adjacent, cand[i]), sub(cand[j], cand[i]));
    let pos = focus > 0.0;
    let selected = [
        side(0, 1) > 0.0
            && if pos {
                side(1, 2) < 0.0
            } else {
                side(3, 0) < 0.0
            },
        side(1, 2) > 0.0
            && if pos {
                side(2, 3) < 0.0
            } else {
                side(0, 1) < 0.0
            },
        side(2, 3) > 0.0
            && if pos {
                side(3, 0) < 0.0
            } else {
                side(1, 2) < 0.0
            },
    ];
    if selected[0] {
        if pos {
            cand[1]
        } else {
            cand[0]
        }
    } else if selected[1] {
        if pos {
            cand[2]
        } else {
            cand[1]
        }
    } else if selected[2] {
        if pos {
            cand[3]
        } else {
            cand[2]
        }
    } else if pos {
        cand[0]
    } else {
        cand[3]
    }
}

/// The outline as a closed polygon in the scene, grown outward by `offset`.
fn outline(e: &Element, offset: f64) -> Vec<Point> {
    let (w, h) = (e.width, e.height);
    let local: Vec<Point> = match e.kind {
        Kind::Ellipse => {
            let (rx, ry) = (w / 2.0 + offset, h / 2.0 + offset);
            (0..64)
                .map(|i| {
                    let a = f64::from(i) / 64.0 * std::f64::consts::TAU;
                    [w / 2.0 + rx * a.cos(), h / 2.0 + ry * a.sin()]
                })
                .collect()
        }
        Kind::Diamond => {
            let d = render::shape::diamond_points(e);
            let pts = [[d[0], d[1]], [d[2], d[3]], [d[4], d[5]], [d[6], d[7]]];
            grow(&pts, offset)
        }
        _ => vec![
            [-offset, -offset],
            [w + offset, -offset],
            [w + offset, h + offset],
            [-offset, h + offset],
        ],
    };
    local
        .into_iter()
        .map(|p| geometry::to_scene(e, p))
        .collect()
}

/// A convex polygon with each edge moved out by `offset` (mitred corners).
fn grow(pts: &[Point], offset: f64) -> Vec<Point> {
    if offset == 0.0 {
        return pts.to_vec();
    }
    let n = pts.len();
    let c = pts.iter().fold([0.0, 0.0], |s, p| {
        [s[0] + p[0] / n as f64, s[1] + p[1] / n as f64]
    });
    (0..n)
        .map(|i| {
            let (prev, p, next) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
            // The edge's normal that points away from the middle.
            let outward = |a: Point, b: Point| {
                let d = norm(sub(b, a));
                let nrm = [d[1], -d[0]];
                let mid = scale(add(a, b), 0.5);
                if nrm[0] * (mid[0] - c[0]) + nrm[1] * (mid[1] - c[1]) < 0.0 {
                    [-nrm[0], -nrm[1]]
                } else {
                    nrm
                }
            };
            let (n1, n2) = (outward(prev, p), outward(p, next));
            let m = norm(add(n1, n2));
            let cos = m[0] * n1[0] + m[1] * n1[1];
            add(p, scale(m, offset / cos.max(0.1)))
        })
        .collect()
}

/// `distanceToBindableElement`: how far `p` is from the outline.
pub fn distance_to(e: &Element, p: Point) -> f64 {
    let poly = outline(e, 0.0);
    (0..poly.len())
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            let ab = sub(b, a);
            let len2 = ab[0] * ab[0] + ab[1] * ab[1];
            let t = if len2 == 0.0 {
                0.0
            } else {
                ((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / len2
            }
            .clamp(0.0, 1.0);
            dist(p, add(a, scale(ab, t)))
        })
        .fold(f64::INFINITY, f64::min)
}

fn inside(e: &Element, p: Point) -> bool {
    let poly = outline(e, 0.0);
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// `intersectElementWithLineSegment` with `offset`: where the segment
/// crosses the outline grown by `offset`.
fn intersect(e: &Element, seg: (Point, Point), offset: f64) -> Vec<Point> {
    let poly = outline(e, offset);
    (0..poly.len())
        .filter_map(|i| segments_cross(seg, (poly[i], poly[(i + 1) % poly.len()])))
        .collect()
}

/// Where a segment crosses `e`'s outline.
pub fn intersect_outline(e: &Element, seg: (Point, Point)) -> Vec<Point> {
    intersect(e, seg, 0.0)
}

/// Whether an arrow end at `p` binds to `e`: inside it, or within its
/// binding gap of the outline.
pub fn catches(e: &Element, p: Point, zoom: f64) -> bool {
    is_bindable(e) && (inside(e, p) || distance_to(e, p) <= max_binding_gap(e, zoom))
}

/// `calculateFocusAndGap`.
pub fn focus_and_gap(arrow: &Element, start: bool, e: &Element) -> Option<(f64, f64)> {
    let pts = &arrow.linear.as_ref()?.points;
    if pts.len() < 2 {
        return None;
    }
    let (edge, adjacent) = if start {
        (0, 1)
    } else {
        (pts.len() - 1, pts.len() - 2)
    };
    let edge = geometry::to_scene(arrow, pts[edge]);
    let adjacent = geometry::to_scene(arrow, pts[adjacent]);
    Some((
        focus_distance(e, adjacent, edge),
        distance_to(e, edge).max(1.0),
    ))
}

/// `updateBoundPoint`: where the arrow's end goes, in the scene, for its
/// binding to `e` with `focus` and `gap`.
pub fn bound_point(
    arrow: &Element,
    start: bool,
    focus: f64,
    gap: f64,
    e: &Element,
) -> Option<Point> {
    let pts = &arrow.linear.as_ref()?.points;
    if pts.len() < 2 {
        return None;
    }
    let (edge_i, adjacent_i) = if start {
        (0, 1)
    } else {
        (pts.len() - 1, pts.len() - 2)
    };
    let adjacent = geometry::to_scene(arrow, pts[adjacent_i]);
    let focus_abs = focus_point(e, focus, adjacent);
    if gap == 0.0 {
        return Some(focus_abs);
    }
    let edge = geometry::to_scene(arrow, pts[edge_i]);
    let c = center(e);
    let length = dist(adjacent, edge) + dist(adjacent, c) + e.width.max(e.height) * 2.0;
    let mut hits = intersect(
        e,
        (
            adjacent,
            add(scale(norm(sub(focus_abs, adjacent)), length), adjacent),
        ),
        gap,
    );
    hits.sort_by(|g, h| dist_sq(*g, adjacent).total_cmp(&dist_sq(*h, adjacent)));
    Some(match hits.len() {
        0 => edge,
        1 => focus_abs,
        _ => hits[0],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use scene::Scene;

    fn rect() -> Scene {
        Scene::parse(r#"{"type":"excalidraw","elements":[{"id":"r","type":"rectangle","x":100,"y":0,"width":100,"height":100}]}"#).unwrap()
    }

    #[test]
    fn aiming_at_the_centre_is_focus_zero() {
        let s = rect();
        let r = &s.elements[0];
        assert!(focus_distance(r, [0.0, 50.0], [95.0, 50.0]).abs() < 1e-12);
        let f = focus_distance(r, [0.0, 20.0], [95.0, 20.0]);
        assert!(f != 0.0 && f.abs() < 1.0, "{f}");
        assert_eq!(focus_point(r, 0.0, [0.0, 50.0]), [150.0, 50.0]);
    }

    #[test]
    fn the_end_lands_on_the_outline_plus_the_gap() {
        let s = Scene::parse(
            r#"{"type":"excalidraw","elements":[
                {"id":"r","type":"rectangle","x":100,"y":0,"width":100,"height":100},
                {"id":"a","type":"arrow","x":0,"y":50,"points":[[0,0],[60,0]]}]}"#,
        )
        .unwrap();
        let (r, a) = (&s.elements[0], &s.elements[1]);
        let p = bound_point(a, false, 0.0, 5.0, r).unwrap();
        assert!(
            (p[0] - 95.0).abs() < 1e-9 && (p[1] - 50.0).abs() < 1e-9,
            "{p:?}"
        );
        assert!(catches(r, [90.0, 50.0], 1.0));
        assert!(!catches(r, [40.0, 50.0], 1.0));
    }
}
