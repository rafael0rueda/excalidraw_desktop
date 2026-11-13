//! The icons of the tool bar and the properties panel, drawn as vector
//! paths in a 24 × 24 box in the theme's colours. Drawn with iced's canvas
//! rather than as SVG files: iced's own SVG support would bring a second,
//! older resvg into the build beside the exporter's.

use crate::Message;
use iced::widget::canvas::{self, Frame, LineCap, LineDash, LineJoin, Path, Stroke};
use iced::{mouse, Color, Element, Length, Point, Rectangle, Renderer, Size, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    // tools
    Hand,
    Select,
    Rectangle,
    Diamond,
    Ellipse,
    Arrow,
    Line,
    Draw,
    Text,
    Eraser,
    Image,
    Frame,
    Lock(bool),
    Undo,
    Redo,
    // properties
    StrokeWidth(u8),
    StrokeSolid,
    StrokeDashed,
    StrokeDotted,
    Sloppiness(u8),
    EdgeSharp,
    EdgeRound,
    FillHachure,
    FillCrossHatch,
    FillSolid,
    ArrowSharp,
    ArrowRound,
    /// An arrowhead, at the right end (`start` flips it to the left).
    Head(Head, bool),
    AlignLeft,
    AlignCenter,
    AlignRight,
    AlignTop,
    AlignMiddle,
    AlignBottom,
    SendToBack,
    SendBackward,
    BringForward,
    BringToFront,
    Duplicate,
    Delete,
    Group,
    Ungroup,
    Link,
    /// Aligning elements to the selection's edge or middle.
    Align(editor::Align),
    Distribute(editor::Axis),
}

/// Excalidraw's arrowheads, `None` for no head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Head {
    None,
    Arrow,
    Triangle,
    TriangleOutline,
    Circle,
    CircleOutline,
    Diamond,
    DiamondOutline,
    Bar,
    CrowfootOne,
    CrowfootMany,
    CrowfootOneOrMany,
}

impl Head {
    pub const ALL: [Head; 12] = [
        Head::None,
        Head::Arrow,
        Head::Triangle,
        Head::TriangleOutline,
        Head::Circle,
        Head::CircleOutline,
        Head::Diamond,
        Head::DiamondOutline,
        Head::Bar,
        Head::CrowfootOne,
        Head::CrowfootMany,
        Head::CrowfootOneOrMany,
    ];

    /// The name in the file. `dot` is the old name of `circle`.
    pub fn name(self) -> Option<&'static str> {
        Some(match self {
            Head::None => return None,
            Head::Arrow => "arrow",
            Head::Triangle => "triangle",
            Head::TriangleOutline => "triangle_outline",
            Head::Circle => "circle",
            Head::CircleOutline => "circle_outline",
            Head::Diamond => "diamond",
            Head::DiamondOutline => "diamond_outline",
            Head::Bar => "bar",
            Head::CrowfootOne => "crowfoot_one",
            Head::CrowfootMany => "crowfoot_many",
            Head::CrowfootOneOrMany => "crowfoot_one_or_many",
        })
    }

    pub fn of(name: Option<&str>) -> Head {
        match name {
            None => Head::None,
            Some("dot") => Head::Circle,
            Some(n) => Head::ALL.into_iter().find(|h| h.name() == Some(n)).unwrap_or(Head::Arrow),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Head::None => "None",
            Head::Arrow => "Arrow",
            Head::Triangle => "Triangle",
            Head::TriangleOutline => "Triangle (outline)",
            Head::Circle => "Circle",
            Head::CircleOutline => "Circle (outline)",
            Head::Diamond => "Diamond",
            Head::DiamondOutline => "Diamond (outline)",
            Head::Bar => "Bar",
            Head::CrowfootOne => "Crow's foot: one",
            Head::CrowfootMany => "Crow's foot: many",
            Head::CrowfootOneOrMany => "Crow's foot: one or many",
        }
    }
}

/// `icon` at `size` pixels, in `color`.
pub fn icon<'a>(icon: Icon, color: Color, size: f32) -> Element<'a, Message> {
    canvas::Canvas::new(Draw { icon, color }).width(Length::Fixed(size)).height(Length::Fixed(size)).into()
}

struct Draw {
    icon: Icon,
    color: Color,
}

impl canvas::Program<Message> for Draw {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let mut pen = Pen { frame: &mut frame, k: bounds.width / 24.0, color: self.color, width: 1.75, dash: false };
        paint(self.icon, &mut pen);
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(&self, _: &(), _: Rectangle, _: mouse::Cursor) -> mouse::Interaction {
        mouse::Interaction::None
    }
}

/// Draws in the 24 × 24 box.
struct Pen<'f> {
    frame: &'f mut Frame,
    k: f32,
    color: Color,
    width: f32,
    dash: bool,
}

type P = (f32, f32);

impl Pen<'_> {
    fn p(&self, (x, y): P) -> Point {
        Point::new(x * self.k, y * self.k)
    }

    fn stroke(&mut self, path: &Path) {
        let segments = [2.0 * self.k, 2.5 * self.k];
        let mut stroke = Stroke::default().with_color(self.color).with_width(self.width * self.k).with_line_cap(LineCap::Round).with_line_join(LineJoin::Round);
        if self.dash {
            stroke.line_dash = LineDash { segments: &segments, offset: 0 };
        }
        self.frame.stroke(path, stroke);
    }

    fn line(&mut self, pts: &[P]) {
        let pts: Vec<Point> = pts.iter().map(|p| self.p(*p)).collect();
        let path = Path::new(|b| {
            b.move_to(pts[0]);
            for p in &pts[1..] {
                b.line_to(*p);
            }
        });
        self.stroke(&path);
    }

    fn shape(&mut self, pts: &[P], fill: bool) {
        let pts: Vec<Point> = pts.iter().map(|p| self.p(*p)).collect();
        let path = Path::new(|b| {
            b.move_to(pts[0]);
            for p in &pts[1..] {
                b.line_to(*p);
            }
            b.close();
        });
        if fill {
            self.frame.fill(&path, self.color);
        }
        self.stroke(&path);
    }

    fn circle(&mut self, c: P, r: f32, fill: bool) {
        let path = Path::circle(self.p(c), r * self.k);
        if fill {
            self.frame.fill(&path, self.color);
        }
        self.stroke(&path);
    }

    fn rect(&mut self, (x, y): P, (w, h): P, r: f32, fill: Option<Color>) {
        let path = Path::rounded_rectangle(self.p((x, y)), Size::new(w * self.k, h * self.k), (r * self.k).into());
        if let Some(c) = fill {
            self.frame.fill(&path, c);
        }
        self.stroke(&path);
    }

    fn curve(&mut self, from: P, c1: P, c2: P, to: P) {
        let (a, b, c, d) = (self.p(from), self.p(c1), self.p(c2), self.p(to));
        let path = Path::new(|p| {
            p.move_to(a);
            p.bezier_curve_to(b, c, d);
        });
        self.stroke(&path);
    }

    fn faint(&self) -> Color {
        Color { a: self.color.a * 0.3, ..self.color }
    }
}

fn paint(icon: Icon, p: &mut Pen) {
    match icon {
        Icon::Hand => {
            p.line(&[(8.0, 13.0), (8.0, 5.5), (10.5, 5.5), (10.5, 11.5)]);
            p.line(&[(10.5, 11.0), (10.5, 4.0), (13.0, 4.0), (13.0, 11.0)]);
            p.line(&[(13.0, 11.0), (13.0, 5.0), (15.5, 5.0), (15.5, 11.5)]);
            p.line(&[(15.5, 11.5), (15.5, 7.5), (18.0, 7.5), (18.0, 15.0)]);
            p.curve((18.0, 15.0), (18.0, 19.0), (15.5, 21.0), (12.5, 21.0));
            p.curve((12.5, 21.0), (9.5, 21.0), (7.5, 19.5), (5.0, 15.5));
            p.line(&[(5.0, 15.5), (4.5, 13.5), (6.5, 12.5), (8.0, 14.5)]);
        }
        Icon::Select => p.shape(&[(6.0, 4.0), (6.0, 19.0), (10.0, 15.0), (13.0, 21.0), (15.5, 20.0), (12.5, 14.0), (18.0, 14.0)], false),
        Icon::Rectangle => p.rect((4.0, 6.0), (16.0, 12.0), 2.5, None),
        Icon::Diamond => p.shape(&[(12.0, 3.5), (20.5, 12.0), (12.0, 20.5), (3.5, 12.0)], false),
        Icon::Ellipse => p.circle((12.0, 12.0), 8.0, false),
        Icon::Arrow => {
            p.line(&[(5.0, 19.0), (19.0, 5.0)]);
            p.line(&[(11.0, 5.0), (19.0, 5.0), (19.0, 13.0)]);
        }
        Icon::Line => p.line(&[(5.0, 12.0), (19.0, 12.0)]),
        Icon::Draw => {
            p.shape(&[(15.0, 4.5), (19.5, 9.0), (9.0, 19.5), (4.5, 19.5), (4.5, 15.0)], false);
            p.line(&[(13.0, 6.5), (17.5, 11.0)]);
        }
        Icon::Text => {
            p.line(&[(6.0, 7.0), (6.0, 5.0), (18.0, 5.0), (18.0, 7.0)]);
            p.line(&[(12.0, 5.0), (12.0, 19.0)]);
            p.line(&[(9.5, 19.0), (14.5, 19.0)]);
        }
        Icon::Eraser => {
            p.shape(&[(14.0, 4.0), (20.5, 10.5), (12.0, 19.0), (8.5, 19.0), (3.5, 14.0)], false);
            p.line(&[(9.0, 9.0), (15.5, 15.5)]);
            p.line(&[(12.0, 19.0), (20.0, 19.0)]);
        }
        Icon::Image => {
            p.rect((4.0, 5.0), (16.0, 14.0), 2.0, None);
            p.circle((9.0, 10.0), 1.5, false);
            p.line(&[(4.5, 17.0), (10.0, 12.5), (14.0, 16.0), (16.5, 14.0), (19.5, 16.5)]);
        }
        Icon::Frame => {
            p.line(&[(7.0, 4.0), (7.0, 20.0)]);
            p.line(&[(17.0, 4.0), (17.0, 20.0)]);
            p.line(&[(4.0, 7.0), (20.0, 7.0)]);
            p.line(&[(4.0, 17.0), (20.0, 17.0)]);
        }
        Icon::Lock(locked) => {
            p.rect((5.5, 11.0), (13.0, 9.0), 2.0, None);
            if locked {
                p.line(&[(8.5, 11.0), (8.5, 7.5)]);
                p.curve((8.5, 7.5), (8.5, 3.0), (15.5, 3.0), (15.5, 7.5));
                p.line(&[(15.5, 7.5), (15.5, 11.0)]);
            } else {
                p.line(&[(8.5, 11.0), (8.5, 7.5)]);
                p.curve((8.5, 7.5), (8.5, 3.0), (15.5, 3.0), (15.5, 7.0));
            }
        }
        Icon::Undo => {
            p.line(&[(9.0, 5.0), (5.0, 9.0), (9.0, 13.0)]);
            p.line(&[(5.0, 9.0), (14.0, 9.0)]);
            p.curve((14.0, 9.0), (21.0, 9.0), (21.0, 19.0), (14.0, 19.0));
            p.line(&[(14.0, 19.0), (9.0, 19.0)]);
        }
        Icon::Redo => {
            p.line(&[(15.0, 5.0), (19.0, 9.0), (15.0, 13.0)]);
            p.line(&[(19.0, 9.0), (10.0, 9.0)]);
            p.curve((10.0, 9.0), (3.0, 9.0), (3.0, 19.0), (10.0, 19.0));
            p.line(&[(10.0, 19.0), (15.0, 19.0)]);
        }
        Icon::StrokeWidth(n) => {
            p.width = match n {
                1 => 1.0,
                2 => 2.25,
                _ => 4.0,
            };
            p.line(&[(4.0, 12.0), (20.0, 12.0)]);
        }
        Icon::StrokeSolid => p.line(&[(4.0, 12.0), (20.0, 12.0)]),
        Icon::StrokeDashed => {
            for x in [4.0, 10.0, 16.0] {
                p.line(&[(x, 12.0), (x + 3.5, 12.0)]);
            }
        }
        Icon::StrokeDotted => {
            for x in [4.5, 9.0, 13.5, 18.0] {
                p.circle((x + 0.5, 12.0), 0.4, true);
            }
        }
        Icon::Sloppiness(n) => match n {
            0 => p.curve((4.0, 17.0), (9.0, 6.0), (14.0, 6.0), (20.0, 9.0)),
            1 => {
                p.curve((4.0, 17.0), (8.0, 8.0), (12.0, 13.0), (20.0, 7.0));
                p.curve((5.0, 18.0), (9.0, 10.0), (13.0, 14.0), (19.5, 9.0));
            }
            _ => {
                p.curve((4.0, 17.0), (6.0, 6.0), (11.0, 16.0), (13.0, 9.0));
                p.curve((13.0, 9.0), (15.0, 3.0), (18.0, 12.0), (20.0, 7.0));
                p.curve((5.0, 19.0), (8.0, 9.0), (12.0, 18.0), (20.0, 9.0));
            }
        },
        Icon::EdgeSharp => p.line(&[(5.0, 19.0), (5.0, 5.0), (19.0, 5.0)]),
        Icon::EdgeRound => {
            p.line(&[(5.0, 19.0), (5.0, 11.0)]);
            p.curve((5.0, 11.0), (5.0, 7.0), (7.0, 5.0), (11.0, 5.0));
            p.line(&[(11.0, 5.0), (19.0, 5.0)]);
        }
        Icon::FillHachure | Icon::FillCrossHatch | Icon::FillSolid => {
            let faint = p.faint();
            match icon {
                Icon::FillSolid => p.rect((4.0, 4.0), (16.0, 16.0), 3.0, Some(faint)),
                _ => {
                    p.rect((4.0, 4.0), (16.0, 16.0), 3.0, None);
                    p.width = 1.1;
                    // Diagonals across the box, both ways for cross-hatch.
                    for s in [12.0f32, 17.0, 22.0, 27.0, 32.0] {
                        let (x0, x1) = ((s - 20.0).max(4.0), (s - 4.0).min(20.0));
                        p.line(&[(x0, s - x0), (x1, s - x1)]);
                    }
                    if icon == Icon::FillCrossHatch {
                        for d in [-10.0f32, -5.0, 0.0, 5.0, 10.0] {
                            let (x0, x1) = ((4.0 + d).max(4.0), (20.0 + d).min(20.0));
                            p.line(&[(x0, x0 - d), (x1, x1 - d)]);
                        }
                    }
                }
            }
        }
        Icon::ArrowSharp => {
            p.line(&[(4.0, 18.0), (12.0, 8.0), (20.0, 8.0)]);
            p.line(&[(16.5, 5.0), (20.0, 8.0), (16.5, 11.0)]);
        }
        Icon::ArrowRound => {
            p.curve((4.0, 18.0), (6.0, 10.0), (12.0, 8.0), (20.0, 8.0));
            p.line(&[(16.5, 5.0), (20.0, 8.0), (16.5, 11.0)]);
        }
        Icon::Head(head, start) => {
            // Drawn pointing right, then mirrored for the start.
            let m = |x: f32| if start { 24.0 - x } else { x };
            let pts = |p: &[P]| -> Vec<P> { p.iter().map(|(x, y)| (m(*x), *y)).collect() };
            let tip = 20.0;
            let shaft_end = match head {
                Head::None | Head::Arrow | Head::Bar | Head::CrowfootOne => tip,
                Head::Circle | Head::CircleOutline => 14.0,
                Head::Diamond | Head::DiamondOutline => 10.0,
                Head::CrowfootMany | Head::CrowfootOneOrMany => tip,
                Head::Triangle | Head::TriangleOutline => 13.0,
            };
            p.line(&pts(&[(3.0, 12.0), (shaft_end, 12.0)]));
            match head {
                Head::None => {}
                Head::Arrow => p.line(&pts(&[(14.0, 7.0), (tip, 12.0), (14.0, 17.0)])),
                Head::Triangle | Head::TriangleOutline => p.shape(&pts(&[(13.0, 7.0), (tip, 12.0), (13.0, 17.0)]), head == Head::Triangle),
                Head::Circle | Head::CircleOutline => p.circle((m(17.0), 12.0), 3.0, head == Head::Circle),
                Head::Diamond | Head::DiamondOutline => p.shape(&pts(&[(10.0, 12.0), (15.0, 8.0), (tip, 12.0), (15.0, 16.0)]), head == Head::Diamond),
                Head::Bar => p.line(&pts(&[(tip, 6.5), (tip, 17.5)])),
                Head::CrowfootOne => p.line(&pts(&[(15.0, 6.5), (15.0, 17.5)])),
                Head::CrowfootMany | Head::CrowfootOneOrMany => {
                    p.line(&pts(&[(14.0, 12.0), (tip, 6.5)]));
                    p.line(&pts(&[(14.0, 12.0), (tip, 17.5)]));
                    if head == Head::CrowfootOneOrMany {
                        p.line(&pts(&[(11.0, 6.5), (11.0, 17.5)]));
                    }
                }
            }
        }
        Icon::AlignLeft => {
            p.line(&[(4.0, 6.0), (20.0, 6.0)]);
            p.line(&[(4.0, 12.0), (14.0, 12.0)]);
            p.line(&[(4.0, 18.0), (17.0, 18.0)]);
        }
        Icon::AlignCenter => {
            p.line(&[(4.0, 6.0), (20.0, 6.0)]);
            p.line(&[(7.0, 12.0), (17.0, 12.0)]);
            p.line(&[(5.5, 18.0), (18.5, 18.0)]);
        }
        Icon::AlignRight => {
            p.line(&[(4.0, 6.0), (20.0, 6.0)]);
            p.line(&[(10.0, 12.0), (20.0, 12.0)]);
            p.line(&[(7.0, 18.0), (20.0, 18.0)]);
        }
        Icon::AlignTop | Icon::AlignMiddle | Icon::AlignBottom => {
            let y = match icon {
                Icon::AlignTop => 5.0,
                Icon::AlignMiddle => 10.0,
                _ => 15.0,
            };
            p.line(&[(4.0, if icon == Icon::AlignBottom { 20.0 } else { 4.0 }), (20.0, if icon == Icon::AlignBottom { 20.0 } else { 4.0 })]);
            let faint = p.faint();
            p.rect((7.0, y), (10.0, 4.0), 1.0, Some(faint));
            if icon == Icon::AlignMiddle {
                p.line(&[(4.0, 20.0), (20.0, 20.0)]);
            }
        }
        Icon::SendToBack | Icon::SendBackward | Icon::BringForward | Icon::BringToFront => {
            let faint = p.faint();
            let up = matches!(icon, Icon::BringForward | Icon::BringToFront);
            p.rect((4.0, 9.0), (10.0, 10.0), 2.0, if up { None } else { Some(faint) });
            p.rect((10.0, 5.0), (10.0, 10.0), 2.0, if up { Some(faint) } else { None });
            let all = matches!(icon, Icon::SendToBack | Icon::BringToFront);
            if all {
                p.line(&[(3.0, 22.0), (21.0, 22.0)].map(|(x, y)| (x, if up { 2.0 } else { y })));
            }
        }
        Icon::Duplicate => {
            p.rect((8.0, 8.0), (12.0, 12.0), 2.0, None);
            p.line(&[(16.0, 8.0), (16.0, 6.0)]);
            p.curve((16.0, 6.0), (16.0, 4.5), (15.5, 4.0), (14.0, 4.0));
            p.line(&[(14.0, 4.0), (6.0, 4.0)]);
            p.curve((6.0, 4.0), (4.5, 4.0), (4.0, 4.5), (4.0, 6.0));
            p.line(&[(4.0, 6.0), (4.0, 14.0)]);
            p.curve((4.0, 14.0), (4.0, 15.5), (4.5, 16.0), (6.0, 16.0));
            p.line(&[(6.0, 16.0), (8.0, 16.0)]);
        }
        Icon::Delete => {
            p.line(&[(4.0, 7.0), (20.0, 7.0)]);
            p.line(&[(9.0, 7.0), (9.0, 4.5), (15.0, 4.5), (15.0, 7.0)]);
            p.line(&[(6.0, 7.0), (7.0, 20.0), (17.0, 20.0), (18.0, 7.0)]);
            p.line(&[(10.0, 11.0), (10.0, 16.0)]);
            p.line(&[(14.0, 11.0), (14.0, 16.0)]);
        }
        Icon::Group | Icon::Ungroup => {
            if icon == Icon::Group {
                p.dash = true;
                p.rect((3.0, 3.0), (18.0, 18.0), 2.0, None);
                p.dash = false;
            }
            p.rect((6.0, 6.0), (7.0, 7.0), 1.0, None);
            p.rect((11.0, 11.0), (7.0, 7.0), 1.0, None);
        }
        Icon::Align(a) => {
            use editor::Align;
            let faint = p.faint();
            // The line they align to, and two boxes against it.
            let vertical = matches!(a, Align::Left | Align::CenterX | Align::Right);
            let line = match a {
                Align::Left | Align::Top => 4.0,
                Align::CenterX | Align::CenterY => 12.0,
                _ => 20.0,
            };
            let boxes: [(f32, f32); 2] = [(10.0, 6.0), (14.0, 6.0)];
            for (i, (long, short)) in boxes.iter().enumerate() {
                let across = if i == 0 { 5.5 } else { 13.0 };
                let start = match a {
                    Align::Left | Align::Top => 6.0,
                    Align::CenterX | Align::CenterY => 12.0 - long / 2.0,
                    _ => 18.0 - long,
                };
                if vertical {
                    p.rect((start, across), (*long, *short - 0.5), 1.0, Some(faint));
                } else {
                    p.rect((across, start), (*short - 0.5, *long), 1.0, Some(faint));
                }
            }
            if vertical {
                p.line(&[(line, 3.0), (line, 21.0)]);
            } else {
                p.line(&[(3.0, line), (21.0, line)]);
            }
        }
        Icon::Distribute(axis) => {
            let faint = p.faint();
            if axis == editor::Axis::X {
                p.line(&[(3.0, 4.0), (3.0, 20.0)]);
                p.line(&[(21.0, 4.0), (21.0, 20.0)]);
                p.rect((8.5, 7.0), (7.0, 10.0), 1.0, Some(faint));
            } else {
                p.line(&[(4.0, 3.0), (20.0, 3.0)]);
                p.line(&[(4.0, 21.0), (20.0, 21.0)]);
                p.rect((7.0, 8.5), (10.0, 7.0), 1.0, Some(faint));
            }
        }
        Icon::Link => {
            p.curve((10.5, 13.5), (12.0, 15.0), (14.0, 15.0), (15.5, 13.5));
            p.line(&[(15.5, 13.5), (19.0, 10.0)]);
            p.curve((19.0, 10.0), (21.0, 8.0), (16.0, 3.0), (14.0, 5.0));
            p.line(&[(14.0, 5.0), (12.5, 6.5)]);
            p.curve((13.5, 10.5), (12.0, 9.0), (10.0, 9.0), (8.5, 10.5));
            p.line(&[(8.5, 10.5), (5.0, 14.0)]);
            p.curve((5.0, 14.0), (3.0, 16.0), (8.0, 21.0), (10.0, 19.0));
            p.line(&[(10.0, 19.0), (11.5, 17.5)]);
        }
    }
}

/// A colour swatch, `size` pixels square; `None` (transparent) shows a
/// checkerboard.
pub fn swatch<'a>(color: Option<Color>, border: Color, size: f32) -> Element<'a, Message> {
    canvas::Canvas::new(Swatch { color, border }).width(Length::Fixed(size)).height(Length::Fixed(size)).into()
}

struct Swatch {
    color: Option<Color>,
    border: Color,
}

impl canvas::Program<Message> for Swatch {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let s = bounds.width;
        let r = (s * 0.18).min(4.0);
        let outline = Path::rounded_rectangle(Point::new(0.5, 0.5), Size::new(s - 1.0, s - 1.0), r.into());
        match self.color {
            Some(c) => frame.fill(&outline, c),
            None => {
                frame.fill(&outline, Color::WHITE);
                let n = 4;
                let cell = s / n as f32;
                for i in 0..n {
                    for j in 0..n {
                        if (i + j) % 2 == 0 {
                            frame.fill_rectangle(Point::new(i as f32 * cell, j as f32 * cell), Size::new(cell, cell), Color::from_rgb8(0xdd, 0xdd, 0xdd));
                        }
                    }
                }
            }
        }
        frame.stroke(&outline, Stroke::default().with_color(self.border).with_width(1.0));
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(&self, _: &(), _: Rectangle, _: mouse::Cursor) -> mouse::Interaction {
        mouse::Interaction::None
    }
}
