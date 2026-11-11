//! Excalidraw Desktop on Iced — phase 1, a read-only viewer.
//!
//!   excalidraw-iced drawing.excalidraw
//!   excalidraw-iced --bench 10 drawing.excalidraw
//!
//! `--bench SECONDS` pans, then zooms, for that long and prints frame timings,
//! memory and start-up time, for the go/no-go gate in docs/iced-port/PLAN.md.
//! `--screenshot out.png [--zoom F]` saves the window once drawn, and exits:
//! the drawing fitted to the window, then zoomed F times about its centre.
//! `NO_MSAA=1` turns antialiasing off, to separate what MSAA does to thin
//! lines (it thickens them; see PLAN.md) from what the geometry is.

mod images;
mod paint;
mod view;

include!(concat!(env!("OUT_DIR"), "/fonts.rs"));

use iced::widget::canvas::Cache;
use iced::widget::{container, text};
use iced::{window, Element, Length, Point, Size, Subscription, Task, Vector};
use std::cell::RefCell;
use std::time::{Duration, Instant};
use view::{Loaded, SceneView, Timings, View};

const WINDOW: Size = Size::new(1280.0, 800.0);

#[derive(Clone, Debug)]
struct Args {
    path: Option<String>,
    bench: Option<f32>,
    screenshot: Option<String>,
    zoom: f32,
}

fn args() -> Args {
    let mut args = Args {
        path: None,
        bench: None,
        screenshot: None,
        zoom: 1.0,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        if a == "--bench" {
            args.bench = it.next().and_then(|s| s.parse().ok());
        } else if a == "--screenshot" {
            args.screenshot = it.next();
        } else if a == "--zoom" {
            args.zoom = it.next().and_then(|s| s.parse().ok()).unwrap_or(1.0);
        } else {
            args.path = Some(a);
        }
    }
    args
}

#[derive(Debug, Clone)]
enum Message {
    View(View),
    Resized(Size),
    Tick(Instant),
    Shot(window::Screenshot),
}

struct Bench {
    seconds: f32,
    /// This process's CPU time when the animation started.
    cpu_at_start: f64,
    started: Option<Instant>,
    base: View,
    last: Option<Instant>,
    /// Intervals between presented frames: (seconds into the run, interval).
    intervals: Vec<(f32, Duration)>,
}

struct App {
    title: String,
    loaded: Result<Loaded, String>,
    view: View,
    size: Size,
    cache: Cache,
    built_at: RefCell<Option<f32>>,
    timings: RefCell<Timings>,
    bench: Option<Bench>,
    launched: Instant,
    load_time: Duration,
    screenshot: Option<String>,
    zoom: f32,
    ticks: u32,
}

fn load(path: &str) -> Result<Loaded, String> {
    let path = std::path::Path::new(path);
    excalidraw_desktop_core::files::readable(path, excalidraw_desktop_core::files::MAX_TEXT_BYTES)?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let scene = scene::Scene::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    // This app's theme owns the canvas colour (PROGRESS.md); the viewer uses
    // the drawing's own until themes arrive in phase 5.
    let background_css = scene
        .view_background_color()
        .unwrap_or("#ffffff")
        .to_owned();
    let mut elements = Vec::new();
    let mut images = paint::Images::new();
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for e in scene.visible() {
        let prepared = render::prepare(e, &scene, &background_css);
        if prepared.items.is_empty() {
            continue;
        }
        let b = prepared.bounds;
        bounds = [
            bounds[0].min(b[0]),
            bounds[1].min(b[1]),
            bounds[2].max(b[2]),
            bounds[3].max(b[3]),
        ];
        if let Some(handle) = images::decode(&scene, e) {
            images.insert(e.id.clone(), handle);
        }
        elements.push((e.id.clone(), prepared));
    }
    if elements.is_empty() {
        bounds = [0.0, 0.0, 1.0, 1.0];
    }
    let bg = render::color::parse(&background_css);
    Ok(Loaded {
        elements,
        images,
        background: iced::Color::from_rgba(bg.r, bg.g, bg.b, bg.a),
        bounds,
    })
}

/// The view that shows the whole drawing, with a margin.
fn fit(bounds: [f64; 4], size: Size) -> View {
    let (w, h) = (
        (bounds[2] - bounds[0]) as f32,
        (bounds[3] - bounds[1]) as f32,
    );
    let zoom = ((size.width - 80.0) / w.max(1.0))
        .min((size.height - 80.0) / h.max(1.0))
        .clamp(View::MIN_ZOOM, 1.0);
    let pan = Vector::new(
        (size.width - w * zoom) / 2.0 - bounds[0] as f32 * zoom,
        (size.height - h * zoom) / 2.0 - bounds[1] as f32 * zoom,
    );
    View { pan, zoom }
}

impl App {
    fn boot(args: Args) -> (Self, Task<Message>) {
        let launched = Instant::now();
        let loaded = match &args.path {
            Some(path) => load(path),
            None => Err("Usage: excalidraw-iced <drawing.excalidraw>".into()),
        };
        let load_time = launched.elapsed();
        let centre = Point::new(WINDOW.width / 2.0, WINDOW.height / 2.0);
        let view = loaded.as_ref().map_or(
            View {
                pan: Vector::ZERO,
                zoom: 1.0,
            },
            |l| fit(l.bounds, WINDOW).zoomed(args.zoom, centre),
        );
        let title = args.path.as_deref().map_or("Excalidraw".into(), |p| {
            let name = std::path::Path::new(p)
                .file_name()
                .map_or(p.into(), |n| n.to_string_lossy().into_owned());
            format!("{name} — Excalidraw (Iced)")
        });
        if let Ok(l) = &loaded {
            eprintln!(
                "loaded {} elements in {:.1} ms",
                l.elements.len(),
                load_time.as_secs_f64() * 1e3
            );
            let b = l.bounds;
            // For iced/bench/webkit.py, which shows Excalidraw at the same view.
            eprintln!("bounds {} {} {} {}", b[0], b[1], b[2], b[3]);
        }
        let bench = args.bench.map(|seconds| Bench {
            seconds,
            cpu_at_start: 0.0,
            started: None,
            base: view,
            last: None,
            intervals: Vec::new(),
        });
        let app = Self {
            title,
            loaded,
            view,
            size: WINDOW,
            cache: Cache::new(),
            built_at: RefCell::new(None),
            timings: RefCell::new(Timings::default()),
            bench,
            launched,
            load_time,
            screenshot: args.screenshot,
            zoom: args.zoom,
            ticks: 0,
        };
        (app, Task::none())
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::View(view) => self.view = view,
            Message::Resized(size) => {
                let first = self.size == WINDOW;
                self.size = size;
                if first {
                    if let Ok(l) = &self.loaded {
                        self.view = fit(l.bounds, size)
                            .zoomed(self.zoom, Point::new(size.width / 2.0, size.height / 2.0));
                        if let Some(b) = &mut self.bench {
                            b.base = self.view;
                        }
                    }
                }
            }
            Message::Shot(shot) => {
                if let Some(path) = &self.screenshot {
                    let saved = image::RgbaImage::from_raw(
                        shot.size.width,
                        shot.size.height,
                        shot.rgba.to_vec(),
                    )
                    .map(|img| img.save(path));
                    match saved {
                        Some(Ok(())) => eprintln!("saved {path}"),
                        _ => eprintln!("could not save {path}"),
                    }
                }
                return iced::exit();
            }
            Message::Tick(now) => {
                if self.screenshot.is_some() {
                    self.ticks += 1;
                    // A few frames in, so the window has its size and the fonts are in.
                    if self.ticks == 5 {
                        return window::latest()
                            .and_then(window::screenshot)
                            .map(Message::Shot);
                    }
                    return Task::none();
                }
                let Some(bench) = &mut self.bench else {
                    return Task::none();
                };
                if bench.started.is_none() {
                    bench.cpu_at_start = cpu_seconds();
                }
                let started = *bench.started.get_or_insert_with(|| {
                    eprintln!(
                        "first frame {:.0} ms after launch",
                        self.launched.elapsed().as_secs_f64() * 1e3
                    );
                    now
                });
                let t = (now - started).as_secs_f32();
                if let Some(last) = bench.last {
                    bench.intervals.push((t, now - last));
                }
                bench.last = Some(now);
                let half = bench.seconds / 2.0;
                let base = bench.base;
                self.view = if t < half {
                    // Pan: a circle 300 px across, once a second, at the fitted zoom.
                    let a = t * std::f32::consts::TAU;
                    View {
                        pan: base.pan + Vector::new(a.cos() * 150.0, a.sin() * 150.0),
                        zoom: base.zoom,
                    }
                } else {
                    // Zoom: from the fit to 4× and back, every two seconds, about the centre.
                    let s = (1.0 - ((t - half) * std::f32::consts::PI).cos()) / 2.0;
                    base.zoomed(
                        4f32.powf(s),
                        Point::new(self.size.width / 2.0, self.size.height / 2.0),
                    )
                };
                if t >= bench.seconds {
                    self.report();
                    return iced::exit();
                }
            }
        }
        Task::none()
    }

    fn report(&self) {
        let Some(bench) = &self.bench else { return };
        let half = bench.seconds / 2.0;
        let stats = |name: &str, xs: Vec<Duration>| {
            if xs.is_empty() {
                return;
            }
            let mut ms: Vec<f64> = xs.iter().map(|d| d.as_secs_f64() * 1e3).collect();
            ms.sort_by(f64::total_cmp);
            let mean = ms.iter().sum::<f64>() / ms.len() as f64;
            let p = |q: f64| ms[((ms.len() - 1) as f64 * q) as usize];
            println!(
                "{name:<28} n={:<5} mean {mean:6.2} ms  p50 {:6.2}  p95 {:6.2}  max {:6.2}  ({:.0} fps)",
                ms.len(),
                p(0.5),
                p(0.95),
                ms[ms.len() - 1],
                1e3 / mean
            );
        };
        stats(
            "pan: frame interval",
            bench
                .intervals
                .iter()
                .filter(|(t, _)| *t < half)
                .map(|(_, d)| *d)
                .collect(),
        );
        stats(
            "zoom: frame interval",
            bench
                .intervals
                .iter()
                .filter(|(t, _)| *t >= half)
                .map(|(_, d)| *d)
                .collect(),
        );
        let timings = self.timings.borrow();
        stats(
            "draw call (CPU)",
            timings.frames.iter().map(|(_, total)| *total).collect(),
        );
        stats(
            "tessellation rebuilds",
            timings
                .frames
                .iter()
                .map(|(r, _)| *r)
                .filter(|r| !r.is_zero())
                .collect(),
        );
        let cpu = cpu_seconds() - bench.cpu_at_start;
        let frames = bench.intervals.len();
        println!(
            "CPU, all processes          {cpu:.2} s over {frames} frames = {:.2} ms/frame",
            cpu * 1e3 / frames.max(1) as f64
        );
        if let Ok(l) = &self.loaded {
            println!(
                "elements {}  load+prepare {:.1} ms",
                l.elements.len(),
                self.load_time.as_secs_f64() * 1e3
            );
        }
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status
                .lines()
                .filter(|l| l.starts_with("VmRSS") || l.starts_with("VmHWM"))
            {
                println!("{line}");
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        match &self.loaded {
            Ok(loaded) => SceneView::new(
                loaded,
                &self.cache,
                &self.built_at,
                self.view,
                self.bench.as_ref().map(|_| &self.timings),
                Message::View,
            )
            .into(),
            Err(e) => container(text(e.clone())).center(Length::Fill).into(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let resize = window::resize_events().map(|(_, size)| Message::Resized(size));
        if self.bench.is_some() || self.screenshot.is_some() {
            Subscription::batch([resize, window::frames().map(Message::Tick)])
        } else {
            resize
        }
    }

    fn title(&self) -> String {
        self.title.clone()
    }
}

/// User + system CPU time of this process, from /proc (Linux).
fn cpu_seconds() -> f64 {
    let Ok(stat) = std::fs::read_to_string("/proc/self/stat") else {
        return 0.0;
    };
    let Some(rest) = stat.rfind(')').map(|i| &stat[i + 2..]) else {
        return 0.0;
    };
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let ticks: f64 = fields.get(11).and_then(|v| v.parse().ok()).unwrap_or(0.0)
        + fields.get(12).and_then(|v| v.parse().ok()).unwrap_or(0.0);
    // CLK_TCK is 100 on every Linux this targets.
    ticks / 100.0
}

fn main() -> iced::Result {
    let args = args();
    let mut app = iced::application(move || App::boot(args.clone()), App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .window_size(WINDOW)
        .antialiasing(std::env::var_os("NO_MSAA").is_none());
    for (_, bytes) in FONTS {
        app = app.font(*bytes);
    }
    app.run()
}
