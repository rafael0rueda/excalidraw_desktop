//! `render::Prepared` onto an iced canvas frame.

use iced::widget::canvas::{
    self, fill, path, Fill, Frame, LineCap, LineDash, LineJoin, Path, Stroke, Text,
};
use iced::{alignment, Color, Font, Pixels, Point, Radians, Rectangle, Size, Vector};
use render::{Align, Item, Prepared, Rgba, Seg};

pub type Renderer = iced::Renderer;

fn color(c: Rgba, opacity: f64) -> Color {
    Color::from_rgba(c.r, c.g, c.b, c.a * opacity as f32)
}

/// A path with every point already in the cache's space (`map`): iced's
/// tessellator moves a path's points by the frame transform but leaves the
/// stroke width and dash lengths as given, so strokes are mapped here and
/// sized explicitly instead.
fn path_of(segs: &[Seg], map: &impl Fn([f64; 2]) -> Point) -> Path {
    Path::new(|b: &mut path::Builder| {
        for seg in segs {
            match *seg {
                Seg::Move(a) => b.move_to(map(a)),
                Seg::Line(a) => b.line_to(map(a)),
                Seg::Cubic(c1, c2, to) => b.bezier_curve_to(map(c1), map(c2), map(to)),
                Seg::Close => b.close(),
            }
        }
    })
}

/// Excalidraw's font families, by the id a text element stores, named as
/// the bundled fonts name themselves (`build.rs`).
pub fn font(family: f64) -> Font {
    let dir = font_dir(family);
    match crate::FAMILY_NAMES.iter().find(|(d, _, _)| *d == dir) {
        // The file's own weight too: Nunito's Latin file is the Medium
        // instance of a family named "Nunito ExtraLight", and asking for
        // Normal fell through to a system font.
        Some((_, name, weight)) => Font {
            weight: font_weight(*weight),
            ..Font::with_name(name)
        },
        None => Font::DEFAULT,
    }
}

/// The bundled font (`build.rs`'s directory name) a family id is drawn in.
fn font_dir(family: f64) -> &'static str {
    match family as i64 {
        1 => "Virgil",
        // Helvetica is a local font in Excalidraw; on Linux the browser
        // resolves it to Liberation Sans or a lookalike.
        2 | 9 => "Liberation",
        3 => "Cascadia",
        6 => "Nunito",
        7 => "Lilita",
        8 => "ComicShanns",
        _ => "Excalifont",
    }
}

/// The same font for the exporter: its name and its bytes.
pub fn export_font(family: f64) -> Option<export::Font> {
    let dir = font_dir(family);
    let (_, name, _) = crate::FAMILY_NAMES.iter().find(|(d, _, _)| *d == dir)?;
    let (_, data) = crate::FONTS.iter().find(|(n, _)| n == name)?;
    Some(export::Font { family: name, data })
}

fn font_weight(w: u16) -> iced::font::Weight {
    use iced::font::Weight;
    match w {
        0..=149 => Weight::Thin,
        150..=249 => Weight::ExtraLight,
        250..=349 => Weight::Light,
        350..=449 => Weight::Normal,
        450..=549 => Weight::Medium,
        550..=649 => Weight::Semibold,
        650..=749 => Weight::Bold,
        750..=849 => Weight::ExtraBold,
        _ => Weight::Black,
    }
}

/// The images one element shows, by element id.
pub type Images = std::collections::HashMap<String, iced::widget::image::Handle>;

/// Draws one element in scene coordinates scaled by `zoom`. `zoom` is the
/// zoom the geometry is built at, which sizes what Excalidraw keeps at a
/// fixed size on screen (frame outlines and names).
pub fn element(frame: &mut Frame<Renderer>, id: &str, p: &Prepared, images: &Images, zoom: f32) {
    let draw = |f: &mut Frame<Renderer>| {
        for item in &p.items {
            item_into(f, id, item, p, images, zoom);
        }
    };
    // Where it may draw, in scene units: its frame, or everywhere it reaches.
    let area = p.clip.unwrap_or(p.bounds);
    let regions: Vec<[f64; 4]> = match p.cut {
        // iced has no inverse clip: the four bands around the label's box
        // together are everything but it.
        Some([cx0, cy0, cx1, cy1]) => {
            let [x0, y0, x1, y1] = area;
            vec![
                [x0, y0, x1, cy0.min(y1)],
                [x0, cy1.max(y0), x1, y1],
                [x0, cy0.max(y0), cx0.min(x1), cy1.min(y1)],
                [cx1.max(x0), cy0.max(y0), x1, cy1.min(y1)],
            ]
        }
        None if p.clip.is_some() => vec![area],
        None => {
            draw(frame);
            return;
        }
    };
    for [x0, y0, x1, y1] in regions {
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        let region = Rectangle::new(
            Point::new(x0 as f32 * zoom, y0 as f32 * zoom),
            Size::new((x1 - x0) as f32 * zoom, (y1 - y0) as f32 * zoom),
        );
        frame.with_clip(region, draw);
    }
}

/// Runs `f` with the frame transformed into the element's own coordinates.
/// For text and images, which iced does transform as a whole.
fn in_element<R>(
    f: &mut Frame<Renderer>,
    p: &Prepared,
    zoom: f32,
    draw: impl FnOnce(&mut Frame<Renderer>) -> R,
) -> R {
    f.with_save(|f| {
        f.scale(zoom);
        f.translate(Vector::new(
            (p.origin[0] + p.center[0]) as f32,
            (p.origin[1] + p.center[1]) as f32,
        ));
        f.rotate(Radians(p.angle as f32));
        f.translate(Vector::new(-p.center[0] as f32, -p.center[1] as f32));
        draw(f)
    })
}

fn item_into(
    f: &mut Frame<Renderer>,
    id: &str,
    item: &Item,
    p: &Prepared,
    images: &Images,
    zoom: f32,
) {
    let opacity = p.opacity;
    let map = |q: [f64; 2]| {
        let [x, y] = p.to_scene(q);
        Point::new(x as f32 * zoom, y as f32 * zoom)
    };
    match item {
        Item::Stroke {
            path,
            color: c,
            width,
            dash,
        } => {
            let segments: Vec<f32> = dash.iter().flatten().map(|d| *d as f32 * zoom).collect();
            let stroke = Stroke {
                width: *width as f32 * zoom,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                line_dash: LineDash {
                    segments: &segments,
                    offset: 0,
                },
                ..Stroke::default()
            }
            .with_color(color(*c, opacity));
            f.stroke(&path_of(path, &map), stroke);
        }
        Item::Fill {
            path,
            color: c,
            even_odd,
        } => {
            let rule = if *even_odd {
                fill::Rule::EvenOdd
            } else {
                fill::Rule::NonZero
            };
            f.fill(
                &path_of(path, &map),
                Fill {
                    style: canvas::Style::Solid(color(*c, opacity)),
                    rule,
                },
            );
        }
        Item::Text(t) => in_element(f, p, zoom, |f| {
            let align_x = match t.align {
                Align::Left => iced::widget::text::Alignment::Left,
                Align::Center => iced::widget::text::Alignment::Center,
                Align::Right => iced::widget::text::Alignment::Right,
            };
            for (i, line) in t.lines.iter().enumerate() {
                // The line box's top: cosmic-text centres the font's ascent and
                // descent in the line height, which puts the baseline exactly
                // where `getVerticalOffset` does.
                f.fill_text(Text {
                    content: line.clone(),
                    position: Point::new(t.x as f32, (i as f64 * t.line_height_px) as f32),
                    max_width: f32::INFINITY,
                    color: color(t.color, opacity),
                    size: Pixels(t.font_size as f32),
                    line_height: iced::widget::text::LineHeight::Absolute(Pixels(
                        t.line_height_px as f32,
                    )),
                    font: font(t.font_family),
                    align_x,
                    align_y: alignment::Vertical::Top,
                    shaping: iced::widget::text::Shaping::Advanced,
                });
            }
        }),
        Item::Image { width, height, .. } => in_element(f, p, zoom, |f| {
            let bounds = Rectangle::new(Point::ORIGIN, Size::new(*width as f32, *height as f32));
            match images.get(id) {
                Some(handle) => f.draw_image(
                    bounds,
                    canvas::Image::new(handle.clone()).opacity(opacity as f32),
                ),
                // `drawImagePlaceholder`'s grey.
                None => f.fill_rectangle(
                    bounds.position(),
                    bounds.size(),
                    color(render::color::parse("#E7E7E7"), opacity),
                ),
            }
        }),
        Item::Frame {
            width,
            height,
            name,
            magic,
        } => in_element(f, p, zoom, |f| {
            // On screen, 2 px and a 14 px name whatever the zoom. Widths are
            // not scaled by the frame transform, so this one is in cache pixels.
            let w = 2.0;
            let outline = Path::rounded_rectangle(
                Point::ORIGIN,
                Size::new(*width as f32, *height as f32),
                (8.0 / zoom).into(),
            );
            let c = if *magic { "#7affd7" } else { "#bbb" };
            f.stroke(
                &outline,
                Stroke::default()
                    .with_width(w)
                    .with_color(color(render::color::parse(c), opacity)),
            );
            if let Some(name) = name {
                let size = 14.0 / zoom;
                f.fill_text(Text {
                    content: name.clone(),
                    position: Point::new(0.0, -3.0 / zoom),
                    size: Pixels(size),
                    color: color(render::color::parse("#999999"), opacity),
                    align_y: alignment::Vertical::Bottom,
                    ..Text::default()
                });
            }
        }),
    }
}
