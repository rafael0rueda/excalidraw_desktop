//! SVG images inside a drawing, for the canvas, which only takes pixels.
//! As untrusted as the drawing: nothing outside the file is read (no
//! linked images, no files), and the pixels are bounded.

use resvg::{tiny_skia, usvg};
use std::sync::{Arc, OnceLock};

/// The desktop's fonts, for text inside SVG images, as a browser would use
/// them. Loaded once, on the first SVG image.
fn fonts() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        // fontdb names Windows fonts for the generic families; take the
        // first a Linux desktop has instead.
        let names: std::collections::HashSet<String> = db.faces().flat_map(|f| f.families.iter().map(|(n, _)| n.clone())).collect();
        let first = |options: &[&str]| options.iter().find(|n| names.contains(**n)).map(|n| (*n).to_owned());
        let (sans, serif, mono) = (
            first(&["DejaVu Sans", "Noto Sans", "Liberation Sans", "Adwaita Sans", "Cantarell"]),
            first(&["DejaVu Serif", "Noto Serif", "Liberation Serif"]),
            first(&["DejaVu Sans Mono", "Noto Sans Mono", "Liberation Mono", "Adwaita Mono"]),
        );
        if let Some(f) = sans {
            db.set_sans_serif_family(f);
        }
        if let Some(f) = serif {
            db.set_serif_family(f);
        }
        if let Some(f) = mono {
            db.set_monospace_family(f);
        }
        Arc::new(db)
    })
    .clone()
}

fn tree(data: &[u8]) -> Option<usvg::Tree> {
    let mut options = usvg::Options { fontdb: fonts(), ..usvg::Options::default() };
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    usvg::Tree::from_data(data, &options).ok()
}

/// The image's own size, as Excalidraw takes it.
pub fn svg_size(data: &[u8]) -> Option<(f64, f64)> {
    let size = tree(data)?.size();
    Some((f64::from(size.width()), f64::from(size.height())))
}

/// The image as straight RGBA, its longer side `max_side` pixels at most
/// (enlarged, up to 16 times, when smaller, so it stays sharp when zoomed).
pub fn svg_rgba(data: &[u8], max_side: u32) -> Option<(u32, u32, Vec<u8>)> {
    let tree = tree(data)?;
    let size = tree.size();
    let scale = (max_side as f32 / size.width().max(size.height())).min(16.0);
    let (w, h) = ((size.width() * scale).ceil().max(1.0) as u32, (size.height() * scale).ceil().max(1.0) as u32);
    let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    Some(crate::straight(pixmap))
}
