//! Excalidraw 0.18.1's `scene/Shape.ts`: which rough.js call draws each
//! element, with which options.

use crate::color::is_transparent;
use rough::{jsmath, Drawable, FillStyle, Op, OpSetType, Options, Point, NONE};
use scene::{Element, Kind};

/// `LINE_CONFIRM_THRESHOLD`: a line whose ends are this close is a loop.
const LINE_CONFIRM_THRESHOLD: f64 = 8.0;
const DEFAULT_PROPORTIONAL_RADIUS: f64 = 0.25;
const DEFAULT_ADAPTIVE_RADIUS: f64 = 32.0;

pub(crate) fn js_number(n: f64) -> String {
    if n.is_finite() {
        ryu_js::Buffer::new().format_finite(n).to_owned()
    } else if n.is_nan() {
        "NaN".into()
    } else if n > 0.0 {
        "Infinity".into()
    } else {
        "-Infinity".into()
    }
}

fn point_distance(a: Point, b: Point) -> f64 {
    jsmath::hypot(&[b[0] - a[0], b[1] - a[1]])
}

/// `isPathALoop` at zoom 1.
pub fn is_path_a_loop(points: &[Point]) -> bool {
    points.len() >= 3
        && point_distance(points[0], points[points.len() - 1]) <= LINE_CONFIRM_THRESHOLD
}

/// `canChangeRoundness`.
fn can_change_roundness(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Rectangle
            | Kind::Iframe
            | Kind::Embeddable
            | Kind::Line
            | Kind::Diamond
            | Kind::Image
    )
}

/// `adjustRoughness`: small shapes are drawn less rough.
fn adjust_roughness(e: &Element) -> f64 {
    let roughness = e.roughness;
    let max_size = jsmath::max(e.width, e.height);
    let min_size = jsmath::min(e.width, e.height);
    if (min_size >= 20.0 && max_size >= 50.0)
        || (min_size >= 15.0 && e.roundness.is_some() && can_change_roundness(e.kind))
        || (e.kind.is_linear() && max_size >= 50.0)
    {
        return roughness;
    }
    jsmath::min(roughness / if max_size < 10.0 { 3.0 } else { 2.0 }, 2.5)
}

/// `getCornerRadius`.
pub fn corner_radius(x: f64, e: &Element) -> f64 {
    let Some(r) = e.roundness else { return 0.0 };
    if r.kind == 2.0 || r.kind == 1.0 {
        return x * DEFAULT_PROPORTIONAL_RADIUS;
    }
    if r.kind == 3.0 {
        let fixed = r.value.unwrap_or(DEFAULT_ADAPTIVE_RADIUS);
        let cutoff = fixed / DEFAULT_PROPORTIONAL_RADIUS;
        if x <= cutoff {
            return x * DEFAULT_PROPORTIONAL_RADIUS;
        }
        return fixed;
    }
    0.0
}

/// `getDiamondPoints`: top, right, bottom, left.
pub fn diamond_points(e: &Element) -> [f64; 8] {
    let top_x = (e.width / 2.0).floor() + 1.0;
    let right_y = (e.height / 2.0).floor() + 1.0;
    [top_x, 0.0, e.width, right_y, top_x, e.height, 0.0, right_y]
}

/// `generateRoughOptions`.
fn rough_options(e: &Element, continuous_path: bool) -> Options {
    let sw = e.stroke_width;
    let solid = e.stroke_style == "solid";
    let mut o = Options {
        seed: e.seed,
        stroke_line_dash: match e.stroke_style.as_str() {
            "dashed" => Some(vec![8.0, 8.0 + sw]),
            "dotted" => Some(vec![1.5, 6.0 + sw]),
            _ => None,
        },
        disable_multi_stroke: !solid,
        stroke_width: if solid { sw } else { sw + 0.5 },
        fill_weight: sw / 2.0,
        hachure_gap: sw * 4.0,
        roughness: adjust_roughness(e),
        stroke: e.stroke_color.clone(),
        // The element's own roughness here, not the adjusted one.
        preserve_vertices: continuous_path || e.roughness < 2.0,
        ..Options::default()
    };
    match e.kind {
        Kind::Rectangle | Kind::Iframe | Kind::Embeddable | Kind::Diamond | Kind::Ellipse => {
            o.fill_style = FillStyle::from_name(&e.fill_style);
            o.fill = (!is_transparent(&e.background_color)).then(|| e.background_color.clone());
            if e.kind == Kind::Ellipse {
                o.curve_fitting = 1.0;
            }
        }
        Kind::Line | Kind::FreeDraw => {
            let points = match (&e.linear, &e.free_draw) {
                (Some(l), _) => l.points.as_slice(),
                (_, Some(f)) => f.points.as_slice(),
                _ => &[],
            };
            if is_path_a_loop(points) {
                o.fill_style = FillStyle::from_name(&e.fill_style);
                // Only the literal "transparent" here, as in the original.
                o.fill = (e.background_color != "transparent").then(|| e.background_color.clone());
            }
        }
        _ => {}
    }
    o
}

/// `modifyIframeLikeForRoughOptions`, never exporting, with no embed ever
/// validated: this app shows embeds only as placeholders.
fn iframe_like(e: &Element) -> std::borrow::Cow<'_, Element> {
    use std::borrow::Cow;
    // `(isExporting || isEmbeddable && !validated)`: never exporting here,
    // and nothing validated, so embeddables only, not iframes.
    if e.kind == Kind::Embeddable
        && is_transparent(&e.background_color)
        && is_transparent(&e.stroke_color)
    {
        let mut m = e.clone();
        m.roughness = 0.0;
        m.background_color = "#d3d3d3".into();
        m.fill_style = "solid".into();
        return Cow::Owned(m);
    }
    if e.kind == Kind::Iframe {
        let mut m = e.clone();
        if is_transparent(&m.stroke_color) {
            m.stroke_color = "#000000".into();
        }
        if is_transparent(&m.background_color) {
            m.background_color = "#f4f4f6".into();
        }
        return Cow::Owned(m);
    }
    Cow::Borrowed(e)
}

/// `getCurvePathOps`: the first stroked set, else the first set.
fn curve_path_ops(shape: &Drawable) -> &[Op] {
    shape
        .sets
        .iter()
        .find(|s| s.kind == OpSetType::Path)
        .or(shape.sets.first())
        .map_or(&[], |s| s.ops.as_slice())
}

fn arrowhead_size(arrowhead: &str) -> f64 {
    match arrowhead {
        "arrow" => 25.0,
        "diamond" | "diamond_outline" => 12.0,
        "crowfoot_many" | "crowfoot_one" | "crowfoot_one_or_many" => 20.0,
        _ => 15.0,
    }
}

fn arrowhead_angle(arrowhead: &str) -> f64 {
    match arrowhead {
        "bar" => 90.0,
        "arrow" => 20.0,
        _ => 25.0,
    }
}

fn degrees_to_radians(degrees: f64) -> f64 {
    degrees * std::f64::consts::PI / 180.0
}

/// `pointRotateRads`.
fn rotate(p: Point, c: Point, angle: f64) -> Point {
    [
        (p[0] - c[0]) * jsmath::cos(angle) - (p[1] - c[1]) * jsmath::sin(angle) + c[0],
        (p[0] - c[0]) * jsmath::sin(angle) + (p[1] - c[1]) * jsmath::cos(angle) + c[1],
    ]
}

/// `options.roughness || 0`.
fn or_zero(x: f64) -> f64 {
    if x.is_nan() {
        0.0
    } else {
        x
    }
}

/// The only powers `getArrowheadPoints` takes are of `t = 0.3`: these four,
/// as V8's `Math.pow` gives them. Not `x * x * x`: V8's pow differs from
/// that on about a quarter of inputs, and from glibc's and `libm`'s too, so
/// the values are taken from it rather than recomputed.
///
///   node -e 'console.log(Math.pow(0.7,3), Math.pow(0.7,2), Math.pow(0.3,2), Math.pow(0.3,3))'
const POW_0_7_3: f64 = 0.3429999999999999;
const POW_0_7_2: f64 = 0.48999999999999994;
const POW_0_3_2: f64 = 0.09;
const POW_0_3_3: f64 = 0.027;

/// `getArrowheadPoints`.
fn arrowhead_points(
    e: &Element,
    shape: &Drawable,
    start: bool,
    arrowhead: &str,
) -> Option<Vec<f64>> {
    let points = &e.linear.as_ref()?.points;
    let ops = curve_path_ops(shape);
    if ops.is_empty() {
        return None;
    }
    let index = if start { 1 } else { ops.len() - 1 };
    let Some(Op::BCurveTo(data)) = ops.get(index) else {
        // `invariant(data.length === 6)` throws.
        return None;
    };
    let p3 = [data[4], data[5]];
    let p2 = [data[2], data[3]];
    let p1 = [data[0], data[1]];
    let p0 = match ops[index - 1] {
        Op::Move(p) => p,
        Op::BCurveTo(d) => [d[4], d[5]],
        Op::LineTo(_) => [0.0, 0.0],
    };
    // `Math.pow(1 - t, 3) * p3 + 3 * t * Math.pow(1 - t, 2) * p2
    //  + 3 * Math.pow(t, 2) * (1 - t) * p1 + p0 * Math.pow(t, 3)` at t = 0.3.
    let t = 0.3;
    let equation = |i: usize| {
        POW_0_7_3 * p3[i]
            + 3.0 * t * POW_0_7_2 * p2[i]
            + 3.0 * POW_0_3_2 * (1.0 - t) * p1[i]
            + p0[i] * POW_0_3_3
    };
    let [x2, y2] = if start { p0 } else { p3 };
    let (x1, y1) = (equation(0), equation(1));
    let distance = jsmath::hypot(&[x2 - x1, y2 - y1]);
    let nx = (x2 - x1) / distance;
    let ny = (y2 - y1) / distance;
    let size = arrowhead_size(arrowhead);
    let length = {
        let n = points.len();
        let [cx, cy] = if start { points[0] } else { points[n - 1] };
        let [px, py] = if n > 1 {
            if start {
                points[1]
            } else {
                points[n - 2]
            }
        } else {
            [0.0, 0.0]
        };
        jsmath::hypot(&[cx - px, cy - py])
    };
    let length_multiplier = if matches!(arrowhead, "diamond" | "diamond_outline") {
        0.25
    } else {
        0.5
    };
    let min_size = jsmath::min(size, length * length_multiplier);
    let xs = x2 - nx * min_size;
    let ys = y2 - ny * min_size;
    if matches!(arrowhead, "dot" | "circle" | "circle_outline") {
        let diameter = jsmath::hypot(&[ys - y2, xs - x2]) + e.stroke_width - 2.0;
        return Some(vec![x2, y2, diameter]);
    }
    let angle = arrowhead_angle(arrowhead);
    if matches!(arrowhead, "crowfoot_many" | "crowfoot_one_or_many") {
        let [x3, y3] = rotate([x2, y2], [xs, ys], degrees_to_radians(-angle));
        let [x4, y4] = rotate([x2, y2], [xs, ys], degrees_to_radians(angle));
        return Some(vec![xs, ys, x3, y3, x4, y4]);
    }
    let [x3, y3] = rotate([xs, ys], [x2, y2], -angle * std::f64::consts::PI / 180.0);
    let [x4, y4] = rotate([xs, ys], [x2, y2], degrees_to_radians(angle));
    if matches!(arrowhead, "diamond" | "diamond_outline") {
        let n = points.len();
        let (ox, oy);
        if start {
            let [px, py] = if n > 1 { points[1] } else { [0.0, 0.0] };
            [ox, oy] = rotate(
                [x2 + min_size * 2.0, y2],
                [x2, y2],
                jsmath::atan2(py - y2, px - x2),
            );
        } else {
            let [px, py] = if n > 1 { points[n - 2] } else { [0.0, 0.0] };
            [ox, oy] = rotate(
                [x2 - min_size * 2.0, y2],
                [x2, y2],
                jsmath::atan2(y2 - py, x2 - px),
            );
        }
        return Some(vec![x2, y2, x3, y3, ox, oy, x4, y4]);
    }
    Some(vec![x2, y2, x3, y3, x4, y4])
}

/// `getArrowheadShapes`. `options` is the arrow's own options object, which
/// the original mutates (dropping the dash, capping roughness), and those
/// changes carry over to the end arrowhead: so it is `&mut` here too.
fn arrowhead_shapes(
    e: &Element,
    main: &Drawable,
    start: bool,
    arrowhead: &str,
    options: &mut Options,
    canvas_background: &str,
) -> Vec<Drawable> {
    let Some(pts) = arrowhead_points(e, main, start, arrowhead) else {
        return Vec::new();
    };
    let crowfoot_one = |pts: Option<Vec<f64>>, o: &Options| -> Vec<Drawable> {
        match pts {
            Some(p) => vec![Drawable::line(p[2], p[3], p[4], p[5], o)],
            None => Vec::new(),
        }
    };
    match arrowhead {
        "dot" | "circle" | "circle_outline" => {
            options.stroke_line_dash = None;
            let o = Options {
                fill: Some(if arrowhead == "circle_outline" {
                    canvas_background.into()
                } else {
                    e.stroke_color.clone()
                }),
                fill_style: FillStyle::Solid,
                stroke: e.stroke_color.clone(),
                roughness: jsmath::min(0.5, or_zero(options.roughness)),
                ..options.clone()
            };
            vec![Drawable::circle(pts[0], pts[1], pts[2], &o)]
        }
        "triangle" | "triangle_outline" => {
            options.stroke_line_dash = None;
            let o = Options {
                fill: Some(if arrowhead == "triangle_outline" {
                    canvas_background.into()
                } else {
                    e.stroke_color.clone()
                }),
                fill_style: FillStyle::Solid,
                roughness: jsmath::min(1.0, or_zero(options.roughness)),
                ..options.clone()
            };
            let poly = [
                [pts[0], pts[1]],
                [pts[2], pts[3]],
                [pts[4], pts[5]],
                [pts[0], pts[1]],
            ];
            vec![Drawable::polygon(&poly, &o)]
        }
        "diamond" | "diamond_outline" => {
            options.stroke_line_dash = None;
            let o = Options {
                fill: Some(if arrowhead == "diamond_outline" {
                    canvas_background.into()
                } else {
                    e.stroke_color.clone()
                }),
                fill_style: FillStyle::Solid,
                roughness: jsmath::min(1.0, or_zero(options.roughness)),
                ..options.clone()
            };
            let poly = [
                [pts[0], pts[1]],
                [pts[2], pts[3]],
                [pts[4], pts[5]],
                [pts[6], pts[7]],
                [pts[0], pts[1]],
            ];
            vec![Drawable::polygon(&poly, &o)]
        }
        "crowfoot_one" => crowfoot_one(Some(pts), options),
        // "bar", "arrow", "crowfoot_many", "crowfoot_one_or_many" and anything unknown.
        _ => {
            let [x2, y2, x3, y3, x4, y4] = [pts[0], pts[1], pts[2], pts[3], pts[4], pts[5]];
            if e.stroke_style == "dotted" {
                let dash = [1.5, 6.0 + (e.stroke_width - 1.0)];
                options.stroke_line_dash = Some(vec![dash[0], dash[1] - 1.0]);
            } else {
                options.stroke_line_dash = None;
            }
            options.roughness = jsmath::min(1.0, or_zero(options.roughness));
            let mut shapes = vec![
                Drawable::line(x3, y3, x2, y2, options),
                Drawable::line(x4, y4, x2, y2, options),
            ];
            if arrowhead == "crowfoot_one_or_many" {
                shapes.extend(crowfoot_one(
                    arrowhead_points(e, main, start, "crowfoot_one"),
                    options,
                ));
            }
            shapes
        }
    }
}

fn heading_is_horizontal(p: Point, o: Point) -> bool {
    // `vectorToHeading(vectorFromPoint(p, o))` is right or left.
    let [x, y] = [p[0] - o[0], p[1] - o[1]];
    x > y.abs() || x <= -y.abs()
}

/// `generateElbowArrowShape`.
fn elbow_arrow_path(points: &[Point], radius: f64) -> String {
    let mut sub = Vec::new();
    for i in 1..points.len().saturating_sub(1) {
        let prev = points[i - 1];
        let next = points[i + 1];
        let point = points[i];
        let prev_horizontal = heading_is_horizontal(point, prev);
        let next_horizontal = heading_is_horizontal(next, point);
        let corner = jsmath::min(
            radius,
            jsmath::min(
                point_distance(points[i], next) / 2.0,
                point_distance(points[i], prev) / 2.0,
            ),
        );
        if prev_horizontal {
            sub.push(if prev[0] < point[0] {
                [point[0] - corner, point[1]]
            } else {
                [point[0] + corner, point[1]]
            });
        } else if prev[1] < point[1] {
            sub.push([point[0], point[1] - corner]);
        } else {
            sub.push([point[0], point[1] + corner]);
        }
        sub.push(point);
        if next_horizontal {
            sub.push(if next[0] < point[0] {
                [point[0] - corner, point[1]]
            } else {
                [point[0] + corner, point[1]]
            });
        } else if next[1] < point[1] {
            sub.push([point[0], point[1] - corner]);
        } else {
            sub.push([point[0], point[1] + corner]);
        }
    }
    let n = js_number;
    let mut d = vec![format!("M {} {}", n(points[0][0]), n(points[0][1]))];
    for c in sub.chunks(3) {
        if let [a, b, c] = c {
            d.push(format!("L {} {}", n(a[0]), n(a[1])));
            d.push(format!(
                "Q {} {}, {} {}",
                n(b[0]),
                n(b[1]),
                n(c[0]),
                n(c[1])
            ));
        }
    }
    let last = points[points.len() - 1];
    d.push(format!("L {} {}", n(last[0]), n(last[1])));
    d.join(" ")
}

/// `_generateElementShape`: everything rough.js draws for one element, in
/// order. Empty for text, images and frames, which are not rough shapes.
pub fn element_shapes(e: &Element, canvas_background: &str) -> Vec<Drawable> {
    let n = js_number;
    match e.kind {
        Kind::Rectangle | Kind::Iframe | Kind::Embeddable => {
            let el = iframe_like(e);
            if el.roundness.is_some() {
                let (w, h) = (el.width, el.height);
                let r = corner_radius(jsmath::min(w, h), &el);
                let d = format!(
                    "M {r} 0 L {wr} 0 Q {w} 0, {w} {r} L {w} {hr} Q {w} {h}, {wr} {h} L {r} {h} Q 0 {h}, 0 {hr} L 0 {r} Q 0 0, {r} 0",
                    r = n(r),
                    w = n(w),
                    h = n(h),
                    wr = n(w - r),
                    hr = n(h - r),
                );
                vec![Drawable::path(&d, &rough_options(&el, true))]
            } else {
                vec![Drawable::rectangle(
                    0.0,
                    0.0,
                    el.width,
                    el.height,
                    &rough_options(&el, false),
                )]
            }
        }
        Kind::Diamond => {
            let [top_x, top_y, right_x, right_y, bottom_x, bottom_y, left_x, left_y] =
                diamond_points(e);
            if e.roundness.is_some() {
                let v = corner_radius((top_x - left_x).abs(), e);
                let h = corner_radius((right_y - top_y).abs(), e);
                let d = format!(
                    "M {} {} L {} {} C {} {}, {} {}, {} {} L {} {} C {} {}, {} {}, {} {} L {} {} C {} {}, {} {}, {} {} L {} {} C {} {}, {} {}, {} {}",
                    n(top_x + v), n(top_y + h), n(right_x - v), n(right_y - h),
                    n(right_x), n(right_y), n(right_x), n(right_y), n(right_x - v), n(right_y + h),
                    n(bottom_x + v), n(bottom_y - h),
                    n(bottom_x), n(bottom_y), n(bottom_x), n(bottom_y), n(bottom_x - v), n(bottom_y - h),
                    n(left_x + v), n(left_y + h),
                    n(left_x), n(left_y), n(left_x), n(left_y), n(left_x + v), n(left_y - h),
                    n(top_x - v), n(top_y + h),
                    n(top_x), n(top_y), n(top_x), n(top_y), n(top_x + v), n(top_y + h),
                );
                vec![Drawable::path(&d, &rough_options(e, true))]
            } else {
                let poly = [
                    [top_x, top_y],
                    [right_x, right_y],
                    [bottom_x, bottom_y],
                    [left_x, left_y],
                ];
                vec![Drawable::polygon(&poly, &rough_options(e, false))]
            }
        }
        Kind::Ellipse => vec![Drawable::ellipse(
            e.width / 2.0,
            e.height / 2.0,
            e.width,
            e.height,
            &rough_options(e, false),
        )],
        Kind::Line | Kind::Arrow => {
            let Some(linear) = &e.linear else {
                return Vec::new();
            };
            let mut options = rough_options(e, false);
            let points: Vec<Point> = if linear.points.is_empty() {
                vec![[0.0, 0.0]]
            } else {
                linear.points.clone()
            };
            let main = if linear.elbowed {
                if !points
                    .iter()
                    .all(|p| p[0].abs() <= 1e6 && p[1].abs() <= 1e6)
                {
                    return Vec::new();
                }
                Drawable::path(&elbow_arrow_path(&points, 16.0), &rough_options(e, true))
            } else if e.roundness.is_none() {
                if options.has_fill() {
                    Drawable::polygon(&points, &options)
                } else {
                    Drawable::linear_path(&points, &options)
                }
            } else {
                Drawable::curve(&points, &options)
            };
            let mut shapes = vec![main];
            if e.kind == Kind::Arrow {
                if let Some(head) = &linear.start_arrowhead {
                    let heads = arrowhead_shapes(
                        e,
                        &shapes[0],
                        true,
                        head,
                        &mut options,
                        canvas_background,
                    );
                    shapes.extend(heads);
                }
                if let Some(head) = &linear.end_arrowhead {
                    let heads = arrowhead_shapes(
                        e,
                        &shapes[0],
                        false,
                        head,
                        &mut options,
                        canvas_background,
                    );
                    shapes.extend(heads);
                }
            }
            shapes
        }
        Kind::FreeDraw => {
            let Some(fd) = &e.free_draw else {
                return Vec::new();
            };
            if is_path_a_loop(&fd.points) {
                let simplified = rough::points::simplify(&fd.points, 0.75);
                let o = Options {
                    stroke: NONE.into(),
                    ..rough_options(e, false)
                };
                vec![Drawable::curve(&simplified, &o)]
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}
