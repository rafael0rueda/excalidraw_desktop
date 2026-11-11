//! perfect-freehand 1.2.0, the outline of a freehand stroke, and
//! Excalidraw's `getFreeDrawSvgPath` over it.
//!
//! Transcribed from the published build with the same care as the rough
//! port: the same operations in the same order, V8's `Math.hypot`, and
//! fdlibm's `sin`/`cos`. `tests/freehand.rs` holds it to the original.

use rough::jsmath;
use rough::Point;
use scene::FreeDraw;

#[derive(Clone, Copy, Debug)]
struct StrokePoint {
    point: Point,
    pressure: f64,
    vector: Point,
    distance: f64,
    running_length: f64,
}

pub struct Options {
    pub size: f64,
    pub thinning: f64,
    pub smoothing: f64,
    pub streamline: f64,
    pub simulate_pressure: bool,
    /// `easing`; Excalidraw's is ease-out sine.
    pub easing: fn(f64) -> f64,
    pub last: bool,
}

const RATE_OF_PRESSURE_CHANGE: f64 = 0.275;
const FIXED_PI: f64 = std::f64::consts::PI + 1e-4;

fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(a: Point, n: f64) -> Point {
    [a[0] * n, a[1] * n]
}
fn div(a: Point, n: f64) -> Point {
    [a[0] / n, a[1] / n]
}
fn neg(a: Point) -> Point {
    [-a[0], -a[1]]
}
fn per(a: Point) -> Point {
    [a[1], -a[0]]
}
fn dpr(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn len(a: Point) -> f64 {
    jsmath::hypot(&[a[0], a[1]])
}
fn len2(a: Point) -> f64 {
    a[0] * a[0] + a[1] * a[1]
}
fn dist2(a: Point, b: Point) -> f64 {
    len2(sub(a, b))
}
fn uni(a: Point) -> Point {
    div(a, len(a))
}
fn dist(a: Point, b: Point) -> f64 {
    jsmath::hypot(&[a[1] - b[1], a[0] - b[0]])
}
fn rot_around(a: Point, c: Point, r: f64) -> Point {
    let s = jsmath::sin(r);
    let co = jsmath::cos(r);
    let px = a[0] - c[0];
    let py = a[1] - c[1];
    let nx = px * co - py * s;
    let ny = px * s + py * co;
    [nx + c[0], ny + c[1]]
}
fn lrp(a: Point, b: Point, t: f64) -> Point {
    add(a, mul(sub(b, a), t))
}
fn prj(a: Point, b: Point, c: f64) -> Point {
    add(a, mul(b, c))
}

fn stroke_radius(size: f64, thinning: f64, pressure: f64, easing: fn(f64) -> f64) -> f64 {
    size * easing(0.5 - thinning * (0.5 - pressure))
}

/// `p[2] >= 0 ? p[2] : fallback`, where a missing pressure is `undefined`.
fn pressure_or(p: Option<f64>, fallback: f64) -> f64 {
    match p {
        Some(v) if v >= 0.0 => v,
        _ => fallback,
    }
}

/// `getStrokePoints`. Input points are `[x, y, pressure?]`.
fn stroke_points(input: &[(Point, Option<f64>)], o: &Options) -> Vec<StrokePoint> {
    if input.is_empty() {
        return Vec::new();
    }
    let t = 0.15 + (1.0 - o.streamline) * 0.85;
    let mut pts: Vec<(Point, Option<f64>)> = input.to_vec();
    if pts.len() == 2 {
        let last = pts[1].0;
        pts.truncate(1);
        for i in 1..5 {
            // `lrp` returns two numbers, so these points carry no pressure.
            pts.push((lrp(pts[0].0, last, f64::from(i) / 4.0), None));
        }
    }
    if pts.len() == 1 {
        pts.push((add(pts[0].0, [1.0, 1.0]), pts[0].1));
    }
    let mut out = vec![StrokePoint {
        point: pts[0].0,
        pressure: pressure_or(pts[0].1, 0.25),
        vector: [1.0, 1.0],
        distance: 0.0,
        running_length: 0.0,
    }];
    let mut has_reached_minimum_length = false;
    let mut running_length = 0.0;
    let mut prev = out[0];
    let max = pts.len() - 1;
    for (i, &(p, pressure)) in pts.iter().enumerate().skip(1) {
        let point = if o.last && i == max {
            p
        } else {
            lrp(prev.point, p, t)
        };
        if prev.point == point {
            continue;
        }
        let distance = dist(point, prev.point);
        running_length += distance;
        if i < max && !has_reached_minimum_length {
            if running_length < o.size {
                continue;
            }
            has_reached_minimum_length = true;
        }
        prev = StrokePoint {
            point,
            pressure: pressure_or(pressure, 0.5),
            vector: uni(sub(prev.point, point)),
            distance,
            running_length,
        };
        out.push(prev);
    }
    out[0].vector = out.get(1).map_or([0.0, 0.0], |p| p.vector);
    out
}

/// `getStrokeOutlinePoints`, with Excalidraw's options: no tapers, both caps.
fn outline_points(points: &[StrokePoint], o: &Options) -> Vec<Point> {
    let size = o.size;
    if points.is_empty() || size <= 0.0 {
        return Vec::new();
    }
    let n = points.len();
    let total_length = points[n - 1].running_length;
    let min_distance = (size * o.smoothing) * (size * o.smoothing);
    let mut left: Vec<Point> = Vec::new();
    let mut right: Vec<Point> = Vec::new();

    let mut prev_pressure = points
        .iter()
        .take(10)
        .fold(points[0].pressure, |acc, curr| {
            let mut pressure = curr.pressure;
            if o.simulate_pressure {
                let sp = jsmath::min(1.0, curr.distance / size);
                let rp = jsmath::min(1.0, 1.0 - sp);
                pressure = jsmath::min(1.0, acc + (rp - acc) * (sp * RATE_OF_PRESSURE_CHANGE));
            }
            (acc + pressure) / 2.0
        });
    let mut radius = stroke_radius(size, o.thinning, points[n - 1].pressure, o.easing);
    let mut first_radius: Option<f64> = None;
    let mut prev_vector = points[0].vector;
    let mut pl = points[0].point;
    let mut pr = pl;
    let mut tl = pl;
    let mut tr = pr;
    let mut is_prev_point_sharp_corner = false;

    for i in 0..n {
        let StrokePoint {
            point,
            vector,
            distance,
            running_length,
            mut pressure,
        } = points[i];
        if i < n - 1 && total_length - running_length < 3.0 {
            continue;
        }
        if o.thinning != 0.0 && !o.thinning.is_nan() {
            if o.simulate_pressure {
                let sp = jsmath::min(1.0, distance / size);
                let rp = jsmath::min(1.0, 1.0 - sp);
                pressure = jsmath::min(
                    1.0,
                    prev_pressure + (rp - prev_pressure) * (sp * RATE_OF_PRESSURE_CHANGE),
                );
            }
            radius = stroke_radius(size, o.thinning, pressure, o.easing);
        } else {
            radius = size / 2.0;
        }
        if first_radius.is_none() {
            first_radius = Some(radius);
        }
        // No tapers: both easings are 1.
        radius = jsmath::max(0.01, radius * jsmath::min(1.0, 1.0));

        let next_vector = if i < n - 1 {
            points[i + 1].vector
        } else {
            points[i].vector
        };
        let next_dpr = if i < n - 1 {
            dpr(vector, next_vector)
        } else {
            1.0
        };
        let prev_dpr = dpr(vector, prev_vector);
        let is_point_sharp_corner = prev_dpr < 0.0 && !is_prev_point_sharp_corner;
        let is_next_point_sharp_corner = next_dpr < 0.0;

        if is_point_sharp_corner || is_next_point_sharp_corner {
            let offset = mul(per(prev_vector), radius);
            let step = 1.0 / 13.0;
            let mut t = 0.0;
            while t <= 1.0 {
                tl = rot_around(sub(point, offset), point, FIXED_PI * t);
                left.push(tl);
                tr = rot_around(add(point, offset), point, FIXED_PI * -t);
                right.push(tr);
                t += step;
            }
            pl = tl;
            pr = tr;
            if is_next_point_sharp_corner {
                is_prev_point_sharp_corner = true;
            }
            continue;
        }
        is_prev_point_sharp_corner = false;

        if i == n - 1 {
            let offset = mul(per(vector), radius);
            left.push(sub(point, offset));
            right.push(add(point, offset));
            continue;
        }

        let offset = mul(per(lrp(next_vector, vector, next_dpr)), radius);
        tl = sub(point, offset);
        if i <= 1 || dist2(pl, tl) > min_distance {
            left.push(tl);
            pl = tl;
        }
        tr = add(point, offset);
        if i <= 1 || dist2(pr, tr) > min_distance {
            right.push(tr);
            pr = tr;
        }
        prev_pressure = pressure;
        prev_vector = vector;
    }

    let first_point = points[0].point;
    let last_point = if n > 1 {
        points[n - 1].point
    } else {
        add(points[0].point, [1.0, 1.0])
    };
    let mut start_cap = Vec::new();
    let mut end_cap = Vec::new();

    if n == 1 {
        // `!(taperStart || taperEnd) || isComplete`: always, with no tapers.
        let r = first_radius.filter(|r| *r != 0.0).unwrap_or(radius);
        let start = prj(first_point, uni(per(sub(first_point, last_point))), -r);
        let mut dot = Vec::new();
        let step = 1.0 / 13.0;
        let mut t = step;
        while t <= 1.0 {
            dot.push(rot_around(start, first_point, FIXED_PI * 2.0 * t));
            t += step;
        }
        return dot;
    }

    // The last point always reaches `right`, so this is never empty; where
    // the original would throw, this leaves the cap off.
    if let Some(&first_right) = right.first() {
        let step = 1.0 / 13.0;
        let mut t = step;
        while t <= 1.0 {
            start_cap.push(rot_around(first_right, first_point, FIXED_PI * t));
            t += step;
        }
    }
    let direction = per(neg(points[n - 1].vector));
    let start = prj(last_point, direction, radius);
    let step = 1.0 / 29.0;
    let mut t = step;
    while t < 1.0 {
        end_cap.push(rot_around(start, last_point, FIXED_PI * 3.0 * t));
        t += step;
    }

    right.reverse();
    let mut out = left;
    out.extend(end_cap);
    out.extend(right);
    out.extend(start_cap);
    out
}

/// `getStroke`.
pub fn stroke(input: &[(Point, Option<f64>)], o: &Options) -> Vec<Point> {
    outline_points(&stroke_points(input, o), o)
}

/// Excalidraw's freedraw options: `size: strokeWidth * 4.25`, ease-out sine.
fn excalidraw_options(element: &FreeDraw, stroke_width: f64) -> Options {
    Options {
        size: stroke_width * 4.25,
        thinning: 0.6,
        smoothing: 0.5,
        streamline: 0.5,
        simulate_pressure: element.simulate_pressure,
        easing: |t| jsmath::sin(t * std::f64::consts::PI / 2.0),
        // `!!element.lastCommittedPoint`, which `restoreElement` nulls.
        last: false,
    }
}

/// `getFreeDrawSvgPath`: the outline as SVG path data, its numbers cut to
/// two decimals the way Excalidraw cuts them.
pub fn svg_path(element: &FreeDraw, stroke_width: f64) -> String {
    let input: Vec<(Point, Option<f64>)> = if element.simulate_pressure {
        element.points.iter().map(|p| (*p, None)).collect()
    } else if element.points.is_empty() {
        vec![([0.0, 0.0], Some(0.5))]
    } else {
        element
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| (*p, element.pressures.get(i).copied()))
            .collect()
    };
    let outline = stroke(&input, &excalidraw_options(element, stroke_width));
    to_fixed_precision(&svg_path_from_stroke(&outline))
}

fn js_number(n: f64) -> String {
    if n.is_nan() {
        "NaN".into()
    } else if n.is_infinite() {
        if n > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        }
    } else {
        ryu_js::Buffer::new().format_finite(n).to_owned()
    }
}

/// `getSvgPathFromStroke` before its regular expression: points as `x,y`,
/// joined by spaces, quadratics through the midpoints.
fn svg_path_from_stroke(points: &[Point]) -> String {
    if points.is_empty() {
        return String::new();
    }
    let pt = |p: Point| format!("{},{}", js_number(p[0]), js_number(p[1]));
    let med = |a: Point, b: Point| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let mut parts = vec!["M".to_owned(), pt(points[0]), "Q".to_owned()];
    let max = points.len() - 1;
    for (i, &p) in points.iter().enumerate() {
        if i == max {
            parts.extend([
                pt(p),
                pt(med(p, points[0])),
                "L".into(),
                pt(points[0]),
                "Z".into(),
            ]);
        } else {
            parts.extend([pt(p), pt(med(p, points[i + 1]))]);
        }
    }
    parts.join(" ")
}

/// `.replace(/(\s?[A-Z]?,?-?[0-9]*\.[0-9]{0,2})(([0-9]|e|-)*)/g, "$1")`:
/// keeps two decimals of every number with a decimal point, and drops the
/// digits, `e` and `-` that follow.
fn to_fixed_precision(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        let mut j = i;
        if j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if j < b.len() && b[j].is_ascii_uppercase() {
            j += 1;
        }
        if j < b.len() && b[j] == b',' {
            j += 1;
        }
        if j < b.len() && b[j] == b'-' {
            j += 1;
        }
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j < b.len() && b[j] == b'.' {
            j += 1;
            let mut kept = 0;
            while kept < 2 && j < b.len() && b[j].is_ascii_digit() {
                j += 1;
                kept += 1;
            }
            out.push_str(&s[i..j]);
            while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'e' || b[j] == b'-') {
                j += 1;
            }
            i = j;
        } else {
            // No match here: the character stands, and matching resumes after it.
            let c = s[i..].chars().next().unwrap_or(' ');
            out.push(c);
            i += c.len_utf8();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::to_fixed_precision;

    #[test]
    fn numbers_are_cut_not_rounded_and_exponents_dropped() {
        assert_eq!(
            to_fixed_precision("M 12.3456,7.891 Q 1.5e-7,-0.129 L 5,6 Z"),
            "M 12.34,7.89 Q 1.5,-0.12 L 5,6 Z"
        );
    }
}
