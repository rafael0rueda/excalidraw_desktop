//! points-on-curve 1.0.1: flattening Béziers, Ramer–Douglas–Peucker, and
//! Catmull-Rom to Bézier.

use crate::Point;

fn distance(p1: Point, p2: Point) -> f64 {
    distance_sq(p1, p2).sqrt()
}

fn distance_sq(p1: Point, p2: Point) -> f64 {
    // `Math.pow(d, 2)` is `d * d` exactly in V8.
    let dx = p1[0] - p2[0];
    let dy = p1[1] - p2[1];
    dx * dx + dy * dy
}

fn distance_to_segment_sq(p: Point, v: Point, w: Point) -> f64 {
    let l2 = distance_sq(v, w);
    if l2 == 0.0 {
        return distance_sq(p, v);
    }
    let mut t = ((p[0] - v[0]) * (w[0] - v[0]) + (p[1] - v[1]) * (w[1] - v[1])) / l2;
    t = crate::jsmath::max(0.0, crate::jsmath::min(1.0, t));
    distance_sq(p, lerp(v, w, t))
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

fn flatness(points: &[Point], offset: usize) -> f64 {
    let p1 = points[offset];
    let p2 = points[offset + 1];
    let p3 = points[offset + 2];
    let p4 = points[offset + 3];
    let mut ux = 3.0 * p2[0] - 2.0 * p1[0] - p4[0];
    ux *= ux;
    let mut uy = 3.0 * p2[1] - 2.0 * p1[1] - p4[1];
    uy *= uy;
    let mut vx = 3.0 * p3[0] - 2.0 * p4[0] - p1[0];
    vx *= vx;
    let mut vy = 3.0 * p3[1] - 2.0 * p4[1] - p1[1];
    vy *= vy;
    if ux < vx {
        ux = vx;
    }
    if uy < vy {
        uy = vy;
    }
    ux + uy
}

fn points_on_bezier_curve_with_splitting(
    points: &[Point],
    offset: usize,
    tolerance: f64,
    out: &mut Vec<Point>,
) {
    if flatness(points, offset) < tolerance {
        let p0 = points[offset];
        match out.last() {
            Some(&last) => {
                if distance(last, p0) > 1.0 {
                    out.push(p0);
                }
            }
            None => out.push(p0),
        }
        out.push(points[offset + 3]);
    } else {
        let t = 0.5;
        let p1 = points[offset];
        let p2 = points[offset + 1];
        let p3 = points[offset + 2];
        let p4 = points[offset + 3];
        let q1 = lerp(p1, p2, t);
        let q2 = lerp(p2, p3, t);
        let q3 = lerp(p3, p4, t);
        let r1 = lerp(q1, q2, t);
        let r2 = lerp(q2, q3, t);
        let red = lerp(r1, r2, t);
        points_on_bezier_curve_with_splitting(&[p1, q1, r1, red], 0, tolerance, out);
        points_on_bezier_curve_with_splitting(&[red, r2, q3, p4], 0, tolerance, out);
    }
}

/// `simplify`: Ramer–Douglas–Peucker over the whole list.
pub fn simplify(points: &[Point], distance: f64) -> Vec<Point> {
    let mut out = Vec::new();
    simplify_points(points, 0, points.len(), distance, &mut out);
    out
}

fn simplify_points(points: &[Point], start: usize, end: usize, epsilon: f64, out: &mut Vec<Point>) {
    if end == 0 || start >= points.len() {
        return;
    }
    let s = points[start];
    let e = points[end - 1];
    let mut max_dist_sq = 0.0;
    // Absolute index 1, as in the original, whatever `start` is.
    let mut max_ndx = 1;
    for (i, &p) in points
        .iter()
        .enumerate()
        .take(end.saturating_sub(1))
        .skip(start + 1)
    {
        let dist_sq = distance_to_segment_sq(p, s, e);
        if dist_sq > max_dist_sq {
            max_dist_sq = dist_sq;
            max_ndx = i;
        }
    }
    if max_dist_sq.sqrt() > epsilon {
        simplify_points(points, start, max_ndx + 1, epsilon, out);
        simplify_points(points, max_ndx, end, epsilon, out);
    } else {
        if out.is_empty() {
            out.push(s);
        }
        out.push(e);
    }
}

/// `pointsOnBezierCurves`: flattens a run of cubic segments sharing endpoints
/// (`p0 c1 c2 p1 c1 c2 p2 …`), then simplifies if `distance` is positive.
pub fn points_on_bezier_curves(
    points: &[Point],
    tolerance: f64,
    distance: Option<f64>,
) -> Vec<Point> {
    let mut new_points = Vec::new();
    if !points.is_empty() {
        let num_segments = (points.len() as f64 - 1.0) / 3.0;
        let mut i = 0usize;
        while (i as f64) < num_segments && i * 3 + 3 < points.len() {
            points_on_bezier_curve_with_splitting(points, i * 3, tolerance, &mut new_points);
            i += 1;
        }
    }
    if let Some(distance) = distance.filter(|d| *d > 0.0) {
        let mut out = Vec::new();
        simplify_points(&new_points, 0, new_points.len(), distance, &mut out);
        return out;
    }
    new_points
}

/// `curveToBezier`: the Catmull-Rom curve through `points`, as cubic segments.
/// None for fewer than three points, where the original throws.
pub fn curve_to_bezier(points_in: &[Point], curve_tightness: f64) -> Option<Vec<Point>> {
    let len = points_in.len();
    if len < 3 {
        return None;
    }
    let mut out = Vec::new();
    if len == 3 {
        out.extend([points_in[0], points_in[1], points_in[2], points_in[2]]);
    } else {
        let mut points = vec![points_in[0], points_in[0]];
        for (i, &p) in points_in.iter().enumerate().skip(1) {
            points.push(p);
            if i == len - 1 {
                points.push(p);
            }
        }
        let s = 1.0 - curve_tightness;
        out.push(points[0]);
        let mut i = 1;
        while i + 2 < points.len() {
            let c = points[i];
            let b1 = [
                c[0] + (s * points[i + 1][0] - s * points[i - 1][0]) / 6.0,
                c[1] + (s * points[i + 1][1] - s * points[i - 1][1]) / 6.0,
            ];
            let b2 = [
                points[i + 1][0] + (s * points[i][0] - s * points[i + 2][0]) / 6.0,
                points[i + 1][1] + (s * points[i][1] - s * points[i + 2][1]) / 6.0,
            ];
            let b3 = [points[i + 1][0], points[i + 1][1]];
            out.extend([b1, b2, b3]);
            i += 1;
        }
    }
    Some(out)
}
