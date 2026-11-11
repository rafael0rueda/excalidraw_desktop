//! rough.js's hachure, cross-hatch and zigzag fills, and hachure-fill 0.5.2's
//! scan-line algorithm underneath them.

use crate::jsmath;
use crate::renderer::double_line;
use crate::{FillStyle, Op, OpSet, OpSetType, Options, Point, Random};
use std::cmp::Ordering;

pub(crate) fn fill_polygons(
    polygon_list: &mut [Vec<Point>],
    o: &Options,
    rng: &mut Random,
) -> OpSet {
    match o.fill_style {
        FillStyle::CrossHatch => {
            let mut set = hachure(polygon_list, o, rng);
            let o2 = Options {
                hachure_angle: o.hachure_angle + 90.0,
                ..o.clone()
            };
            let set2 = hachure(polygon_list, &o2, rng);
            set.ops.extend(set2.ops);
            set
        }
        FillStyle::Zigzag => zigzag(polygon_list, o, rng),
        FillStyle::Hachure | FillStyle::Solid => hachure(polygon_list, o, rng),
    }
}

fn sketch(ops: Vec<Op>) -> OpSet {
    OpSet {
        kind: OpSetType::FillSketch,
        ops,
    }
}

fn hachure(polygon_list: &mut [Vec<Point>], o: &Options, rng: &mut Random) -> OpSet {
    let lines = polygon_hachure_lines(polygon_list, o, rng);
    sketch(render_lines(&lines, o, rng))
}

fn render_lines(lines: &[[Point; 2]], o: &Options, rng: &mut Random) -> Vec<Op> {
    let mut ops = Vec::new();
    for line in lines {
        ops.extend(double_line(
            line[0][0], line[0][1], line[1][0], line[1][1], o, rng, true,
        ));
    }
    ops
}

fn line_length(line: &[Point; 2]) -> f64 {
    let dx = line[0][0] - line[1][0];
    let dy = line[0][1] - line[1][1];
    (dx * dx + dy * dy).sqrt()
}

fn zigzag(polygon_list: &mut [Vec<Point>], o: &Options, rng: &mut Random) -> OpSet {
    let mut gap = o.hachure_gap;
    if gap < 0.0 {
        gap = o.stroke_width * 4.0;
    }
    gap = jsmath::max(gap, 0.1);
    let o2 = Options {
        hachure_gap: gap,
        ..o.clone()
    };
    let lines = polygon_hachure_lines(polygon_list, &o2, rng);
    let zig_zag_angle = (std::f64::consts::PI / 180.0) * o.hachure_angle;
    let dgx = gap * 0.5 * jsmath::cos(zig_zag_angle);
    let dgy = gap * 0.5 * jsmath::sin(zig_zag_angle);
    let mut zigzag_lines = Vec::new();
    for [p1, p2] in &lines {
        if crate::truthy(line_length(&[*p1, *p2])) {
            zigzag_lines.push([[p1[0] - dgx, p1[1] + dgy], *p2]);
            zigzag_lines.push([[p1[0] + dgx, p1[1] - dgy], *p2]);
        }
    }
    sketch(render_lines(&zigzag_lines, o, rng))
}

/// `polygonHachureLines` from rough.js's scan-line-hachure.
fn polygon_hachure_lines(
    polygon_list: &mut [Vec<Point>],
    o: &Options,
    rng: &mut Random,
) -> Vec<[Point; 2]> {
    let angle = o.hachure_angle + 90.0;
    let mut gap = o.hachure_gap;
    if gap < 0.0 {
        gap = o.stroke_width * 4.0;
    }
    gap = jsmath::max(gap, 0.1);
    let mut skip_offset = 1.0;
    if o.roughness >= 1.0 && rng.next() > 0.7 {
        skip_offset = gap;
    }
    hachure_lines(
        polygon_list,
        gap,
        angle,
        if crate::truthy(skip_offset) {
            skip_offset
        } else {
            1.0
        },
    )
}

fn rotate_points(points: &mut [Point], degrees: f64) {
    let angle = (std::f64::consts::PI / 180.0) * degrees;
    let cos = jsmath::cos(angle);
    let sin = jsmath::sin(angle);
    let (cx, cy) = (0.0, 0.0);
    for p in points {
        let [x, y] = *p;
        p[0] = ((x - cx) * cos) - ((y - cy) * sin) + cx;
        p[1] = ((x - cx) * sin) + ((y - cy) * cos) + cy;
    }
}

/// hachure-fill's `hachureLines`. The polygons are rotated in place and back,
/// exactly as the original does: the rounding that leaves is part of what a
/// later pass over the same polygons sees.
pub(crate) fn hachure_lines(
    polygons: &mut [Vec<Point>],
    hachure_gap: f64,
    angle: f64,
    step_offset: f64,
) -> Vec<[Point; 2]> {
    let gap = jsmath::max(hachure_gap, 0.1);
    let rotate = crate::truthy(angle);
    if rotate {
        for polygon in polygons.iter_mut() {
            rotate_points(polygon, angle);
        }
    }
    let mut lines = straight_hachure_lines(polygons, gap, step_offset);
    if rotate {
        for polygon in polygons.iter_mut() {
            rotate_points(polygon, -angle);
        }
        let mut flat: Vec<Point> = lines.iter().flat_map(|l| [l[0], l[1]]).collect();
        rotate_points(&mut flat, -angle);
        for (i, line) in lines.iter_mut().enumerate() {
            *line = [flat[2 * i], flat[2 * i + 1]];
        }
    }
    lines
}

struct Edge {
    ymin: f64,
    ymax: f64,
    x: f64,
    islope: f64,
}

/// `(a - b) / Math.abs(a - b)` as a sort result.
fn sign(a: f64, b: f64) -> Ordering {
    let v = (a - b) / (a - b).abs();
    if v < 0.0 {
        Ordering::Less
    } else if v > 0.0 {
        Ordering::Greater
    } else {
        Ordering::Equal
    }
}

fn straight_hachure_lines(polygons: &[Vec<Point>], gap: f64, step_offset: f64) -> Vec<[Point; 2]> {
    let mut vertex_array = Vec::new();
    for polygon in polygons {
        let Some(&first) = polygon.first() else {
            continue;
        };
        let mut vertices = polygon.clone();
        let last = vertices[vertices.len() - 1];
        if !(first[0] == last[0] && first[1] == last[1]) {
            vertices.push(first);
        }
        if vertices.len() > 2 {
            vertex_array.push(vertices);
        }
    }
    let mut lines = Vec::new();
    let gap = jsmath::max(gap, 0.1);

    let mut edges = Vec::new();
    for vertices in &vertex_array {
        for pair in vertices.windows(2) {
            let (p1, p2) = (pair[0], pair[1]);
            if p1[1] != p2[1] {
                let ymin = jsmath::min(p1[1], p2[1]);
                edges.push(Edge {
                    ymin,
                    ymax: jsmath::max(p1[1], p2[1]),
                    x: if ymin == p1[1] { p1[0] } else { p2[0] },
                    islope: (p2[0] - p1[0]) / (p2[1] - p1[1]),
                });
            }
        }
    }
    // Stable, as V8's sort is, so ties keep their order the same way.
    edges.sort_by(|e1, e2| {
        if e1.ymin < e2.ymin {
            return Ordering::Less;
        }
        if e1.ymin > e2.ymin {
            return Ordering::Greater;
        }
        if e1.x < e2.x {
            return Ordering::Less;
        }
        if e1.x > e2.x {
            return Ordering::Greater;
        }
        if e1.ymax == e2.ymax {
            return Ordering::Equal;
        }
        sign(e1.ymax, e2.ymax)
    });
    if edges.is_empty() {
        return lines;
    }

    let mut edges = std::collections::VecDeque::from(edges);
    let mut active: Vec<Edge> = Vec::new();
    let mut y = edges[0].ymin;
    let mut iteration = 0.0f64;
    while !active.is_empty() || !edges.is_empty() {
        while edges.front().is_some_and(|e| e.ymin <= y) {
            active.extend(edges.pop_front());
        }
        // `filter(ae => !(ae.edge.ymax <= y))`, NaN included.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        active.retain(|ae| !(ae.ymax <= y));
        active.sort_by(|a, b| {
            if a.x == b.x {
                Ordering::Equal
            } else {
                sign(a.x, b.x)
            }
        });
        if step_offset != 1.0 || iteration % gap == 0.0 {
            let mut i = 0;
            while i + 1 < active.len() {
                lines.push([
                    [jsmath::round(active[i].x), y],
                    [jsmath::round(active[i + 1].x), y],
                ]);
                i += 2;
            }
        }
        y += step_offset;
        for ae in &mut active {
            ae.x += step_offset * ae.islope;
        }
        iteration += 1.0;
    }
    lines
}
