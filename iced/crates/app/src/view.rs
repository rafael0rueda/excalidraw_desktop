//! The scene widget: the drawing's chunks, tessellated once and drawn under
//! the view transform, with the editor's overlay on top.
//!
//! iced's own `Canvas` cannot do this: its cached geometry is drawn where it
//! was built, so every pan would re-tessellate every element. This widget
//! draws the caches under `renderer.with_transformation`, and they are only
//! rebuilt when the zoom leaves the power-of-two bucket they were built at
//! (so curves stay smooth) or when an edit touches them (plan, W1).

use crate::layers::Layers;
use crate::paint::Renderer;
use editor::{Cursor, Editor, Handle};
use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{tree, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::mouse::{self, ScrollDelta};
use iced::widget::canvas::{Frame, Path, Stroke};
use iced::{
    keyboard, Color, Element, Event, Length, Point, Rectangle, Size, Transformation, Vector,
};
use std::cell::RefCell;
use std::time::{Duration, Instant};

/// Where the drawing is on screen: scene point `p` lands at `p * zoom + pan`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub pan: Vector,
    pub zoom: f32,
}

impl View {
    pub const MIN_ZOOM: f32 = 0.1;
    pub const MAX_ZOOM: f32 = 30.0;

    /// Zooms by `factor`, keeping the screen point `anchor` still.
    pub fn zoomed(self, factor: f32, anchor: Point) -> Self {
        let zoom = (self.zoom * factor).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        let f = zoom / self.zoom;
        let pan = Vector::new(
            anchor.x - (anchor.x - self.pan.x) * f,
            anchor.y - (anchor.y - self.pan.y) * f,
        );
        Self { pan, zoom }
    }

    /// The zoom the geometry is built at: the power of two nearest `zoom`.
    pub fn build_zoom(self) -> f32 {
        2f32.powi(self.zoom.log2().round() as i32)
    }

    pub fn to_scene(self, p: Point) -> [f64; 2] {
        [
            f64::from((p.x - self.pan.x) / self.zoom),
            f64::from((p.y - self.pan.y) / self.zoom),
        ]
    }

    pub fn to_screen(self, p: [f64; 2]) -> Point {
        Point::new(
            p[0] as f32 * self.zoom + self.pan.x,
            p[1] as f32 * self.zoom + self.pan.y,
        )
    }
}

/// What the widget reports: pointer events in scene coordinates, or a new view.
#[derive(Debug, Clone)]
pub enum Input {
    Down([f64; 2], editor::Modifiers),
    Move([f64; 2], editor::Modifiers),
    Up([f64; 2], editor::Modifiers),
    DoubleClick([f64; 2]),
    View(View),
}

/// Time spent drawing, for the benchmark: (rebuilding caches, total).
#[derive(Default)]
pub struct Timings {
    pub frames: Vec<(Duration, Duration)>,
}

pub struct SceneView<'a, Message> {
    pub layers: &'a Layers,
    pub editor: &'a Editor,
    pub built_at: &'a RefCell<Option<f32>>,
    pub view: View,
    pub background: Color,
    /// Dragging pans instead of editing: the Hand tool, or Space held.
    pub panning: bool,
    pub timings: Option<&'a RefCell<Timings>>,
    pub on_input: Box<dyn Fn(Input) -> Message + 'a>,
}

#[derive(Default)]
struct State {
    /// A pan in progress: where it started, and the pan then.
    pan: Option<(Point, Vector)>,
    /// The left button went down on the drawing, for the editor.
    editing: bool,
    /// The last click, to tell a double click.
    last_click: Option<iced::advanced::mouse::Click>,
    modifiers: keyboard::Modifiers,
}

fn modifiers(m: keyboard::Modifiers) -> editor::Modifiers {
    editor::Modifiers {
        shift: m.shift(),
        alt: m.alt(),
        ctrl: m.command(),
    }
}

/// Excalidraw's selection colour.
const SELECTION: Color = Color::from_rgb(
    0x69 as f32 / 255.0,
    0x65 as f32 / 255.0,
    0xdb as f32 / 255.0,
);

fn outline(points: &[Point]) -> Path {
    Path::new(|b| {
        b.move_to(points[0]);
        for p in &points[1..] {
            b.line_to(*p);
        }
        b.close();
    })
}

impl<Message> SceneView<'_, Message> {
    /// Selection outlines, handles and the rubber band, in screen pixels.
    fn overlay(&self, frame: &mut Frame<Renderer>) {
        let to = |p: [f64; 2]| self.view.to_screen(p);
        let line = Stroke::default().with_width(1.0).with_color(SELECTION);
        let ed = self.editor;
        let selected: Vec<_> = ed
            .selection
            .iter()
            .filter_map(|id| ed.scene.by_id(id))
            .filter(|e| !e.is_deleted)
            .collect();
        if selected.len() > 1 {
            for e in &selected {
                frame.stroke(&outline(&editor::geometry::corners(e).map(to)), line);
            }
        }
        if let Some(h) = ed.handles() {
            frame.stroke(&outline(&h.corners.map(to)), line);
            let size = 8.0;
            for (handle, at) in &h.handles {
                let p = to(*at);
                let shape = if *handle == Handle::Rotation {
                    Path::circle(p, size / 2.0)
                } else {
                    Path::rectangle(
                        Point::new(p.x - size / 2.0, p.y - size / 2.0),
                        Size::new(size, size),
                    )
                };
                frame.fill(&shape, Color::WHITE);
                frame.stroke(&shape, line);
            }
        }
        if let Some(m) = ed.marquee {
            let (a, b) = (to([m[0], m[1]]), to([m[2], m[3]]));
            let rect = Path::rectangle(a, Size::new(b.x - a.x, b.y - a.y));
            frame.fill(
                &rect,
                Color {
                    a: 0.08,
                    ..SELECTION
                },
            );
            frame.stroke(&rect, line);
        }
    }
}

impl<Message, Theme> Widget<Message, Theme, Renderer> for SceneView<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let started = Instant::now();
        let bounds = layout.bounds();
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                ..renderer::Quad::default()
            },
            self.background,
        );

        let build_zoom = self.view.build_zoom();
        if *self.built_at.borrow() != Some(build_zoom) {
            self.layers.clear_caches();
            *self.built_at.borrow_mut() = Some(build_zoom);
        }
        // What part of the scene is on screen: chunks off it are skipped.
        let tl = self.view.to_scene(Point::ORIGIN);
        let br = self.view.to_scene(Point::new(bounds.width, bounds.height));
        let mut rebuilt = Duration::ZERO;
        let scale = self.view.zoom / build_zoom;
        let transform =
            Transformation::translate(bounds.x + self.view.pan.x, bounds.y + self.view.pan.y)
                * Transformation::scale(scale);
        renderer.with_layer(bounds, |renderer| {
            for chunk in &self.layers.chunks {
                let Some(b) = chunk.bounds else { continue };
                if b[2] < tl[0] || b[0] > br[0] || b[3] < tl[1] || b[1] > br[1] {
                    continue;
                }
                let rect = Rectangle::new(
                    Point::new(b[0] as f32 * build_zoom, b[1] as f32 * build_zoom),
                    Size::new(
                        ((b[2] - b[0]) as f32 * build_zoom).max(1.0),
                        ((b[3] - b[1]) as f32 * build_zoom).max(1.0),
                    ),
                );
                let geometry = chunk.cache.draw_with_bounds(renderer, rect, |frame| {
                    let t = Instant::now();
                    self.layers.draw_chunk(frame, chunk, build_zoom);
                    rebuilt += t.elapsed();
                });
                renderer
                    .with_transformation(transform, |renderer| renderer.draw_geometry(geometry));
            }
            let mut frame = Frame::new(renderer, bounds.size());
            self.overlay(&mut frame);
            renderer.with_translation(Vector::new(bounds.x, bounds.y), |renderer| {
                renderer.draw_geometry(frame.into_geometry())
            });
        });
        if let Some(timings) = self.timings {
            timings
                .borrow_mut()
                .frames
                .push((rebuilt, started.elapsed()));
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        let m = modifiers(state.modifiers);
        let local = |p: Point| p - Vector::new(bounds.x, bounds.y);
        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(mods)) => state.modifiers = *mods,
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                let Some(p) = cursor.position_in(bounds) else {
                    return;
                };
                if *button == mouse::Button::Middle
                    || (*button == mouse::Button::Left && self.panning)
                {
                    state.pan = Some((p, self.view.pan));
                } else if *button == mouse::Button::Left {
                    let click = iced::advanced::mouse::Click::new(p, *button, state.last_click);
                    state.last_click = Some(click);
                    if click.kind() == iced::advanced::mouse::click::Kind::Double {
                        shell.publish((self.on_input)(Input::DoubleClick(self.view.to_scene(p))));
                        shell.capture_event();
                        return;
                    }
                    state.editing = true;
                    shell.publish((self.on_input)(Input::Down(self.view.to_scene(p), m)));
                } else {
                    return;
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(_)) => {
                if state.pan.take().is_some() {
                    shell.capture_event();
                }
                if state.editing {
                    state.editing = false;
                    if let Some(p) = cursor.position() {
                        shell.publish((self.on_input)(Input::Up(self.view.to_scene(local(p)), m)));
                    }
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let p = local(*position);
                if let Some((start, pan)) = state.pan {
                    shell.publish((self.on_input)(Input::View(View {
                        pan: pan + (p - start),
                        ..self.view
                    })));
                    shell.capture_event();
                } else if state.editing || bounds.contains(*position) {
                    shell.publish((self.on_input)(Input::Move(self.view.to_scene(p), m)));
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let Some(p) = cursor.position_in(bounds) else {
                    return;
                };
                let (dx, dy) = match *delta {
                    ScrollDelta::Lines { x, y } => (x * 60.0, y * 60.0),
                    ScrollDelta::Pixels { x, y } => (x, y),
                };
                // As in Excalidraw: the wheel pans, with Ctrl it zooms.
                let view = if state.modifiers.command() {
                    self.view.zoomed((dy / 300.0).exp(), p)
                } else if state.modifiers.shift() {
                    View {
                        pan: self.view.pan + Vector::new(dy, 0.0),
                        ..self.view
                    }
                } else {
                    View {
                        pan: self.view.pan + Vector::new(dx, dy),
                        ..self.view
                    }
                };
                shell.publish((self.on_input)(Input::View(view)));
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if state.pan.is_some() {
            return mouse::Interaction::Grabbing;
        }
        if self.panning {
            return mouse::Interaction::Grab;
        }
        let Some(p) = cursor.position_in(layout.bounds()) else {
            return mouse::Interaction::None;
        };
        match self.editor.cursor_at(self.view.to_scene(p)) {
            Cursor::Default => mouse::Interaction::None,
            Cursor::Move => mouse::Interaction::Move,
            Cursor::Crosshair => mouse::Interaction::Crosshair,
            Cursor::Grab | Cursor::Resize(Handle::Rotation) => mouse::Interaction::Grab,
            Cursor::Resize(Handle::N | Handle::S) => mouse::Interaction::ResizingVertically,
            Cursor::Resize(Handle::E | Handle::W) => mouse::Interaction::ResizingHorizontally,
            Cursor::Resize(Handle::NE | Handle::SW) => mouse::Interaction::ResizingDiagonallyUp,
            Cursor::Resize(_) => mouse::Interaction::ResizingDiagonallyDown,
        }
    }
}

impl<'a, Message: 'a, Theme: 'a> From<SceneView<'a, Message>>
    for Element<'a, Message, Theme, Renderer>
{
    fn from(view: SceneView<'a, Message>) -> Self {
        Element::new(view)
    }
}
