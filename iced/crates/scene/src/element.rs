//! One element: its JSON, and the view `restoreElement` would make of it.

use crate::{Error, MAX_POINTS};
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Rectangle,
    Diamond,
    Ellipse,
    Line,
    Arrow,
    FreeDraw,
    Text,
    Image,
    Frame,
    MagicFrame,
    Iframe,
    Embeddable,
    /// A type Excalidraw 0.18.1 does not know. Never drawn, always saved.
    Unknown,
}

impl Kind {
    fn from_name(name: &str) -> Self {
        match name {
            "rectangle" => Kind::Rectangle,
            "diamond" => Kind::Diamond,
            "ellipse" => Kind::Ellipse,
            // `draw` is what lines were called before 2020.
            "line" | "draw" => Kind::Line,
            "arrow" => Kind::Arrow,
            "freedraw" => Kind::FreeDraw,
            "text" => Kind::Text,
            "image" => Kind::Image,
            "frame" => Kind::Frame,
            "magicframe" => Kind::MagicFrame,
            "iframe" => Kind::Iframe,
            "embeddable" => Kind::Embeddable,
            _ => Kind::Unknown,
        }
    }

    pub fn is_linear(self) -> bool {
        matches!(self, Kind::Line | Kind::Arrow)
    }

    /// `isUsingAdaptiveRadius`.
    fn uses_adaptive_radius(self) -> bool {
        matches!(
            self,
            Kind::Rectangle | Kind::Embeddable | Kind::Iframe | Kind::Image
        )
    }
}

/// `ROUNDNESS`: 1 legacy, 2 proportional, 3 adaptive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roundness {
    pub kind: f64,
    pub value: Option<f64>,
}

pub type Arrowhead = Option<String>;

#[derive(Clone, Debug, PartialEq)]
pub struct Linear {
    pub points: Vec<[f64; 2]>,
    pub start_arrowhead: Arrowhead,
    pub end_arrowhead: Arrowhead,
    pub elbowed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FreeDraw {
    pub points: Vec<[f64; 2]>,
    pub pressures: Vec<f64>,
    pub simulate_pressure: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub text: String,
    pub font_size: f64,
    pub font_family: f64,
    pub text_align: String,
    pub vertical_align: String,
    pub container_id: Option<String>,
    pub line_height: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub file_id: Option<String>,
    pub scale: [f64; 2],
    /// `{ x, y, width, height, naturalWidth, naturalHeight }`, if cropped.
    pub crop: Option<[f64; 6]>,
}

#[derive(Clone, Debug)]
pub struct Element {
    raw: Map<String, Value>,
    pub id: String,
    pub kind: Kind,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub angle: f64,
    pub stroke_color: String,
    pub background_color: String,
    pub fill_style: String,
    pub stroke_width: f64,
    pub stroke_style: String,
    pub roughness: f64,
    pub opacity: f64,
    pub seed: f64,
    pub version: f64,
    pub roundness: Option<Roundness>,
    pub is_deleted: bool,
    pub group_ids: Vec<String>,
    pub frame_id: Option<String>,
    pub link: Option<String>,
    pub linear: Option<Linear>,
    pub free_draw: Option<FreeDraw>,
    pub text: Option<Text>,
    pub image: Option<Image>,
    pub frame_name: Option<String>,
}

// JavaScript's two ways of defaulting a property.

/// `value ?? default`: only missing and `null` take the default. A value of
/// the wrong type is treated as missing, where Excalidraw would carry it into
/// its maths as NaN.
fn nullish_num(raw: &Map<String, Value>, key: &str) -> Option<f64> {
    raw.get(key).and_then(Value::as_f64)
}

/// `value || default`: 0 and NaN take the default too.
fn truthy_num(raw: &Map<String, Value>, key: &str) -> Option<f64> {
    nullish_num(raw, key).filter(|n| *n != 0.0 && !n.is_nan())
}

/// `value || default` for a string: "" takes the default.
fn truthy_str(raw: &Map<String, Value>, key: &str) -> Option<String> {
    raw.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn nullish_str(raw: &Map<String, Value>, key: &str) -> Option<String> {
    raw.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

fn points(raw: &Map<String, Value>, key: &str) -> Result<Option<Vec<[f64; 2]>>, Error> {
    let Some(Value::Array(items)) = raw.get(key) else {
        return Ok(None);
    };
    if items.len() > MAX_POINTS {
        return Err(Error::TooLarge(format!(
            "{} points in one element, limit {MAX_POINTS}",
            items.len()
        )));
    }
    Ok(Some(
        items
            .iter()
            .map(|p| {
                let x = p.get(0).and_then(Value::as_f64).unwrap_or(f64::NAN);
                let y = p.get(1).and_then(Value::as_f64).unwrap_or(f64::NAN);
                [x, y]
            })
            .collect(),
    ))
}

/// `getSizeFromPoints`.
fn size_from_points(points: &[[f64; 2]]) -> (f64, f64) {
    let span = |i: usize| {
        let (min, max) = points
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p[i]), hi.max(p[i]))
            });
        if points.is_empty() {
            0.0
        } else {
            max - min
        }
    };
    (span(0), span(1))
}

/// `splitIntoLines(normalizeText(text))`: a text element's lines.
pub fn lines(text: &str) -> Vec<String> {
    text.replace('\t', "        ")
        .replace("\r\n", "\n")
        .split('\n')
        .map(str::to_owned)
        .collect()
}

/// `getLineHeight(fontFamily)`: Excalidraw's per-family default.
pub fn default_line_height(font_family: f64) -> f64 {
    match font_family as i64 {
        6 => 1.35,
        2 | 7 | 9 => 1.15,
        3 => 1.2,
        _ => 1.25,
    }
}

/// `getFontFamilyByName`, for the legacy `font: "20px Virgil"`.
fn font_family_by_name(name: &str) -> Option<f64> {
    Some(match name {
        "Virgil" => 1.0,
        "Helvetica" => 2.0,
        "Cascadia" => 3.0,
        "Excalifont" => 5.0,
        "Nunito" => 6.0,
        "Lilita One" => 7.0,
        "Comic Shanns" => 8.0,
        "Liberation Sans" => 9.0,
        _ => return None,
    })
}

impl Element {
    pub fn raw(&self) -> &Map<String, Value> {
        &self.raw
    }

    pub(crate) fn restore(raw: Map<String, Value>) -> Result<Self, Error> {
        let kind = raw
            .get("type")
            .and_then(Value::as_str)
            .map_or(Kind::Unknown, Kind::from_name);
        let mut x = nullish_num(&raw, "x").unwrap_or(0.0);
        let mut y = nullish_num(&raw, "y").unwrap_or(0.0);
        let mut width = truthy_num(&raw, "width").unwrap_or(0.0);
        let mut height = truthy_num(&raw, "height").unwrap_or(0.0);

        let roundness = if truthy(raw.get("roundness")) {
            let r = &raw["roundness"];
            Some(Roundness {
                kind: r.get("type").and_then(Value::as_f64).unwrap_or(f64::NAN),
                value: r.get("value").and_then(Value::as_f64),
            })
        } else if raw.get("strokeSharpness").and_then(Value::as_str) == Some("round") {
            // Old elements that would now use the adaptive radius keep the legacy one.
            Some(Roundness {
                kind: if kind.uses_adaptive_radius() {
                    1.0
                } else {
                    2.0
                },
                value: None,
            })
        } else {
            None
        };

        let mut linear = None;
        let mut free_draw = None;
        let mut text = None;
        let mut image = None;
        let mut frame_name = None;
        let mut is_deleted = raw
            .get("isDeleted")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        match kind {
            Kind::Line | Kind::Arrow => {
                let start_arrowhead = nullish_str(&raw, "startArrowhead");
                // An absent end arrowhead is an arrow on an arrow, nothing on a line;
                // an explicit `null` is nothing on both.
                let end_arrowhead = match raw.get("endArrowhead") {
                    None if kind == Kind::Arrow => Some("arrow".to_owned()),
                    other => other.and_then(Value::as_str).map(str::to_owned),
                };
                let mut pts = match points(&raw, "points")? {
                    Some(p) if p.len() >= 2 => p,
                    // The pre-2020 arrow model: two points from the size.
                    _ => vec![[0.0, 0.0], [width, height]],
                };
                // `getNormalizedPoints`: the first point at the origin.
                let [ox, oy] = pts[0];
                if ox != 0.0 || oy != 0.0 {
                    for p in &mut pts {
                        *p = [p[0] - ox, p[1] - oy];
                    }
                    x += ox;
                    y += oy;
                }
                (width, height) = size_from_points(&pts);
                linear = Some(Linear {
                    points: pts,
                    start_arrowhead,
                    end_arrowhead,
                    elbowed: kind == Kind::Arrow
                        && raw.get("elbowed").and_then(Value::as_bool).unwrap_or(false),
                });
            }
            Kind::FreeDraw => {
                free_draw = Some(FreeDraw {
                    points: points(&raw, "points")?.unwrap_or_default(),
                    pressures: match raw.get("pressures") {
                        Some(Value::Array(p)) => {
                            p.iter().map(|v| v.as_f64().unwrap_or(f64::NAN)).collect()
                        }
                        _ => Vec::new(),
                    },
                    simulate_pressure: raw
                        .get("simulatePressure")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                });
            }
            Kind::Text => {
                let mut font_size = nullish_num(&raw, "fontSize");
                let mut font_family = nullish_num(&raw, "fontFamily");
                if let Some(font) = raw.get("font").and_then(Value::as_str) {
                    let mut parts = font.splitn(2, ' ');
                    font_size = parts
                        .next()
                        .and_then(|px| px.trim_end_matches("px").parse().ok());
                    font_family = parts.next().and_then(font_family_by_name);
                }
                let content = truthy_str(&raw, "text").unwrap_or_default();
                let font_size = font_size.unwrap_or(20.0);
                let font_family = font_family.unwrap_or(1.0);
                let line_height = truthy_num(&raw, "lineHeight").unwrap_or_else(|| {
                    if height != 0.0 {
                        // `detectLineHeight`
                        height / lines(&content).len() as f64 / font_size
                    } else {
                        default_line_height(font_family)
                    }
                });
                if content.is_empty() {
                    is_deleted = true;
                }
                text = Some(Text {
                    text: content,
                    font_size,
                    font_family,
                    text_align: truthy_str(&raw, "textAlign").unwrap_or_else(|| "left".into()),
                    vertical_align: truthy_str(&raw, "verticalAlign")
                        .unwrap_or_else(|| "top".into()),
                    container_id: nullish_str(&raw, "containerId"),
                    line_height,
                });
            }
            Kind::Image => {
                let scale = match raw.get("scale") {
                    Some(Value::Array(s)) if s.len() == 2 => {
                        [s[0].as_f64().unwrap_or(1.0), s[1].as_f64().unwrap_or(1.0)]
                    }
                    _ => [1.0, 1.0],
                };
                let crop = raw.get("crop").and_then(|c| {
                    let n = |k: &str| c.get(k).and_then(Value::as_f64);
                    Some([
                        n("x")?,
                        n("y")?,
                        n("width")?,
                        n("height")?,
                        n("naturalWidth")?,
                        n("naturalHeight")?,
                    ])
                });
                image = Some(Image {
                    file_id: nullish_str(&raw, "fileId"),
                    scale,
                    crop,
                });
            }
            Kind::Frame | Kind::MagicFrame => frame_name = nullish_str(&raw, "name"),
            _ => {}
        }

        // `getNormalizedDimensions`, which linear elements' own sizes override.
        if !kind.is_linear() {
            if width < 0.0 {
                width = width.abs();
                x -= width;
            }
            if height < 0.0 {
                height = height.abs();
                y -= height;
            }
        }

        Ok(Self {
            id: nullish_str(&raw, "id").unwrap_or_default(),
            kind,
            x,
            y,
            width,
            height,
            angle: truthy_num(&raw, "angle").unwrap_or(0.0),
            stroke_color: truthy_str(&raw, "strokeColor").unwrap_or_else(|| "#1e1e1e".into()),
            background_color: truthy_str(&raw, "backgroundColor")
                .unwrap_or_else(|| "transparent".into()),
            fill_style: truthy_str(&raw, "fillStyle").unwrap_or_else(|| "solid".into()),
            stroke_width: truthy_num(&raw, "strokeWidth").unwrap_or(2.0),
            stroke_style: nullish_str(&raw, "strokeStyle").unwrap_or_else(|| "solid".into()),
            roughness: nullish_num(&raw, "roughness").unwrap_or(1.0),
            opacity: nullish_num(&raw, "opacity").unwrap_or(100.0),
            seed: nullish_num(&raw, "seed").unwrap_or(1.0),
            version: truthy_num(&raw, "version").unwrap_or(1.0),
            roundness,
            is_deleted,
            group_ids: match raw.get("groupIds") {
                Some(Value::Array(ids)) => ids
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect(),
                _ => Vec::new(),
            },
            frame_id: nullish_str(&raw, "frameId"),
            link: truthy_str(&raw, "link"),
            linear,
            free_draw,
            text,
            image,
            frame_name,
            raw,
        })
    }
}
