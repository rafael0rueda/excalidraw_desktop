//! A port of [rough.js] 4.6.4, the library Excalidraw draws every shape with.
//!
//! The goal is *the same strokes*, not similar ones: the same seed and options
//! must give the same numbers rough.js gives, so a drawing made in Excalidraw
//! wobbles identically here (docs/iced-port/PLAN.md, decision 3). That rules
//! out `roughr`, which draws from a different random number generator. This
//! port keeps rough.js's structure function for function, evaluates every
//! expression in the same order, and calls random numbers in the same order;
//! `tests/rough_js.rs` holds it to rough.js's own output.
//!
//! Only what Excalidraw uses is here: no `arc`, and no `dots`, `dashed` or
//! `zigzag-line` fills.
//!
//! [rough.js]: https://github.com/rough-stuff/rough
//! Ported from rough.js and its helpers hachure-fill, points-on-curve,
//! points-on-path and path-data-parser, all MIT, (c) Preet Shihn.

#![forbid(unsafe_code)]

mod fdlibm;
mod fillers;
mod generator;
pub mod jsmath;
pub mod path;
pub mod points;
mod random;
mod renderer;

pub use generator::{Drawable, Shape};
pub use random::Random;

pub type Point = [f64; 2];

/// rough.js's `'none'`: no stroke, or no fill.
pub const NONE: &str = "none";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Move(Point),
    LineTo(Point),
    BCurveTo([f64; 6]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpSetType {
    /// Stroked with the stroke colour and width.
    Path,
    /// Filled with the fill colour.
    FillPath,
    /// Hachure lines, stroked with the fill colour at the fill weight.
    FillSketch,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OpSet {
    pub kind: OpSetType,
    pub ops: Vec<Op>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FillStyle {
    #[default]
    Hachure,
    CrossHatch,
    Zigzag,
    Solid,
}

impl FillStyle {
    /// rough.js falls back to hachure for any name it does not know.
    pub fn from_name(name: &str) -> Self {
        match name {
            "cross-hatch" => Self::CrossHatch,
            "zigzag" => Self::Zigzag,
            "solid" => Self::Solid,
            _ => Self::Hachure,
        }
    }
}

/// rough.js's `ResolvedOptions`, defaults included.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub max_randomness_offset: f64,
    pub roughness: f64,
    pub bowing: f64,
    pub stroke: String,
    pub stroke_width: f64,
    pub curve_fitting: f64,
    pub curve_tightness: f64,
    pub curve_step_count: f64,
    pub fill: Option<String>,
    pub fill_style: FillStyle,
    pub fill_weight: f64,
    pub hachure_angle: f64,
    pub hachure_gap: f64,
    pub simplification: Option<f64>,
    pub seed: f64,
    pub stroke_line_dash: Option<Vec<f64>>,
    pub stroke_line_dash_offset: Option<f64>,
    pub fill_line_dash: Option<Vec<f64>>,
    pub fill_line_dash_offset: Option<f64>,
    pub disable_multi_stroke: bool,
    pub disable_multi_stroke_fill: bool,
    pub preserve_vertices: bool,
    pub fill_shape_roughness_gain: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_randomness_offset: 2.0,
            roughness: 1.0,
            bowing: 1.0,
            stroke: "#000".into(),
            stroke_width: 1.0,
            curve_tightness: 0.0,
            curve_fitting: 0.95,
            curve_step_count: 9.0,
            fill: None,
            fill_style: FillStyle::Hachure,
            fill_weight: -1.0,
            hachure_angle: -41.0,
            hachure_gap: -1.0,
            simplification: None,
            seed: 0.0,
            stroke_line_dash: None,
            stroke_line_dash_offset: None,
            fill_line_dash: None,
            fill_line_dash_offset: None,
            disable_multi_stroke: false,
            disable_multi_stroke_fill: false,
            preserve_vertices: false,
            fill_shape_roughness_gain: 0.8,
        }
    }
}

impl Options {
    /// JS truthiness of `o.fill`.
    pub(crate) fn has_fill(&self) -> bool {
        self.fill.as_deref().is_some_and(|f| !f.is_empty())
    }

    pub(crate) fn has_stroke(&self) -> bool {
        self.stroke != NONE
    }

    /// The width hachure lines are stroked at: `fillWeight`, or half the
    /// stroke width when that is unset.
    pub fn fill_sketch_width(&self) -> f64 {
        if self.fill_weight < 0.0 {
            self.stroke_width / 2.0
        } else {
            self.fill_weight
        }
    }
}

/// JS truthiness of a number.
pub(crate) fn truthy(x: f64) -> bool {
    x != 0.0 && !x.is_nan()
}
