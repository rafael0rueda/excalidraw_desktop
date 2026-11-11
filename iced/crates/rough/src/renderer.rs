//! rough.js `renderer.js`: the strokes themselves.
//!
//! Every function takes the options and the random generator separately. In
//! rough.js the generator lives on the options object (`o.randomizer`), and
//! `Object.assign({}, o, …)` copies share it; passing `rng` along is that
//! sharing. `cloneOptionsAlterSeed` is the one place that starts a new one.

use crate::fillers;
use crate::jsmath;
use crate::path;
use crate::{Op, OpSet, OpSetType, Options, Point, Random};
use std::f64::consts::PI;

pub(crate) fn path_set(ops: Vec<Op>) -> OpSet {
    OpSet {
        kind: OpSetType::Path,
        ops,
    }
}

pub fn line(x1: f64, y1: f64, x2: f64, y2: f64, o: &Options, rng: &mut Random) -> OpSet {
    path_set(double_line(x1, y1, x2, y2, o, rng, false))
}

pub fn linear_path(points: &[Point], close: bool, o: &Options, rng: &mut Random) -> OpSet {
    let len = points.len();
    if len > 2 {
        let mut ops = Vec::new();
        for i in 0..len - 1 {
            ops.extend(double_line(
                points[i][0],
                points[i][1],
                points[i + 1][0],
                points[i + 1][1],
                o,
                rng,
                false,
            ));
        }
        if close {
            ops.extend(double_line(
                points[len - 1][0],
                points[len - 1][1],
                points[0][0],
                points[0][1],
                o,
                rng,
                false,
            ));
        }
        path_set(ops)
    } else if len == 2 {
        line(
            points[0][0],
            points[0][1],
            points[1][0],
            points[1][1],
            o,
            rng,
        )
    } else {
        path_set(Vec::new())
    }
}

pub fn rectangle(x: f64, y: f64, width: f64, height: f64, o: &Options, rng: &mut Random) -> OpSet {
    let points = [
        [x, y],
        [x + width, y],
        [x + width, y + height],
        [x, y + height],
    ];
    linear_path(&points, true, o, rng)
}

pub fn curve(points: &[Point], o: &Options, rng: &mut Random) -> OpSet {
    let mut o1 = curve_with_offset(points, 1.0 * (1.0 + o.roughness * 0.2), o, rng);
    if !o.disable_multi_stroke {
        let mut altered = altered_seed(o);
        let o2 = curve_with_offset(points, 1.5 * (1.0 + o.roughness * 0.22), o, &mut altered);
        o1.extend(o2);
    }
    path_set(o1)
}

pub struct EllipseParams {
    pub increment: f64,
    pub rx: f64,
    pub ry: f64,
}

pub fn generate_ellipse_params(
    width: f64,
    height: f64,
    o: &Options,
    rng: &mut Random,
) -> EllipseParams {
    let half_w = width / 2.0;
    let half_h = height / 2.0;
    let psq = (PI * 2.0 * ((half_w * half_w + half_h * half_h) / 2.0).sqrt()).sqrt();
    let step_count = jsmath::max(
        o.curve_step_count,
        (o.curve_step_count / 200f64.sqrt()) * psq,
    )
    .ceil();
    let increment = (PI * 2.0) / step_count;
    let mut rx = (width / 2.0).abs();
    let mut ry = (height / 2.0).abs();
    let curve_fit_randomness = 1.0 - o.curve_fitting;
    rx += offset_opt(rx * curve_fit_randomness, o, rng, 1.0);
    ry += offset_opt(ry * curve_fit_randomness, o, rng, 1.0);
    EllipseParams { increment, rx, ry }
}

pub struct EllipseResult {
    pub estimated_points: Vec<Point>,
    pub opset: OpSet,
}

pub fn ellipse_with_params(
    x: f64,
    y: f64,
    o: &Options,
    rng: &mut Random,
    p: &EllipseParams,
) -> EllipseResult {
    let inner = offset(0.4, 1.0, o, rng, 1.0);
    let overlap = p.increment * offset(0.1, inner, o, rng, 1.0);
    let (ap1, cp1) = compute_ellipse_points(p.increment, x, y, p.rx, p.ry, 1.0, overlap, o, rng);
    let mut o1 = curve_ops(&ap1, o, rng);
    if !o.disable_multi_stroke && o.roughness != 0.0 {
        let (ap2, _) = compute_ellipse_points(p.increment, x, y, p.rx, p.ry, 1.5, 0.0, o, rng);
        let o2 = curve_ops(&ap2, o, rng);
        o1.extend(o2);
    }
    EllipseResult {
        estimated_points: cp1,
        opset: path_set(o1),
    }
}

pub fn svg_path(d: &str, o: &Options, rng: &mut Random) -> OpSet {
    let mut ops = Vec::new();
    let mut first = [0.0, 0.0];
    let mut current = [0.0, 0.0];
    for seg in path::segments(d) {
        let data = &seg.data;
        match seg.key {
            'M' => {
                current = [data[0], data[1]];
                first = [data[0], data[1]];
            }
            'L' => {
                ops.extend(double_line(
                    current[0], current[1], data[0], data[1], o, rng, false,
                ));
                current = [data[0], data[1]];
            }
            'C' => {
                let [x1, y1, x2, y2, x, y] = [data[0], data[1], data[2], data[3], data[4], data[5]];
                ops.extend(bezier_to(x1, y1, x2, y2, x, y, current, o, rng));
                current = [x, y];
            }
            'Z' => {
                ops.extend(double_line(
                    current[0], current[1], first[0], first[1], o, rng, false,
                ));
                current = first;
            }
            _ => {}
        }
    }
    path_set(ops)
}

pub fn solid_fill_polygon(polygon_list: &[Vec<Point>], o: &Options, rng: &mut Random) -> OpSet {
    let mut ops = Vec::new();
    for points in polygon_list {
        let offset = o.max_randomness_offset;
        let len = points.len();
        if len > 2 {
            let x = points[0][0] + offset_opt(offset, o, rng, 1.0);
            let y = points[0][1] + offset_opt(offset, o, rng, 1.0);
            ops.push(Op::Move([x, y]));
            for p in &points[1..] {
                let x = p[0] + offset_opt(offset, o, rng, 1.0);
                let y = p[1] + offset_opt(offset, o, rng, 1.0);
                ops.push(Op::LineTo([x, y]));
            }
        }
    }
    OpSet {
        kind: OpSetType::FillPath,
        ops,
    }
}

/// Hachure, cross-hatch or zigzag. Takes the polygons by `&mut` because
/// rough.js rotates them in place and back, and the drift that leaves
/// behind is what a second cross-hatch pass sees.
pub fn pattern_fill_polygons(
    polygon_list: &mut [Vec<Point>],
    o: &Options,
    rng: &mut Random,
) -> OpSet {
    fillers::fill_polygons(polygon_list, o, rng)
}

// Private helpers

/// `cloneOptionsAlterSeed`: a fresh generator one seed along.
fn altered_seed(o: &Options) -> Random {
    let seed = if crate::truthy(o.seed) {
        o.seed + 1.0
    } else {
        o.seed
    };
    Random::new(seed)
}

fn offset(min: f64, max: f64, o: &Options, rng: &mut Random, roughness_gain: f64) -> f64 {
    o.roughness * roughness_gain * ((rng.next() * (max - min)) + min)
}

fn offset_opt(x: f64, o: &Options, rng: &mut Random, roughness_gain: f64) -> f64 {
    offset(-x, x, o, rng, roughness_gain)
}

pub(crate) fn double_line(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    o: &Options,
    rng: &mut Random,
    filling: bool,
) -> Vec<Op> {
    let single_stroke = if filling {
        o.disable_multi_stroke_fill
    } else {
        o.disable_multi_stroke
    };
    let mut o1 = line_ops(x1, y1, x2, y2, o, rng, true, false);
    if single_stroke {
        return o1;
    }
    let o2 = line_ops(x1, y1, x2, y2, o, rng, true, true);
    o1.extend(o2);
    o1
}

#[allow(clippy::too_many_arguments)]
fn line_ops(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    o: &Options,
    rng: &mut Random,
    mv: bool,
    overlay: bool,
) -> Vec<Op> {
    let length_sq = (x1 - x2) * (x1 - x2) + (y1 - y2) * (y1 - y2);
    let length = length_sq.sqrt();
    let roughness_gain = if length < 200.0 {
        1.0
    } else if length > 500.0 {
        0.4
    } else {
        (-0.0016668) * length + 1.233334
    };
    let mut offset = o.max_randomness_offset;
    if (offset * offset * 100.0) > length_sq {
        offset = length / 10.0;
    }
    let half_offset = offset / 2.0;
    let diverge_point = 0.2 + rng.next() * 0.2;
    let mut mid_disp_x = o.bowing * o.max_randomness_offset * (y2 - y1) / 200.0;
    let mut mid_disp_y = o.bowing * o.max_randomness_offset * (x1 - x2) / 200.0;
    mid_disp_x = offset_opt(mid_disp_x, o, rng, roughness_gain);
    mid_disp_y = offset_opt(mid_disp_y, o, rng, roughness_gain);
    let pv = o.preserve_vertices;
    let mut ops = Vec::with_capacity(2);
    // `randomHalf()` and `randomFull()`.
    let jitter = if overlay { half_offset } else { offset };
    if mv {
        let x = x1
            + if pv {
                0.0
            } else {
                offset_opt(jitter, o, rng, roughness_gain)
            };
        let y = y1
            + if pv {
                0.0
            } else {
                offset_opt(jitter, o, rng, roughness_gain)
            };
        ops.push(Op::Move([x, y]));
    }
    let a =
        mid_disp_x + x1 + (x2 - x1) * diverge_point + offset_opt(jitter, o, rng, roughness_gain);
    let b =
        mid_disp_y + y1 + (y2 - y1) * diverge_point + offset_opt(jitter, o, rng, roughness_gain);
    let c = mid_disp_x
        + x1
        + 2.0 * (x2 - x1) * diverge_point
        + offset_opt(jitter, o, rng, roughness_gain);
    let d = mid_disp_y
        + y1
        + 2.0 * (y2 - y1) * diverge_point
        + offset_opt(jitter, o, rng, roughness_gain);
    let e = x2
        + if pv {
            0.0
        } else {
            offset_opt(jitter, o, rng, roughness_gain)
        };
    let f = y2
        + if pv {
            0.0
        } else {
            offset_opt(jitter, o, rng, roughness_gain)
        };
    ops.push(Op::BCurveTo([a, b, c, d, e, f]));
    ops
}

fn curve_with_offset(points: &[Point], offset: f64, o: &Options, rng: &mut Random) -> Vec<Op> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut ps = Vec::with_capacity(points.len() + 2);
    for _ in 0..2 {
        let x = points[0][0] + offset_opt(offset, o, rng, 1.0);
        let y = points[0][1] + offset_opt(offset, o, rng, 1.0);
        ps.push([x, y]);
    }
    for i in 1..points.len() {
        let x = points[i][0] + offset_opt(offset, o, rng, 1.0);
        let y = points[i][1] + offset_opt(offset, o, rng, 1.0);
        ps.push([x, y]);
        if i == points.len() - 1 {
            let x = points[i][0] + offset_opt(offset, o, rng, 1.0);
            let y = points[i][1] + offset_opt(offset, o, rng, 1.0);
            ps.push([x, y]);
        }
    }
    curve_ops(&ps, o, rng)
}

/// `_curve(points, null, o)`. Nothing in rough.js passes a close point.
fn curve_ops(points: &[Point], o: &Options, rng: &mut Random) -> Vec<Op> {
    let len = points.len();
    let mut ops = Vec::new();
    if len > 3 {
        let s = 1.0 - o.curve_tightness;
        ops.push(Op::Move(points[1]));
        let mut i = 1;
        while i + 2 < len {
            let c = points[i];
            let b1 = [
                c[0] + (s * points[i + 1][0] - s * points[i - 1][0]) / 6.0,
                c[1] + (s * points[i + 1][1] - s * points[i - 1][1]) / 6.0,
            ];
            let b2 = [
                points[i + 1][0] + (s * points[i][0] - s * points[i + 2][0]) / 6.0,
                points[i + 1][1] + (s * points[i][1] - s * points[i + 2][1]) / 6.0,
            ];
            let b3 = points[i + 1];
            ops.push(Op::BCurveTo([b1[0], b1[1], b2[0], b2[1], b3[0], b3[1]]));
            i += 1;
        }
    } else if len == 3 {
        ops.push(Op::Move(points[1]));
        ops.push(Op::BCurveTo([
            points[1][0],
            points[1][1],
            points[2][0],
            points[2][1],
            points[2][0],
            points[2][1],
        ]));
    } else if len == 2 {
        ops.extend(double_line(
            points[0][0],
            points[0][1],
            points[1][0],
            points[1][1],
            o,
            rng,
            false,
        ));
    }
    ops
}

#[allow(clippy::too_many_arguments)]
fn compute_ellipse_points(
    mut increment: f64,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    offset: f64,
    overlap: f64,
    o: &Options,
    rng: &mut Random,
) -> (Vec<Point>, Vec<Point>) {
    let core_only = o.roughness == 0.0;
    let mut core_points = Vec::new();
    let mut all_points = Vec::new();
    let (cos, sin) = (jsmath::cos, jsmath::sin);
    if core_only {
        increment /= 4.0;
        all_points.push([cx + rx * cos(-increment), cy + ry * sin(-increment)]);
        let mut angle = 0.0;
        while angle <= PI * 2.0 {
            let p = [cx + rx * cos(angle), cy + ry * sin(angle)];
            core_points.push(p);
            all_points.push(p);
            angle += increment;
        }
        all_points.push([cx + rx * cos(0.0), cy + ry * sin(0.0)]);
        all_points.push([cx + rx * cos(increment), cy + ry * sin(increment)]);
    } else {
        let rad_offset = offset_opt(0.5, o, rng, 1.0) - (PI / 2.0);
        let x = offset_opt(offset, o, rng, 1.0) + cx + 0.9 * rx * cos(rad_offset - increment);
        let y = offset_opt(offset, o, rng, 1.0) + cy + 0.9 * ry * sin(rad_offset - increment);
        all_points.push([x, y]);
        let end_angle = PI * 2.0 + rad_offset - 0.01;
        let mut angle = rad_offset;
        while angle < end_angle {
            let x = offset_opt(offset, o, rng, 1.0) + cx + rx * cos(angle);
            let y = offset_opt(offset, o, rng, 1.0) + cy + ry * sin(angle);
            core_points.push([x, y]);
            all_points.push([x, y]);
            angle += increment;
        }
        let x =
            offset_opt(offset, o, rng, 1.0) + cx + rx * cos(rad_offset + PI * 2.0 + overlap * 0.5);
        let y =
            offset_opt(offset, o, rng, 1.0) + cy + ry * sin(rad_offset + PI * 2.0 + overlap * 0.5);
        all_points.push([x, y]);
        let x = offset_opt(offset, o, rng, 1.0) + cx + 0.98 * rx * cos(rad_offset + overlap);
        let y = offset_opt(offset, o, rng, 1.0) + cy + 0.98 * ry * sin(rad_offset + overlap);
        all_points.push([x, y]);
        let x = offset_opt(offset, o, rng, 1.0) + cx + 0.9 * rx * cos(rad_offset + overlap * 0.5);
        let y = offset_opt(offset, o, rng, 1.0) + cy + 0.9 * ry * sin(rad_offset + overlap * 0.5);
        all_points.push([x, y]);
    }
    (all_points, core_points)
}

#[allow(clippy::too_many_arguments)]
fn bezier_to(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    x: f64,
    y: f64,
    current: Point,
    o: &Options,
    rng: &mut Random,
) -> Vec<Op> {
    let mut ops = Vec::new();
    let mro = if crate::truthy(o.max_randomness_offset) {
        o.max_randomness_offset
    } else {
        1.0
    };
    let ros = [mro, mro + 0.3];
    let iterations = if o.disable_multi_stroke { 1 } else { 2 };
    let pv = o.preserve_vertices;
    for (i, ro) in ros.iter().copied().enumerate().take(iterations) {
        if i == 0 {
            ops.push(Op::Move(current));
        } else {
            let mx = current[0]
                + if pv {
                    0.0
                } else {
                    offset_opt(ros[0], o, rng, 1.0)
                };
            let my = current[1]
                + if pv {
                    0.0
                } else {
                    offset_opt(ros[0], o, rng, 1.0)
                };
            ops.push(Op::Move([mx, my]));
        }
        let f = if pv {
            [x, y]
        } else {
            let fx = x + offset_opt(ro, o, rng, 1.0);
            let fy = y + offset_opt(ro, o, rng, 1.0);
            [fx, fy]
        };
        let a = x1 + offset_opt(ro, o, rng, 1.0);
        let b = y1 + offset_opt(ro, o, rng, 1.0);
        let c = x2 + offset_opt(ro, o, rng, 1.0);
        let d = y2 + offset_opt(ro, o, rng, 1.0);
        ops.push(Op::BCurveTo([a, b, c, d, f[0], f[1]]));
    }
    ops
}
