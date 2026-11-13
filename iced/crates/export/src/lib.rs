//! A drawing, or part of one, as an SVG or a PNG: what Excalidraw's
//! `exportToSvg` and `exportToBlob` make, and the drawing stored inside the
//! image the way Excalidraw stores it, so excalidraw.com opens it again.
//!
//! Both are drawn from `render::prepare`, the same paint operations the
//! canvas draws. The PNG is the SVG drawn by resvg, with the fonts the app
//! bundles and no others, so it comes out the same on every machine.
//!
//! An SVG written to a file carries its fonts as Excalidraw's does: a subset
//! of each font in use, inline, holding only the characters the drawing
//! needs. Unlike Excalidraw's, the subsets have no kerning (`fontmerge`).

#![forbid(unsafe_code)]

mod embed;

pub use embed::{decode_png, decode_svg};
use base64::Engine;
use render::{Align, Item, Prepared, Rgba, Seg};
use scene::{Element, Kind, Scene};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::Arc;

/// `DEFAULT_EXPORT_PADDING`.
pub const PADDING: f64 = 10.0;
/// The largest PNG made, in pixels a side and in all. A browser's canvas
/// stops at about the same size.
pub const MAX_SIDE: u32 = 16_384;
pub const MAX_PIXELS: u64 = 120_000_000;

/// A bundled font: the family name in the file, and the file.
#[derive(Clone, Copy)]
pub struct Font {
    pub family: &'static str,
    pub data: &'static [u8],
}

/// What to export.
pub struct Export<'a> {
    pub scene: &'a Scene,
    /// These elements only (a selection), or all of them.
    pub only: Option<&'a HashSet<String>>,
    /// The canvas colour: the background, unless `transparent`, and the
    /// fill of outline arrowheads either way.
    pub canvas: &'a str,
    pub transparent: bool,
    /// The bundled font for an Excalidraw font family id.
    pub font: &'a dyn Fn(f64) -> Option<Font>,
}

#[derive(Debug, PartialEq)]
pub enum Error {
    Empty,
    TooLarge { width: u32, height: u32 },
    Render(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Empty => write!(f, "Nothing to export."),
            Error::TooLarge { width, height } => write!(
                f,
                "The image would be {width} × {height} pixels, more than can be made. Export at a smaller scale, or a part of the drawing."
            ),
            Error::Render(e) => write!(f, "Could not draw the image: {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// Who the SVG is for: a file (Excalidraw's font names, fonts inline,
/// metadata), or resvg (the names the bundled fonts are loaded under).
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    File,
    Raster,
}

/// `FONT_FAMILY`, and `getFontFamilyString`'s fallbacks.
fn excalidraw_family(id: f64) -> (Option<&'static str>, &'static str) {
    match id as i64 {
        1 => (Some("Virgil"), "Virgil, Segoe UI Emoji"),
        2 => (None, "Helvetica, Segoe UI Emoji"),
        3 => (Some("Cascadia"), "Cascadia, Segoe UI Emoji"),
        5 => (Some("Excalifont"), "Excalifont, Xiaolai, Segoe UI Emoji"),
        6 => (Some("Nunito"), "Nunito, Segoe UI Emoji"),
        7 => (Some("Lilita One"), "Lilita One, Segoe UI Emoji"),
        8 => (Some("Comic Shanns"), "Comic Shanns, Segoe UI Emoji"),
        9 => (Some("Liberation Sans"), "Liberation Sans, Segoe UI Emoji"),
        _ => (None, "Segoe UI Emoji"),
    }
}

/// A number as the SVG gets it: two decimals at most
/// (`MAX_DECIMALS_FOR_SVG_EXPORT`).
fn n(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r == 0.0 || !r.is_finite() {
        "0".into()
    } else {
        format!("{r}")
    }
}

fn hex(c: Rgba) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c.r), b(c.g), b(c.b))
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Not allowed in XML at all.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

fn path_data(segs: &[Seg]) -> String {
    let mut d = String::new();
    for s in segs {
        match *s {
            Seg::Move(p) => write!(d, "M{} {} ", n(p[0]), n(p[1])),
            Seg::Line(p) => write!(d, "L{} {} ", n(p[0]), n(p[1])),
            Seg::Cubic(a, b, c) => write!(d, "C{} {}, {} {}, {} {} ", n(a[0]), n(a[1]), n(b[0]), n(b[1]), n(c[0]), n(c[1])),
            Seg::Close => write!(d, "Z "),
        }
        .unwrap();
    }
    d.trim_end().to_owned()
}

/// `isRTL`: any character of a right-to-left script.
fn is_rtl(s: &str) -> bool {
    s.chars().any(|c| matches!(c as u32, 0x0591..=0x07FF | 0xFB1D..=0xFDFD | 0xFE70..=0xFEFC))
}

/// A link Excalidraw would keep (`sanitizeUrl`): web and mail only.
fn safe_link(link: &str) -> Option<&str> {
    let l = link.trim();
    let lower = l.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")).then_some(l)
}

fn bound_text_id(e: &Element) -> Option<&str> {
    e.raw()
        .get("boundElements")?
        .as_array()?
        .iter()
        .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))?
        .get("id")?
        .as_str()
}

impl Export<'_> {
    /// The elements drawn, in order: as `renderSceneToSvg` walks them, each
    /// container's label straight after it.
    fn elements(&self) -> Vec<&Element> {
        let included: Vec<&Element> = self
            .scene
            .visible()
            .filter(|e| self.only.is_none_or(|ids| ids.contains(&e.id)))
            .filter(|e| !matches!(e.kind, Kind::Unknown))
            .collect();
        let by_id: HashMap<&str, &Element> = included.iter().map(|e| (e.id.as_str(), *e)).collect();
        let mut out = Vec::with_capacity(included.len());
        for e in &included {
            if e.text.as_ref().and_then(|t| t.container_id.as_deref()).is_some_and(|c| by_id.contains_key(c)) {
                continue;
            }
            out.push(*e);
            if let Some(t) = bound_text_id(e).and_then(|id| by_id.get(id)) {
                if t.text.as_ref().and_then(|t| t.container_id.as_deref()) == Some(e.id.as_str()) {
                    out.push(*t);
                }
            }
        }
        out
    }

    /// `getCanvasSize` over `getRootElements`: what is inside an exported
    /// frame is clipped to it, so the frame alone counts. A frame's name,
    /// drawn above it, counts too.
    fn bounds(&self, elements: &[&Element]) -> Option<[f64; 4]> {
        let frames: HashSet<&str> = elements.iter().filter(|e| matches!(e.kind, Kind::Frame | Kind::MagicFrame)).map(|e| e.id.as_str()).collect();
        let mut b: Option<[f64; 4]> = None;
        for e in elements {
            if e.frame_id.as_deref().is_some_and(|f| frames.contains(f)) {
                continue;
            }
            let mut eb = editor::geometry::bounds(e);
            if frames.contains(e.id.as_str()) && e.frame_name.as_deref().is_some_and(|s| !s.is_empty()) {
                eb[1] -= FRAME_NAME_SIZE * 1.25 + 3.0;
            }
            b = Some(b.map_or(eb, |b| editor::geometry::union(b, eb)));
        }
        b
    }

    /// The SVG, and its size in SVG pixels.
    fn write(&self, mode: Mode, embed: Option<&str>) -> Result<(String, f64, f64), Error> {
        let elements = self.elements();
        let b = self.bounds(&elements).ok_or(Error::Empty)?;
        let (width, height) = (b[2] - b[0] + PADDING * 2.0, b[3] - b[1] + PADDING * 2.0);
        let offset = [PADDING - b[0], PADDING - b[1]];

        let mut defs = String::new();
        let mut body = String::new();
        let mut used: BTreeMap<i64, BTreeSet<char>> = BTreeMap::new();
        let mut clips: HashMap<String, String> = HashMap::new();
        let mut next_id = 0usize;
        let mut fresh = |what: &str| {
            next_id += 1;
            format!("{what}-{next_id}")
        };

        if !self.transparent {
            write!(body, r#"<rect x="0" y="0" width="{}" height="{}" fill="{}"/>"#, n(width), n(height), escape(self.canvas)).unwrap();
        }
        for e in &elements {
            let p = render::prepare(e, self.scene, self.canvas);
            let mut node = String::new();
            let degrees = p.angle.to_degrees();
            let transform = format!(
                "translate({} {}) rotate({} {} {})",
                n(p.origin[0] + offset[0]),
                n(p.origin[1] + offset[1]),
                n(degrees),
                n(p.center[0]),
                n(p.center[1])
            );
            write!(node, r#"<g stroke-linecap="round" stroke-linejoin="round" transform="{transform}">"#).unwrap();
            for item in &p.items {
                self.item(&mut node, &mut defs, &mut fresh, item, &p, mode, &mut used);
            }
            node.push_str("</g>");

            // An arrow's label is cut out of it.
            if let Some([x0, y0, x1, y1]) = p.cut {
                let id = fresh("mask");
                let [bx0, by0, bx1, by1] = p.bounds;
                write!(
                    defs,
                    r##"<mask id="{id}" maskUnits="userSpaceOnUse" x="{}" y="{}" width="{}" height="{}"><rect x="{}" y="{}" width="{}" height="{}" fill="#fff"/><rect x="{}" y="{}" width="{}" height="{}" fill="#000"/></mask>"##,
                    n(bx0 + offset[0]),
                    n(by0 + offset[1]),
                    n(bx1 - bx0),
                    n(by1 - by0),
                    n(bx0 + offset[0]),
                    n(by0 + offset[1]),
                    n(bx1 - bx0),
                    n(by1 - by0),
                    n(x0 + offset[0]),
                    n(y0 + offset[1]),
                    n(x1 - x0),
                    n(y1 - y0)
                )
                .unwrap();
                node = format!(r#"<g mask="url(#{id})">{node}</g>"#);
            }
            // Inside a frame: clipped to it.
            if let Some([x0, y0, x1, y1]) = p.clip {
                let key = format!("{x0} {y0} {x1} {y1}");
                let id = match clips.get(&key) {
                    Some(id) => id.clone(),
                    None => {
                        let id = fresh("frame-clip");
                        write!(
                            defs,
                            r#"<clipPath id="{id}"><rect x="{}" y="{}" width="{}" height="{}" rx="{FRAME_RADIUS}" ry="{FRAME_RADIUS}"/></clipPath>"#,
                            n(x0 + offset[0]),
                            n(y0 + offset[1]),
                            n(x1 - x0),
                            n(y1 - y0)
                        )
                        .unwrap();
                        clips.insert(key, id.clone());
                        id
                    }
                };
                node = format!(r#"<g clip-path="url(#{id})">{node}</g>"#);
            }
            if let Some(link) = e.link.as_deref().and_then(safe_link) {
                node = format!(r#"<a href="{}">{node}</a>"#, escape(link));
            }
            body.push_str(&node);
        }

        let mut svg = format!(
            r#"<svg version="1.1" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}">"#,
            w = n(width),
            h = n(height)
        );
        if mode == Mode::File {
            svg.push_str("<!-- svg-source:excalidraw -->");
            svg.push_str("<metadata>");
            if let Some(payload) = embed {
                svg.push_str(&embed::svg_payload(payload));
            }
            svg.push_str("</metadata>");
        }
        svg.push_str("<defs>");
        svg.push_str(&defs);
        if mode == Mode::File {
            svg.push_str(r#"<style class="style-fonts">"#);
            for (family, chars) in &used {
                let (Some(name), Some(font)) = (excalidraw_family(*family as f64).0, (self.font)(*family as f64)) else {
                    continue;
                };
                let Ok(data) = fontmerge::subset(font.data, chars.iter().copied()) else {
                    continue;
                };
                write!(
                    svg,
                    "\n      @font-face {{ font-family: {name}; src: url(data:font/ttf;base64,{}); }}",
                    base64::engine::general_purpose::STANDARD.encode(data)
                )
                .unwrap();
            }
            svg.push_str("\n    </style>");
        }
        svg.push_str("</defs>");
        svg.push_str(&body);
        svg.push_str("</svg>");
        Ok((svg, width, height))
    }

    #[allow(clippy::too_many_arguments)]
    fn item(
        &self,
        out: &mut String,
        defs: &mut String,
        fresh: &mut impl FnMut(&str) -> String,
        item: &Item,
        p: &Prepared,
        mode: Mode,
        used: &mut BTreeMap<i64, BTreeSet<char>>,
    ) {
        let opacity = |c: Rgba| f64::from(c.a) * p.opacity;
        match item {
            Item::Stroke { path, color, width, dash } => {
                write!(out, r#"<path d="{}" stroke="{}" stroke-width="{}" fill="none""#, path_data(path), hex(*color), n(*width)).unwrap();
                if opacity(*color) < 1.0 {
                    write!(out, r#" stroke-opacity="{}""#, opacity(*color)).unwrap();
                }
                if let Some(dash) = dash.as_ref().filter(|d| !d.is_empty()) {
                    let list: Vec<String> = dash.iter().map(|d| n(*d)).collect();
                    write!(out, r#" stroke-dasharray="{}""#, list.join(" ")).unwrap();
                }
                out.push_str("/>");
            }
            Item::Fill { path, color, even_odd } => {
                write!(out, r#"<path d="{}" fill="{}" stroke="none""#, path_data(path), hex(*color)).unwrap();
                if opacity(*color) < 1.0 {
                    write!(out, r#" fill-opacity="{}""#, opacity(*color)).unwrap();
                }
                if *even_odd {
                    out.push_str(r#" fill-rule="evenodd""#);
                }
                out.push_str("/>");
            }
            Item::Text(t) => {
                let family = match mode {
                    Mode::File => excalidraw_family(t.font_family).1.to_owned(),
                    Mode::Raster => (self.font)(t.font_family).map_or("sans-serif".to_owned(), |f| f.family.to_owned()),
                };
                let whole: String = t.lines.join("\n");
                let rtl = is_rtl(&whole);
                let anchor = match t.align {
                    Align::Center => "middle",
                    Align::Right => "end",
                    Align::Left if rtl => "end",
                    Align::Left => "start",
                };
                used.entry(t.font_family as i64).or_default().extend(whole.chars().filter(|c| *c != '\n'));
                for (i, line) in t.lines.iter().enumerate() {
                    write!(
                        out,
                        r#"<text x="{}" y="{}" font-family="{}" font-size="{}px" fill="{}" text-anchor="{anchor}" style="white-space: pre;" xml:space="preserve" direction="{}" dominant-baseline="alphabetic""#,
                        n(t.x),
                        n(i as f64 * t.line_height_px + t.baseline),
                        escape(&family),
                        n(t.font_size),
                        hex(t.color),
                        if rtl { "rtl" } else { "ltr" }
                    )
                    .unwrap();
                    if opacity(t.color) < 1.0 {
                        write!(out, r#" fill-opacity="{}""#, opacity(t.color)).unwrap();
                    }
                    write!(out, ">{}</text>", escape(line)).unwrap();
                }
            }
            Item::Image { file_id, width, height, crop, corner_radius } => {
                // A missing file is left out, as Excalidraw leaves it out.
                let Some((_, url)) = file_id.as_deref().and_then(|id| self.scene.file(id)) else {
                    return;
                };
                if !url.starts_with("data:image/") {
                    return;
                }
                let (w, h) = (*width, *height);
                // Cropped: the whole image, shifted, shown through the box.
                let (dx, dy, uw, uh) = match crop {
                    Some([cx, cy, cw, ch, nw, nh]) if *cw > 0.0 && *ch > 0.0 => {
                        let (uw, uh) = (w / (cw / nw), h / (ch / nh));
                        (cx / (nw / uw), cy / (nh / uh), uw, uh)
                    }
                    _ => (0.0, 0.0, w, h),
                };
                let clip = fresh("image-clip");
                write!(
                    defs,
                    r#"<clipPath id="{clip}"><rect width="{}" height="{}" rx="{r}" ry="{r}"/></clipPath>"#,
                    n(w),
                    n(h),
                    r = n(*corner_radius)
                )
                .unwrap();
                let (cx, cy) = (w / 2.0, h / 2.0);
                write!(
                    out,
                    r#"<g transform="translate({} {}) scale({} {}) translate({} {})"><g clip-path="url(#{clip})"><image href="{}" x="{}" y="{}" width="{}" height="{}" preserveAspectRatio="none" opacity="{}"/></g></g>"#,
                    n(cx),
                    n(cy),
                    p.flip[0],
                    p.flip[1],
                    n(-cx),
                    n(-cy),
                    escape(url),
                    n(-dx),
                    n(-dy),
                    n(uw),
                    n(uh),
                    p.opacity
                )
                .unwrap();
            }
            Item::Frame { width, height, name, .. } => {
                write!(
                    out,
                    r##"<rect width="{}" height="{}" rx="{FRAME_RADIUS}" ry="{FRAME_RADIUS}" fill="none" stroke="#bbb" stroke-width="2"/>"##,
                    n(*width),
                    n(*height)
                )
                .unwrap();
                if let Some(name) = name.as_deref().filter(|s| !s.is_empty()) {
                    let family = match mode {
                        Mode::File => excalidraw_family(2.0).1.to_owned(),
                        Mode::Raster => (self.font)(2.0).map_or("sans-serif".to_owned(), |f| f.family.to_owned()),
                    };
                    write!(
                        out,
                        r##"<text x="0" y="-{}" font-family="{}" font-size="{FRAME_NAME_SIZE}px" fill="#999999" style="white-space: pre;" xml:space="preserve">{}</text>"##,
                        n(3.0 + FRAME_NAME_SIZE * 0.25),
                        escape(&family),
                        escape(name)
                    )
                    .unwrap();
                }
            }
        }
    }

    /// The SVG Excalidraw would write, the drawing inside it if `embed`
    /// (the `.excalidraw` JSON).
    pub fn svg(&self, embed: Option<&str>) -> Result<String, Error> {
        Ok(self.write(Mode::File, embed)?.0)
    }

    /// The image at `scale` pixels per drawing unit, premultiplied.
    fn pixmap(&self, scale: f64) -> Result<resvg::tiny_skia::Pixmap, Error> {
        self.pixmap_by(|_, _| scale)
    }

    /// The image at the scale `scale` picks for its size in drawing units.
    fn pixmap_by(&self, scale: impl Fn(f64, f64) -> f64) -> Result<resvg::tiny_skia::Pixmap, Error> {
        let (svg, w, h) = self.write(Mode::Raster, None)?;
        let scale = scale(w, h);
        // `getExportSize`: truncated.
        let (pw, ph) = ((w * scale).trunc().max(1.0), (h * scale).trunc().max(1.0));
        let too_large = Error::TooLarge { width: pw as u32, height: ph as u32 };
        if pw > f64::from(MAX_SIDE) || ph > f64::from(MAX_SIDE) || (pw * ph) as u64 > MAX_PIXELS {
            return Err(too_large);
        }
        let mut db = resvg::usvg::fontdb::Database::new();
        let mut loaded = HashSet::new();
        // Characters a text's own font lacks, which a browser (and the
        // canvas) would take from Excalifont, the one with CJK glyphs.
        let mut missing = BTreeSet::new();
        let spare = (self.font)(FALLBACK_FAMILY);
        let spare_face = spare.and_then(|f| ttf_parser::Face::parse(f.data, 0).ok());
        for e in self.elements() {
            let (family, text) = match (&e.text, e.kind) {
                (Some(t), _) => (t.font_family, t.text.as_str()),
                (None, Kind::Frame | Kind::MagicFrame) => (2.0, e.frame_name.as_deref().unwrap_or("")),
                _ => continue,
            };
            let Some(font) = (self.font)(family) else { continue };
            if loaded.insert(font.family) {
                db.load_font_source(resvg::usvg::fontdb::Source::Binary(Arc::new(font.data)));
            }
            if let (Ok(face), Some(spare)) = (ttf_parser::Face::parse(font.data, 0), &spare_face) {
                missing.extend(text.chars().filter(|c| !c.is_whitespace() && face.glyph_index(*c).is_none() && spare.glyph_index(*c).is_some()));
            }
        }
        // resvg's own fallback sets a whole line in the fallback font when
        // that font has every character of it, as Excalifont has the Latin
        // ones: given a font of only the missing characters, under a name of
        // its own, it falls back one character at a time instead.
        let mut fallback = None;
        if let (false, Some(spare)) = (missing.is_empty(), spare) {
            if let Ok(data) = fontmerge::subset_named(spare.data, missing.iter().copied(), Some("Excalidraw Fallback")) {
                fallback = db.load_font_source(resvg::usvg::fontdb::Source::Binary(Arc::new(data))).first().copied();
            }
        }
        let mut options = resvg::usvg::Options {
            fontdb: Arc::new(db),
            ..resvg::usvg::Options::default()
        };
        options.font_resolver.select_fallback = Box::new(move |_, used, _| fallback.filter(|id| !used.contains(id)));
        // Images come inline or not at all: an image inside a drawing must
        // not reach for a file on this machine.
        options.image_href_resolver.resolve_string = Box::new(|_, _| None);
        let tree = resvg::usvg::Tree::from_str(&svg, &options).map_err(|e| Error::Render(e.to_string()))?;
        let mut pixmap = resvg::tiny_skia::Pixmap::new(pw as u32, ph as u32).ok_or(too_large)?;
        resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale as f32, scale as f32), &mut pixmap.as_mut());
        Ok(pixmap)
    }

    /// A PNG at `scale`, the drawing inside it if `embed`.
    pub fn png(&self, scale: f64, embed: Option<&str>) -> Result<Vec<u8>, Error> {
        let png = self.pixmap(scale)?.encode_png().map_err(|e| Error::Render(e.to_string()))?;
        Ok(match embed {
            Some(payload) => embed::png_with_payload(&png, payload),
            None => png,
        })
    }

    /// The image as straight (not premultiplied) RGBA: for the clipboard,
    /// and for pictures of library items.
    pub fn rgba(&self, scale: f64) -> Result<(u32, u32, Vec<u8>), Error> {
        Ok(straight(self.pixmap(scale)?))
    }

    /// A picture at most `side` pixels wide and high, never enlarged past
    /// `max_scale`: a library item's.
    pub fn thumbnail(&self, side: f64, max_scale: f64) -> Result<(u32, u32, Vec<u8>), Error> {
        Ok(straight(self.pixmap_by(|w, h| (side / w).min(side / h).min(max_scale))?))
    }
}

/// Premultiplied pixels made straight RGBA.
fn straight(pixmap: resvg::tiny_skia::Pixmap) -> (u32, u32, Vec<u8>) {
    let (w, h) = (pixmap.width(), pixmap.height());
    let mut data = Vec::with_capacity(pixmap.data().len());
    for p in pixmap.pixels() {
        let c = p.demultiply();
        data.extend([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    (w, h, data)
}

const FRAME_RADIUS: f64 = 8.0;
/// Excalifont: where a character no other font has is looked for.
const FALLBACK_FAMILY: f64 = 5.0;
const FRAME_NAME_SIZE: f64 = 14.0;

/// The `.excalidraw` JSON of what `only` picks out of `scene` (all of it if
/// None), with the files those elements show: what Excalidraw stores inside
/// an exported image.
pub fn payload(scene: &Scene, only: Option<&HashSet<String>>) -> String {
    let mut part = scene.clone();
    if let Some(ids) = only {
        part.elements.retain(|e| ids.contains(&e.id));
    }
    let files: HashSet<String> = part
        .elements
        .iter()
        .filter(|e| !e.is_deleted)
        .filter_map(|e| e.image.as_ref()?.file_id.clone())
        .collect();
    part.retain_files(|id| files.contains(id));
    part.to_file_json()
}

#[cfg(test)]
mod tests;
