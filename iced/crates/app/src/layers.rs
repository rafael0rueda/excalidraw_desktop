//! The drawing, prepared and tessellated in chunks.
//!
//! Elements are prepared once per change (`render::prepare`, as Excalidraw's
//! `ShapeCache`), and tessellated in chunks of consecutive elements, each
//! with its own geometry cache. An edit invalidates only the chunks holding
//! what changed: dragging one element in a 10 000-element drawing
//! re-tessellates 256 of them, not all (plan, phase 1 follow-up: a whole-scene
//! rebuild took 72 ms).

use crate::paint::{self, Images};
use editor::Changes;
use iced::widget::canvas::Cache;
use render::Prepared;
use scene::Scene;
use std::collections::HashMap;

/// Elements per chunk: small enough to re-tessellate within a frame, large
/// enough that drawing the chunks is not many small draws.
const CHUNK: usize = 256;

pub type Bounds = [f64; 4];

pub struct Chunk {
    pub ids: Vec<String>,
    pub cache: Cache,
    pub bounds: Option<Bounds>,
}

pub struct Layers {
    pub prepared: HashMap<String, Prepared>,
    pub images: Images,
    pub chunks: Vec<Chunk>,
    chunk_of: HashMap<String, usize>,
    /// The canvas colour, which fills "outline" arrowheads.
    canvas_css: String,
}

fn union(a: Option<Bounds>, b: Bounds) -> Option<Bounds> {
    Some(match a {
        Some(a) => [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ],
        None => b,
    })
}

impl Layers {
    pub fn new(scene: &Scene, canvas_css: &str) -> Self {
        let mut layers = Self {
            prepared: HashMap::new(),
            images: Images::new(),
            chunks: Vec::new(),
            chunk_of: HashMap::new(),
            canvas_css: canvas_css.to_owned(),
        };
        for e in scene.visible() {
            layers.prepare(scene, &e.id);
        }
        layers.partition(scene);
        layers
    }

    fn prepare(&mut self, scene: &Scene, id: &str) {
        match scene.by_id(id).filter(|e| !e.is_deleted) {
            Some(e) => {
                let p = render::prepare(e, scene, &self.canvas_css);
                if e.image.is_some() && !self.images.contains_key(id) {
                    if let Some(h) = crate::images::decode(scene, e) {
                        self.images.insert(id.to_owned(), h);
                    }
                }
                if p.items.is_empty() {
                    self.prepared.remove(id);
                } else {
                    self.prepared.insert(id.to_owned(), p);
                }
            }
            None => {
                self.prepared.remove(id);
                self.images.remove(id);
            }
        }
    }

    /// Splits the stacking order into chunks, keeping the caches of chunks
    /// whose elements are the same as before.
    fn partition(&mut self, scene: &Scene) {
        let order: Vec<String> = scene
            .visible()
            .map(|e| e.id.clone())
            .filter(|id| self.prepared.contains_key(id))
            .collect();
        let mut old: HashMap<Vec<String>, Cache> =
            self.chunks.drain(..).map(|c| (c.ids, c.cache)).collect();
        self.chunk_of.clear();
        for (i, ids) in order.chunks(CHUNK).enumerate() {
            let ids = ids.to_vec();
            for id in &ids {
                self.chunk_of.insert(id.clone(), i);
            }
            let cache = old.remove(&ids).unwrap_or_default();
            let bounds = ids
                .iter()
                .filter_map(|id| self.prepared.get(id))
                .fold(None, |b, p| union(b, p.bounds));
            self.chunks.push(Chunk { ids, cache, bounds });
        }
    }

    /// Brings everything up to date with what the editor changed.
    pub fn apply(&mut self, scene: &Scene, changes: Changes) {
        if changes.ids.is_empty() && !changes.order {
            return;
        }
        let mut dirty = std::collections::HashSet::new();
        for id in &changes.ids {
            if let Some(&c) = self.chunk_of.get(id) {
                dirty.insert(c);
            }
            self.prepare(scene, id);
        }
        // Anything appearing, disappearing or restacking reshapes the chunks.
        let order_changed = changes.order
            || changes
                .ids
                .iter()
                .any(|id| self.prepared.contains_key(id) != self.chunk_of.contains_key(id));
        if order_changed {
            self.partition(scene);
        }
        for id in &changes.ids {
            if let Some(&c) = self.chunk_of.get(id) {
                dirty.insert(c);
            }
        }
        for c in dirty {
            if let Some(chunk) = self.chunks.get_mut(c) {
                chunk.cache.clear();
                chunk.bounds = chunk
                    .ids
                    .iter()
                    .filter_map(|id| self.prepared.get(id))
                    .fold(None, |b, p| union(b, p.bounds));
            }
        }
    }

    /// The zoom bucket changed: every chunk is tessellated again.
    pub fn clear_caches(&self) {
        for c in &self.chunks {
            c.cache.clear();
        }
    }

    pub fn bounds(&self) -> Option<Bounds> {
        self.chunks.iter().filter_map(|c| c.bounds).reduce(|a, b| {
            [
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ]
        })
    }

    pub fn draw_chunk(
        &self,
        frame: &mut iced::widget::canvas::Frame<paint::Renderer>,
        chunk: &Chunk,
        zoom: f32,
    ) {
        for id in &chunk.ids {
            if let Some(p) = self.prepared.get(id) {
                paint::element(frame, id, p, &self.images, zoom);
            }
        }
    }
}
