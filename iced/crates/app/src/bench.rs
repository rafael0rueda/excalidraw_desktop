//! `--bench`: the pan-then-zoom animation the phase 1 gate was measured with.

use crate::view::{Timings, View};
use iced::{Point, Size, Vector};
use std::time::{Duration, Instant};

pub struct Bench {
    seconds: f32,
    /// This process's CPU time when the animation started.
    cpu_at_start: f64,
    started: Option<Instant>,
    pub base: View,
    last: Option<Instant>,
    /// Intervals between presented frames: (seconds into the run, interval).
    intervals: Vec<(f32, Duration)>,
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

impl Bench {
    pub fn new(seconds: f32, base: View) -> Self {
        Self {
            seconds,
            cpu_at_start: 0.0,
            started: None,
            base,
            last: None,
            intervals: Vec::new(),
        }
    }

    /// The view for this frame, or None when the run is over.
    pub fn tick(&mut self, now: Instant, launched: Instant, size: Size) -> Option<View> {
        if self.started.is_none() {
            self.cpu_at_start = cpu_seconds();
            eprintln!(
                "first frame {:.0} ms after launch",
                launched.elapsed().as_secs_f64() * 1e3
            );
        }
        let started = *self.started.get_or_insert(now);
        let t = (now - started).as_secs_f32();
        if let Some(last) = self.last {
            self.intervals.push((t, now - last));
        }
        self.last = Some(now);
        if t >= self.seconds {
            return None;
        }
        let base = self.base;
        Some(if t < self.seconds / 2.0 {
            // Pan: a circle 300 px across, once a second, at the fitted zoom.
            let a = t * std::f32::consts::TAU;
            View {
                pan: base.pan + Vector::new(a.cos() * 150.0, a.sin() * 150.0),
                zoom: base.zoom,
            }
        } else {
            // Zoom: from the fit to 4× and back, every two seconds, about the centre.
            let s = (1.0 - ((t - self.seconds / 2.0) * std::f32::consts::PI).cos()) / 2.0;
            base.zoomed(
                4f32.powf(s),
                Point::new(size.width / 2.0, size.height / 2.0),
            )
        })
    }

    pub fn report(&self, timings: &Timings, elements: usize, load_time: Duration) {
        let half = self.seconds / 2.0;
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
            self.intervals
                .iter()
                .filter(|(t, _)| *t < half)
                .map(|(_, d)| *d)
                .collect(),
        );
        stats(
            "zoom: frame interval",
            self.intervals
                .iter()
                .filter(|(t, _)| *t >= half)
                .map(|(_, d)| *d)
                .collect(),
        );
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
        let cpu = cpu_seconds() - self.cpu_at_start;
        let frames = self.intervals.len();
        println!(
            "CPU, all processes          {cpu:.2} s over {frames} frames = {:.2} ms/frame",
            cpu * 1e3 / frames.max(1) as f64
        );
        println!(
            "elements {elements}  load+prepare {:.1} ms",
            load_time.as_secs_f64() * 1e3
        );
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status
                .lines()
                .filter(|l| l.starts_with("VmRSS") || l.starts_with("VmHWM"))
            {
                println!("{line}");
            }
        }
    }
}
