//! An Excalidraw element as paint operations, in the element's own
//! coordinates, plus where those sit in the scene.
//!
//! This is what Excalidraw's `renderElement` and `drawElementOnCanvas` do,
//! with the canvas calls left as data, so the iced canvas and the SVG and PNG
//! exporters (phase 6) all draw from the same thing. Preparing an element is
//! the expensive part, so callers keep the result keyed by element id and
//! `version`, as Excalidraw's `ShapeCache` does.

#![forbid(unsafe_code)]

pub mod color;
pub mod freehand;
pub mod shape;

pub use color::Rgba;
use rough::{jsmath, Drawable, Op, OpSetType, Point};
use scene::{Element, Kind, Scene};

/// One step of a path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Move(Point),
    Line(Point),
    Cubic(Point, Point, Point),
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// Round caps and joins, as Excalidraw sets for every shape.
    Stroke {
        path: Vec<Seg>,
        color: Rgba,
        width: f64,
        dash: Option<Vec<f64>>,
    },
    Fill {
        path: Vec<Seg>,
        color: Rgba,
        even_odd: bool,
    },
    Text(TextItem),
    Image {
        file_id: Option<String>,
        width: f64,
        height: f64,
        crop: Option<[f64; 6]>,
        corner_radius: f64,
    },
    /// Drawn at a fixed size on screen, whatever the zoom, so the backend
    /// sizes the outline and the name itself.
    Frame {
        width: f64,
        height: f64,
        name: Option<String>,
        magic: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextItem {
    pub lines: Vec<String>,
    pub font_family: f64,
    pub font_size: f64,
    /// `fontSize * lineHeight`: the distance between baselines.
    pub line_height_px: f64,
    /// Where the first baseline is, below the element's top.
    pub baseline: f64,
    /// The x each line is aligned to.
    pub x: f64,
    pub align: Align,
    pub color: Rgba,
}

#[derive(Clone, Debug)]
pub struct Prepared {
    pub items: Vec<Item>,
    /// The element's `x`, `y`: where its local origin is in the scene.
    pub origin: Point,
    /// The local point it rotates about.
    pub center: Point,
    pub angle: f64,
    /// An image's `scale`: -1 flips it about `center`.
    pub flip: [f64; 2],
    /// 0–1, with the containing frame's opacity folded in.
    pub opacity: f64,
    /// The scene-space box everything drawn falls inside, for culling.
    pub bounds: [f64; 4],
    /// The frame this element is clipped to, if any.
    pub clip: Option<[f64; 4]>,
}

impl Prepared {
    /// Where a local point lands in the scene.
    pub fn to_scene(&self, p: Point) -> Point {
        let (s, c) = self.angle.sin_cos();
        let lx = self.center[0] + (p[0] - self.center[0]) * self.flip[0];
        let ly = self.center[1] + (p[1] - self.center[1]) * self.flip[1];
        let dx = lx - self.center[0];
        let dy = ly - self.center[1];
        [
            self.origin[0] + self.center[0] + dx * c - dy * s,
            self.origin[1] + self.center[1] + dx * s + dy * c,
        ]
    }
}

fn ops_to_path(ops: &[Op]) -> Vec<Seg> {
    ops.iter()
        .map(|op| match *op {
            Op::Move(p) => Seg::Move(p),
            Op::LineTo(p) => Seg::Line(p),
            Op::BCurveTo(d) => Seg::Cubic([d[0], d[1]], [d[2], d[3]], [d[4], d[5]]),
        })
        .collect()
}

/// rough.js's canvas `draw`: each op set stroked or filled as its type says.
fn draw(drawable: &Drawable, items: &mut Vec<Item>) {
    let o = &drawable.options;
    for set in &drawable.sets {
        match set.kind {
            OpSetType::Path => {
                if o.stroke == rough::NONE {
                    continue;
                }
                items.push(Item::Stroke {
                    path: ops_to_path(&set.ops),
                    color: color::parse(&o.stroke),
                    width: o.stroke_width,
                    dash: o.stroke_line_dash.clone(),
                });
            }
            OpSetType::FillPath => items.push(Item::Fill {
                path: ops_to_path(&set.ops),
                color: color::parse(o.fill.as_deref().unwrap_or("")),
                even_odd: drawable.shape.even_odd(),
            }),
            OpSetType::FillSketch => items.push(Item::Stroke {
                path: ops_to_path(&set.ops),
                color: color::parse(o.fill.as_deref().unwrap_or("")),
                width: o.fill_sketch_width(),
                dash: o.fill_line_dash.clone(),
            }),
        }
    }
}

/// SVG path data (absolute `M L C Z` after normalising) as segments.
fn svg_to_path(d: &str) -> Vec<Seg> {
    rough::path::segments(d)
        .into_iter()
        .filter_map(|s| match s.key {
            'M' => Some(Seg::Move([s.data[0], s.data[1]])),
            'L' => Some(Seg::Line([s.data[0], s.data[1]])),
            'C' => Some(Seg::Cubic(
                [s.data[0], s.data[1]],
                [s.data[2], s.data[3]],
                [s.data[4], s.data[5]],
            )),
            'Z' => Some(Seg::Close),
            _ => None,
        })
        .collect()
}

/// `FONT_METADATA[family].metrics`: units per em, ascender, descender.
/// Unknown families fall back to Virgil's, as `getVerticalOffset` does.
pub fn font_metrics(family: f64) -> (f64, f64, f64) {
    match family as i64 {
        2 => (2048.0, 1577.0, -471.0),
        3 => (2048.0, 1900.0, -480.0),
        6 => (1000.0, 1011.0, -353.0),
        7 => (1000.0, 923.0, -220.0),
        8 => (1000.0, 750.0, -250.0),
        9 => (2048.0, 1854.0, -434.0),
        _ => (1000.0, 886.0, -374.0),
    }
}

/// `getVerticalOffset`: the first baseline, below the top of the line box.
fn vertical_offset(family: f64, font_size: f64, line_height_px: f64) -> f64 {
    let (units_per_em, ascender, descender) = font_metrics(family);
    let font_size_em = font_size / units_per_em;
    let line_gap = (line_height_px - font_size_em * ascender + font_size_em * descender) / 2.0;
    font_size_em * ascender + line_gap
}

fn local_bounds(e: &Element) -> [f64; 4] {
    let pts = match (&e.linear, &e.free_draw) {
        (Some(l), _) => Some(&l.points),
        (_, Some(f)) => Some(&f.points),
        _ => None,
    };
    match pts {
        Some(points) if !points.is_empty() => {
            let mut b = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for p in points {
                b = [
                    b[0].min(p[0]),
                    b[1].min(p[1]),
                    b[2].max(p[0]),
                    b[3].max(p[1]),
                ];
            }
            b
        }
        _ => [0.0, 0.0, e.width, e.height],
    }
}

/// Everything Excalidraw draws for `e`. `canvas_background` fills the
/// "outline" arrowheads, as the canvas colour does in Excalidraw.
pub fn prepare(e: &Element, scene: &Scene, canvas_background: &str) -> Prepared {
    let mut items = Vec::new();
    match e.kind {
        Kind::Text => {
            if let Some(t) = &e.text {
                let line_height_px = t.font_size * t.line_height;
                let (x, align) = match t.text_align.as_str() {
                    "center" => (e.width / 2.0, Align::Center),
                    "right" => (e.width, Align::Right),
                    _ => (0.0, Align::Left),
                };
                items.push(Item::Text(TextItem {
                    // `.replace(/\r\n?/g, "\n").split("\n")`
                    lines: t
                        .text
                        .replace("\r\n", "\n")
                        .replace('\r', "\n")
                        .split('\n')
                        .map(str::to_owned)
                        .collect(),
                    font_family: t.font_family,
                    font_size: t.font_size,
                    line_height_px,
                    baseline: vertical_offset(t.font_family, t.font_size, line_height_px),
                    x,
                    align,
                    color: color::parse(&e.stroke_color),
                }));
            }
        }
        Kind::Image => {
            if let Some(img) = &e.image {
                items.push(Item::Image {
                    file_id: img.file_id.clone(),
                    width: e.width,
                    height: e.height,
                    crop: img.crop,
                    corner_radius: if e.roundness.is_some() {
                        shape::corner_radius(jsmath::min(e.width, e.height), e)
                    } else {
                        0.0
                    },
                });
            }
        }
        Kind::Frame | Kind::MagicFrame => items.push(Item::Frame {
            width: e.width,
            height: e.height,
            name: e.frame_name.clone(),
            magic: e.kind == Kind::MagicFrame,
        }),
        Kind::FreeDraw => {
            // The loop's fill first, then the outline, filled with the stroke colour.
            for d in shape::element_shapes(e, canvas_background) {
                draw(&d, &mut items);
            }
            if let Some(fd) = &e.free_draw {
                items.push(Item::Fill {
                    path: svg_to_path(&freehand::svg_path(fd, e.stroke_width)),
                    color: color::parse(&e.stroke_color),
                    even_odd: false,
                });
            }
        }
        Kind::Unknown => {}
        _ => {
            for d in shape::element_shapes(e, canvas_background) {
                draw(&d, &mut items);
            }
        }
    }

    let lb = local_bounds(e);
    let center = [(lb[0] + lb[2]) / 2.0, (lb[1] + lb[3]) / 2.0];
    let frame = e
        .frame_id
        .as_deref()
        .and_then(|id| scene.by_id(id))
        .filter(|f| matches!(f.kind, Kind::Frame | Kind::MagicFrame));
    let mut prepared = Prepared {
        items,
        origin: [e.x, e.y],
        center,
        angle: e.angle,
        flip: e.image.as_ref().map_or([1.0, 1.0], |i| i.scale),
        opacity: frame.map_or(100.0, |f| f.opacity) * e.opacity / 10000.0,
        bounds: [0.0; 4],
        clip: frame.map(|f| [f.x, f.y, f.x + f.width, f.y + f.height]),
    };
    // Rough strokes wander a few pixels outside the shape, arrowheads and
    // freehand outlines further; the margin only has to be generous.
    let margin = e.stroke_width * 4.0 + 8.0 + if e.kind == Kind::Arrow { 30.0 } else { 0.0 };
    let corners = [
        [lb[0], lb[1]],
        [lb[2], lb[1]],
        [lb[2], lb[3]],
        [lb[0], lb[3]],
    ]
    .map(|p| prepared.to_scene(p));
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in corners {
        b = [
            b[0].min(p[0]),
            b[1].min(p[1]),
            b[2].max(p[0]),
            b[3].max(p[1]),
        ];
    }
    prepared.bounds = [b[0] - margin, b[1] - margin, b[2] + margin, b[3] + margin];
    prepared
}
