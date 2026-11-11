//! Where elements are, and what a pointer is on.

use render::color::is_transparent;
use rough::Point;
use scene::{Element, Kind};

/// An axis-aligned box: min x, min y, max x, max y.
pub type Bounds = [f64; 4];

pub fn union(a: Bounds, b: Bounds) -> Bounds {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

pub fn contains(b: Bounds, p: Point) -> bool {
    p[0] >= b[0] && p[0] <= b[2] && p[1] >= b[1] && p[1] <= b[3]
}

pub fn center(b: Bounds) -> Point {
    [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]
}

pub fn rotate(p: Point, c: Point, angle: f64) -> Point {
    let (s, co) = angle.sin_cos();
    let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
    [c[0] + dx * co - dy * s, c[1] + dx * s + dy * co]
}

/// The element's box in its own coordinates (before rotation), relative to
/// `x, y`: points' extent for lines and freehand, the size otherwise.
pub fn local_box(e: &Element) -> Bounds {
    let points = e
        .linear
        .as_ref()
        .map(|l| &l.points)
        .or(e.free_draw.as_ref().map(|f| &f.points));
    match points {
        Some(pts) if !pts.is_empty() => pts.iter().fold(
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
        ),
        _ => [0.0, 0.0, e.width, e.height],
    }
}

/// The scene point the element rotates about: the centre of its box.
pub fn rotation_center(e: &Element) -> Point {
    let b = local_box(e);
    [e.x + (b[0] + b[2]) / 2.0, e.y + (b[1] + b[3]) / 2.0]
}

/// A local point (relative to `x, y`) in the scene, rotation applied.
pub fn to_scene(e: &Element, p: Point) -> Point {
    rotate([e.x + p[0], e.y + p[1]], rotation_center(e), e.angle)
}

/// A scene point in the element's local coordinates.
pub fn to_local(e: &Element, p: Point) -> Point {
    let q = rotate(p, rotation_center(e), -e.angle);
    [q[0] - e.x, q[1] - e.y]
}

/// The four corners of the element's box, in the scene.
pub fn corners(e: &Element) -> [Point; 4] {
    let b = local_box(e);
    [[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]].map(|p| to_scene(e, p))
}

/// The axis-aligned box around the rotated element.
pub fn bounds(e: &Element) -> Bounds {
    corners(e).iter().fold(
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

pub fn bounds_of<'a>(elements: impl IntoIterator<Item = &'a Element>) -> Option<Bounds> {
    elements.into_iter().map(bounds).reduce(union)
}

fn dist_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (a[0] + t * dx, a[1] + t * dy);
    ((p[0] - cx).powi(2) + (p[1] - cy).powi(2)).sqrt()
}

fn dist_to_polyline(p: Point, pts: &[Point], closed: bool) -> f64 {
    let mut d = f64::INFINITY;
    for w in pts.windows(2) {
        d = d.min(dist_to_segment(p, w[0], w[1]));
    }
    if closed && pts.len() > 2 {
        d = d.min(dist_to_segment(p, pts[pts.len() - 1], pts[0]));
    }
    if pts.len() == 1 {
        d = ((p[0] - pts[0][0]).powi(2) + (p[1] - pts[0][1]).powi(2)).sqrt();
    }
    d
}

/// Even-odd point in polygon.
fn inside_polygon(p: Point, pts: &[Point]) -> bool {
    let mut inside = false;
    let mut j = pts.len().wrapping_sub(1);
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[j]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// The outline as a polygon or polyline in local coordinates: what is hit.
fn outline(e: &Element) -> (Vec<Point>, bool) {
    let (w, h) = (e.width, e.height);
    match e.kind {
        Kind::Ellipse => {
            let (rx, ry) = (w / 2.0, h / 2.0);
            (
                (0..64)
                    .map(|i| {
                        let a = f64::from(i) / 64.0 * std::f64::consts::TAU;
                        [rx + rx * a.cos(), ry + ry * a.sin()]
                    })
                    .collect(),
                true,
            )
        }
        Kind::Diamond => {
            let d = render::shape::diamond_points(e);
            (
                vec![[d[0], d[1]], [d[2], d[3]], [d[4], d[5]], [d[6], d[7]]],
                true,
            )
        }
        Kind::Line | Kind::Arrow => {
            let Some(l) = &e.linear else {
                return (Vec::new(), false);
            };
            // A curved line is hit along the curve it draws, not its corners.
            let pts = if e.roundness.is_some() && l.points.len() >= 3 {
                rough::points::curve_to_bezier(&l.points, 0.0)
                    .map(|b| rough::points::points_on_bezier_curves(&b, 0.5, None))
                    .unwrap_or_else(|| l.points.clone())
            } else {
                l.points.clone()
            };
            (pts, false)
        }
        Kind::FreeDraw => (
            e.free_draw
                .as_ref()
                .map(|f| f.points.clone())
                .unwrap_or_default(),
            false,
        ),
        _ => (vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]], true),
    }
}

/// `shouldTestInside`: whether a click inside the shape, not just on its
/// outline, picks it.
fn test_inside(e: &Element) -> bool {
    let filled = !is_transparent(&e.background_color)
        || matches!(e.kind, Kind::Iframe | Kind::Embeddable | Kind::Text);
    match e.kind {
        Kind::Arrow => false,
        Kind::Line => {
            filled
                && e.linear
                    .as_ref()
                    .is_some_and(|l| render::shape::is_path_a_loop(&l.points))
        }
        Kind::FreeDraw => {
            filled
                && e.free_draw
                    .as_ref()
                    .is_some_and(|f| render::shape::is_path_a_loop(&f.points))
        }
        Kind::Image => true,
        // Frames are picked by their border; clicks inside go to what they hold.
        Kind::Frame | Kind::MagicFrame => false,
        _ => filled,
    }
}

/// Whether scene point `p` is on the element, within `threshold` scene units
/// of its outline (Excalidraw: 10 screen pixels).
pub fn hit(e: &Element, p: Point, threshold: f64) -> bool {
    if e.is_deleted || e.kind == Kind::Unknown {
        return false;
    }
    let q = to_local(e, p);
    let (pts, closed) = outline(e);
    if pts.is_empty() {
        return false;
    }
    let reach = threshold + e.stroke_width / 2.0;
    if test_inside(e) && closed && inside_polygon(q, &pts) {
        return true;
    }
    if test_inside(e) && !closed && pts.len() > 2 && inside_polygon(q, &pts) {
        return true;
    }
    if e.kind == Kind::FreeDraw {
        // The stroke is thick: its width is part of what is drawn.
        return dist_to_polyline(q, &pts, false) <= reach + e.stroke_width * 2.0;
    }
    dist_to_polyline(q, &pts, closed) <= reach
}

#[cfg(test)]
mod tests {
    use super::*;
    use scene::Scene;

    fn one(json: &str) -> Scene {
        Scene::parse(&format!(r#"{{"type":"excalidraw","elements":[{json}]}}"#)).unwrap()
    }

    #[test]
    fn transparent_shapes_are_hit_on_their_outline_only() {
        let s = one(
            r#"{"id":"r","type":"rectangle","x":0,"y":0,"width":100,"height":50,"strokeWidth":2}"#,
        );
        let r = &s.elements[0];
        assert!(hit(r, [1.0, 25.0], 5.0));
        assert!(
            !hit(r, [50.0, 25.0], 5.0),
            "the middle of a transparent box is empty"
        );
        let s = one(
            r##"{"id":"r","type":"rectangle","x":0,"y":0,"width":100,"height":50,"backgroundColor":"#ff0000"}"##,
        );
        assert!(
            hit(&s.elements[0], [50.0, 25.0], 5.0),
            "a filled one is hit anywhere inside"
        );
    }

    #[test]
    fn rotation_is_respected() {
        let s = one(
            r##"{"id":"r","type":"rectangle","x":0,"y":0,"width":100,"height":10,"angle":1.5707963267948966,"backgroundColor":"#000"}"##,
        );
        let r = &s.elements[0];
        // Turned a quarter about its centre (50, 5): now 10 wide, 100 tall.
        assert!(hit(r, [50.0, 40.0], 1.0));
        assert!(!hit(r, [90.0, 5.0], 1.0));
        let b = bounds(r);
        assert!(
            (b[0] - 45.0).abs() < 1e-9 && (b[3] - 55.0).abs() < 1e-9,
            "{b:?}"
        );
    }

    #[test]
    fn arrows_are_hit_along_their_line() {
        let s = one(r#"{"id":"a","type":"arrow","x":10,"y":10,"points":[[0,0],[100,0]]}"#);
        let a = &s.elements[0];
        assert!(hit(a, [60.0, 13.0], 5.0));
        assert!(!hit(a, [60.0, 30.0], 5.0));
    }
}
