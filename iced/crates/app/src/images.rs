//! Embedded images: the `files` of a drawing, decoded once per element.
//!
//! A drawing someone sends you is untrusted input (plan, "Security"), so
//! decoding is bounded: by the image crate's own limits on dimensions and
//! memory, and to the formats a browser would show.

use base64::Engine;
use iced::widget::image::Handle;
use scene::{Element, Scene};

/// Larger than any image Excalidraw would accept, small enough that a
/// hostile file cannot ask for gigabytes.
const MAX_DIMENSION: u32 = 16_384;
const MAX_ALLOC: u64 = 512 * 1024 * 1024;

/// The pixels an image element shows: its file, cropped and flipped as the
/// element says. None when the file is missing or cannot be decoded, which
/// Excalidraw draws as a grey placeholder.
pub fn decode(scene: &Scene, element: &Element) -> Option<Handle> {
    let img = element.image.as_ref()?;
    let (_mime, data_url) = scene.file(img.file_id.as_deref()?)?;
    let (_, payload) = data_url.split_once(";base64,")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .ok()?;

    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    let mut rgba = reader.decode().ok()?.to_rgba8();

    if let Some([x, y, w, h, natural_w, natural_h]) = img.crop {
        // Crop is in the natural size's pixels; the decoded image may differ.
        let sx = f64::from(rgba.width()) / natural_w.max(1.0);
        let sy = f64::from(rgba.height()) / natural_h.max(1.0);
        let cx = (x * sx).round().max(0.0) as u32;
        let cy = (y * sy).round().max(0.0) as u32;
        let cw = ((w * sx).round() as u32)
            .min(rgba.width().saturating_sub(cx))
            .max(1);
        let ch = ((h * sy).round() as u32)
            .min(rgba.height().saturating_sub(cy))
            .max(1);
        rgba = image::imageops::crop_imm(&rgba, cx, cy, cw, ch).to_image();
    }
    // A negative scale is a flip about the centre.
    if img.scale[0] < 0.0 {
        image::imageops::flip_horizontal_in_place(&mut rgba);
    }
    if img.scale[1] < 0.0 {
        image::imageops::flip_vertical_in_place(&mut rgba);
    }
    let (w, h) = rgba.dimensions();
    Some(Handle::from_rgba(w, h, rgba.into_raw()))
}
