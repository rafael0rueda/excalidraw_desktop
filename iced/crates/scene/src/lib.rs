//! The `.excalidraw` file format.
//!
//! A drawing is kept twice: as the JSON it was read from, key order and all,
//! and as a typed view of each element, built the way Excalidraw's own
//! `restoreElement` (0.18.1) builds one: same defaults, same migrations of
//! older files. The renderer reads the view; saving writes the JSON. So a
//! field this app does not know about, or one a newer Excalidraw adds, goes
//! back to disk untouched (docs/iced-port/PLAN.md, W7).

#![forbid(unsafe_code)]

mod element;

pub use element::{
    default_line_height, lines, Arrowhead, Element, FreeDraw, Image, Kind, Linear, Roundness, Text,
};
use serde_json::{Map, Value};

/// A drawing larger than this is refused rather than laid out. Generous —
/// Excalidraw itself slows to a crawl long before — but a hostile file
/// cannot ask for unbounded work.
pub const MAX_ELEMENTS: usize = 500_000;
/// Points in one line, arrow or freehand stroke.
pub const MAX_POINTS: usize = 200_000;

#[derive(Clone, Debug)]
pub struct Scene {
    /// Everything but `elements`, as read.
    raw: Map<String, Value>,
    pub elements: Vec<Element>,
}

#[derive(Debug, PartialEq)]
pub enum Error {
    Json(String),
    NotADrawing,
    TooLarge(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Json(e) => write!(f, "not valid JSON: {e}"),
            Error::NotADrawing => write!(f, "not an Excalidraw drawing"),
            Error::TooLarge(what) => write!(f, "too large to open: {what}"),
        }
    }
}

impl std::error::Error for Error {}

impl Scene {
    pub fn parse(text: &str) -> Result<Self, Error> {
        let value: Value = serde_json::from_str(text).map_err(|e| Error::Json(e.to_string()))?;
        let Value::Object(mut raw) = value else {
            return Err(Error::NotADrawing);
        };
        // Excalidraw's own check (`isValidExcalidrawData`): `type` is
        // "excalidraw", or absent with an `elements` array.
        let kind_ok = match raw.get("type") {
            Some(Value::String(t)) => t == "excalidraw",
            None => raw.get("elements").is_some_and(Value::is_array),
            _ => false,
        };
        if !kind_ok {
            return Err(Error::NotADrawing);
        }
        let elements = match raw.get_mut("elements") {
            Some(Value::Array(items)) => std::mem::take(items),
            None => Vec::new(),
            Some(_) => return Err(Error::NotADrawing),
        };
        if elements.len() > MAX_ELEMENTS {
            return Err(Error::TooLarge(format!(
                "{} elements, limit {MAX_ELEMENTS}",
                elements.len()
            )));
        }
        let mut out = Vec::with_capacity(elements.len());
        for item in elements {
            // Not an object: Excalidraw drops it on load, and so do we. An
            // object of a type it does not know is kept, as `Kind::Unknown`,
            // so that saving does not lose it.
            let Value::Object(map) = item else { continue };
            out.push(Element::restore(map)?);
        }
        Ok(Self { raw, elements: out })
    }

    /// The drawing as Excalidraw writes it: `JSON.stringify(data, null, 2)`.
    pub fn to_json(&self) -> String {
        let mut raw = self.raw.clone();
        let elements = self
            .elements
            .iter()
            .map(|e| Value::Object(e.raw().clone()))
            .collect();
        // Back where it was read from, or after the keys Excalidraw puts
        // before it in a file it writes itself.
        match raw.get_mut("elements") {
            Some(slot) => *slot = Value::Array(elements),
            None => {
                raw.insert("elements".into(), Value::Array(elements));
            }
        }
        to_js_json(&Value::Object(raw))
    }

    /// The canvas colour the drawing was saved with, if any.
    pub fn view_background_color(&self) -> Option<&str> {
        self.raw
            .get("appState")?
            .get("viewBackgroundColor")?
            .as_str()
    }

    /// An embedded image by `fileId`: its MIME type and `data:` URL.
    pub fn file(&self, id: &str) -> Option<(&str, &str)> {
        let file = self.raw.get("files")?.get(id)?;
        Some((
            file.get("mimeType")?.as_str()?,
            file.get("dataURL")?.as_str()?,
        ))
    }

    /// Elements that are drawn: not deleted, in the order they are stacked.
    pub fn visible(&self) -> impl Iterator<Item = &Element> {
        self.elements.iter().filter(|e| !e.is_deleted)
    }

    pub fn by_id(&self, id: &str) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == id)
    }
}

/// `JSON.stringify(value, null, 2)`: two-space indentation, and numbers as
/// JavaScript spells them.
pub fn to_js_json(value: &Value) -> String {
    use serde::Serialize;
    let mut out = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(
        &mut out,
        JsFormatter(serde_json::ser::PrettyFormatter::new()),
    );
    // Writing to a Vec cannot fail, and a Value is always serialisable.
    let _ = value.serialize(&mut ser);
    String::from_utf8(out).unwrap_or_default()
}

struct JsFormatter<'a>(serde_json::ser::PrettyFormatter<'a>);

macro_rules! delegate {
    ($($name:ident($($arg:ident: $ty:ty),*);)*) => {$(
        fn $name<W: ?Sized + std::io::Write>(&mut self, w: &mut W $(, $arg: $ty)*) -> std::io::Result<()> {
            serde_json::ser::Formatter::$name(&mut self.0, w $(, $arg)*)
        }
    )*};
}

impl serde_json::ser::Formatter for JsFormatter<'_> {
    fn write_f64<W: ?Sized + std::io::Write>(
        &mut self,
        w: &mut W,
        value: f64,
    ) -> std::io::Result<()> {
        // `JSON.stringify` writes NaN and the infinities as null.
        if !value.is_finite() {
            return w.write_all(b"null");
        }
        w.write_all(ryu_js::Buffer::new().format_finite(value).as_bytes())
    }

    delegate! {
        begin_array();
        end_array();
        begin_array_value(first: bool);
        end_array_value();
        begin_object();
        end_object();
        begin_object_key(first: bool);
        end_object_key();
        begin_object_value();
        end_object_value();
    }
}
