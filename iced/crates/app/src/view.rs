//! The scene widget: the drawing tessellated once, then panned and zoomed
//! by transforming the cached meshes rather than rebuilding them.
//!
//! iced's own `Canvas` cannot do that: its cached geometry is drawn where it
//! was built, so every pan would re-tessellate every element. This widget
//! draws the cache under `renderer.with_transformation`, and rebuilds it
//! only when the zoom leaves the power-of-two bucket it was built at, so
//! curves stay smooth when zoomed in (plan, W1).

use crate::paint::{self, Images, Renderer};
use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{tree, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::mouse::{self, ScrollDelta};
use iced::widget::canvas::Cache;
use iced::{
    keyboard, Color, Element, Event, Length, Point, Rectangle, Size, Transformation, Vector,
};
use render::Prepared;
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

    /// Zooms by `factor`, keeping the scene point under `anchor` still.
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
}

pub struct Loaded {
    pub elements: Vec<(String, Prepared)>,
    pub images: Images,
    pub background: Color,
    /// The union of every element's bounds.
    pub bounds: [f64; 4],
}

/// Time spent drawing, for the benchmark: (rebuilding the cache, total).
#[derive(Default)]
pub struct Timings {
    pub frames: Vec<(Duration, Duration)>,
}

pub struct SceneView<'a, Message> {
    loaded: &'a Loaded,
    cache: &'a Cache,
    built_at: &'a RefCell<Option<f32>>,
    view: View,
    timings: Option<&'a RefCell<Timings>>,
    on_view: Box<dyn Fn(View) -> Message + 'a>,
}

impl<'a, Message> SceneView<'a, Message> {
    pub fn new(
        loaded: &'a Loaded,
        cache: &'a Cache,
        built_at: &'a RefCell<Option<f32>>,
        view: View,
        timings: Option<&'a RefCell<Timings>>,
        on_view: impl Fn(View) -> Message + 'a,
    ) -> Self {
        Self {
            loaded,
            cache,
            built_at,
            view,
            timings,
            on_view: Box::new(on_view),
        }
    }
}

#[derive(Default)]
struct State {
    /// Where a drag started, and the pan at that moment.
    drag: Option<(Point, Vector)>,
    modifiers: keyboard::Modifiers,
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
            self.loaded.background,
        );

        let build_zoom = self.view.build_zoom();
        if *self.built_at.borrow() != Some(build_zoom) {
            self.cache.clear();
            *self.built_at.borrow_mut() = Some(build_zoom);
        }
        let [x0, y0, x1, y1] = self.loaded.bounds;
        let scene_rect = Rectangle::new(
            Point::new(x0 as f32 * build_zoom, y0 as f32 * build_zoom),
            Size::new(
                ((x1 - x0) as f32 * build_zoom).max(1.0),
                ((y1 - y0) as f32 * build_zoom).max(1.0),
            ),
        );
        let mut rebuilt = Duration::ZERO;
        let geometry = self.cache.draw_with_bounds(renderer, scene_rect, |frame| {
            let t = Instant::now();
            for (id, prepared) in &self.loaded.elements {
                paint::element(frame, id, prepared, &self.loaded.images, build_zoom);
            }
            rebuilt = t.elapsed();
        });

        let scale = self.view.zoom / build_zoom;
        let transform =
            Transformation::translate(bounds.x + self.view.pan.x, bounds.y + self.view.pan.y)
                * Transformation::scale(scale);
        renderer.with_layer(bounds, |renderer| {
            renderer.with_transformation(transform, |renderer| renderer.draw_geometry(geometry));
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
        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => state.modifiers = *m,
            Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Left | mouse::Button::Middle,
            )) => {
                if let Some(p) = cursor.position_in(bounds) {
                    state.drag = Some((p, self.view.pan));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(
                mouse::Button::Left | mouse::Button::Middle,
            )) => {
                state.drag = None;
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let (Some((start, pan)), Some(p)) = (state.drag, cursor.position_in(bounds)) {
                    let view = View {
                        pan: pan + (p - start),
                        ..self.view
                    };
                    shell.publish((self.on_view)(view));
                    shell.capture_event();
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
                let view = if state.modifiers.control() {
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
                shell.publish((self.on_view)(view));
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if tree.state.downcast_ref::<State>().drag.is_some() {
            mouse::Interaction::Grabbing
        } else {
            mouse::Interaction::Grab
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
