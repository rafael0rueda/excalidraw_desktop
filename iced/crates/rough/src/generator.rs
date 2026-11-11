//! rough.js's `RoughGenerator`: shapes as sets of operations.
//!
//! Each call starts its own random generator from `options.seed`, as rough.js
//! does by copying the options it is given (`_o`) before drawing from them.

use crate::path::points_on_path;
use crate::points::{curve_to_bezier, points_on_bezier_curves};
use crate::renderer::{self, ellipse_with_params, generate_ellipse_params};
use crate::{FillStyle, Op, OpSet, OpSetType, Options, Point, Random, NONE};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Line,
    Rectangle,
    Ellipse,
    Circle,
    LinearPath,
    Curve,
    Polygon,
    Path,
}

impl Shape {
    /// The fill rule rough.js's canvas renderer uses for a `fillPath`.
    pub fn even_odd(self) -> bool {
        matches!(self, Shape::Curve | Shape::Polygon | Shape::Path)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Drawable {
    pub shape: Shape,
    pub sets: Vec<OpSet>,
    pub options: Options,
}

fn drawable(shape: Shape, sets: Vec<OpSet>, o: &Options) -> Drawable {
    Drawable {
        shape,
        sets,
        options: o.clone(),
    }
}

/// `_mergedShape`: every `move` but the first dropped, so a fill is one path.
fn merged_shape(input: Vec<Op>) -> Vec<Op> {
    input
        .into_iter()
        .enumerate()
        .filter(|(i, op)| *i == 0 || !matches!(op, Op::Move(_)))
        .map(|(_, op)| op)
        .collect()
}

/// The options a solid fill is traced with in `curve` and `path`.
fn fill_shape_options(o: &Options) -> Options {
    Options {
        disable_multi_stroke: true,
        roughness: if crate::truthy(o.roughness) {
            o.roughness + o.fill_shape_roughness_gain
        } else {
            0.0
        },
        ..o.clone()
    }
}

impl Drawable {
    pub fn line(x1: f64, y1: f64, x2: f64, y2: f64, o: &Options) -> Self {
        let mut rng = Random::new(o.seed);
        drawable(
            Shape::Line,
            vec![renderer::line(x1, y1, x2, y2, o, &mut rng)],
            o,
        )
    }

    pub fn rectangle(x: f64, y: f64, width: f64, height: f64, o: &Options) -> Self {
        let mut rng = Random::new(o.seed);
        let mut paths = Vec::new();
        let outline = renderer::rectangle(x, y, width, height, o, &mut rng);
        if o.has_fill() {
            let mut points = vec![vec![
                [x, y],
                [x + width, y],
                [x + width, y + height],
                [x, y + height],
            ]];
            if o.fill_style == FillStyle::Solid {
                paths.push(renderer::solid_fill_polygon(&points, o, &mut rng));
            } else {
                paths.push(renderer::pattern_fill_polygons(&mut points, o, &mut rng));
            }
        }
        if o.has_stroke() {
            paths.push(outline);
        }
        drawable(Shape::Rectangle, paths, o)
    }

    pub fn ellipse(x: f64, y: f64, width: f64, height: f64, o: &Options) -> Self {
        let mut rng = Random::new(o.seed);
        let mut paths = Vec::new();
        let params = generate_ellipse_params(width, height, o, &mut rng);
        let response = ellipse_with_params(x, y, o, &mut rng, &params);
        if o.has_fill() {
            if o.fill_style == FillStyle::Solid {
                let mut shape = ellipse_with_params(x, y, o, &mut rng, &params).opset;
                shape.kind = OpSetType::FillPath;
                paths.push(shape);
            } else {
                let mut polygons = vec![response.estimated_points];
                paths.push(renderer::pattern_fill_polygons(&mut polygons, o, &mut rng));
            }
        }
        if o.has_stroke() {
            paths.push(response.opset);
        }
        drawable(Shape::Ellipse, paths, o)
    }

    pub fn circle(x: f64, y: f64, diameter: f64, o: &Options) -> Self {
        let mut d = Self::ellipse(x, y, diameter, diameter, o);
        d.shape = Shape::Circle;
        d
    }

    pub fn linear_path(points: &[Point], o: &Options) -> Self {
        let mut rng = Random::new(o.seed);
        drawable(
            Shape::LinearPath,
            vec![renderer::linear_path(points, false, o, &mut rng)],
            o,
        )
    }

    pub fn curve(points: &[Point], o: &Options) -> Self {
        let mut rng = Random::new(o.seed);
        let mut paths = Vec::new();
        let outline = renderer::curve(points, o, &mut rng);
        if o.has_fill() && o.fill.as_deref() != Some(NONE) && points.len() >= 3 {
            if o.fill_style == FillStyle::Solid {
                let fill_shape = renderer::curve(points, &fill_shape_options(o), &mut rng);
                paths.push(OpSet {
                    kind: OpSetType::FillPath,
                    ops: merged_shape(fill_shape.ops),
                });
            } else if let Some(bcurve) = curve_to_bezier(points, 0.0) {
                let poly_points =
                    points_on_bezier_curves(&bcurve, 10.0, Some((1.0 + o.roughness) / 2.0));
                let mut polygons = vec![poly_points];
                paths.push(renderer::pattern_fill_polygons(&mut polygons, o, &mut rng));
            }
        }
        if o.has_stroke() {
            paths.push(outline);
        }
        drawable(Shape::Curve, paths, o)
    }

    pub fn polygon(points: &[Point], o: &Options) -> Self {
        let mut rng = Random::new(o.seed);
        let mut paths = Vec::new();
        let outline = renderer::linear_path(points, true, o, &mut rng);
        if o.has_fill() {
            let mut polygons = vec![points.to_vec()];
            if o.fill_style == FillStyle::Solid {
                paths.push(renderer::solid_fill_polygon(&polygons, o, &mut rng));
            } else {
                paths.push(renderer::pattern_fill_polygons(&mut polygons, o, &mut rng));
            }
        }
        if o.has_stroke() {
            paths.push(outline);
        }
        drawable(Shape::Polygon, paths, o)
    }

    pub fn path(d: &str, o: &Options) -> Self {
        let mut rng = Random::new(o.seed);
        let mut paths = Vec::new();
        if d.is_empty() {
            return drawable(Shape::Path, paths, o);
        }
        // `.replace(/\n/g, ' ').replace(/(-\s)/g, '-')`; the third replace is
        // of a literal string that never occurs.
        let d = d.replace('\n', " ");
        let d = replace_minus_space(&d);
        let has_fill =
            o.has_fill() && !matches!(o.fill.as_deref(), Some("transparent") | Some(NONE));
        let has_stroke = o.has_stroke();
        let simplified = o
            .simplification
            .is_some_and(|s| crate::truthy(s) && s < 1.0);
        let distance = if simplified {
            4.0 - 4.0
                * o.simplification
                    .filter(|s| crate::truthy(*s))
                    .unwrap_or(1.0)
        } else {
            (1.0 + o.roughness) / 2.0
        };
        let mut sets = points_on_path(&d, 1.0, Some(distance));
        let shape = renderer::svg_path(&d, o, &mut rng);
        if has_fill {
            if o.fill_style == FillStyle::Solid {
                if sets.len() == 1 {
                    let fill_shape = renderer::svg_path(&d, &fill_shape_options(o), &mut rng);
                    paths.push(OpSet {
                        kind: OpSetType::FillPath,
                        ops: merged_shape(fill_shape.ops),
                    });
                } else {
                    paths.push(renderer::solid_fill_polygon(&sets, o, &mut rng));
                }
            } else {
                paths.push(renderer::pattern_fill_polygons(&mut sets, o, &mut rng));
            }
        }
        if has_stroke {
            if simplified {
                for set in &sets {
                    paths.push(renderer::linear_path(set, false, o, &mut rng));
                }
            } else {
                paths.push(shape);
            }
        }
        drawable(Shape::Path, paths, o)
    }
}

/// `/(-\s)/g` → `'-'`: a minus followed by one whitespace character.
fn replace_minus_space(d: &str) -> String {
    let mut out = String::with_capacity(d.len());
    let mut chars = d.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == '-' && chars.peek().is_some_and(|n| is_js_whitespace(*n)) {
            chars.next();
        }
    }
    out
}

/// What `\s` matches in a JS regular expression.
fn is_js_whitespace(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}
