//! Embedded images: the `files` of a drawing, decoded once per element, and
//! image files made ready to go into one.
//!
//! A drawing someone sends you is untrusted input (plan, "Security"), so
//! decoding is bounded: by the image crate's own limits on dimensions and
//! memory, and to the formats a browser would show. SVG images are drawn
//! by resvg without reaching outside the file.

use base64::Engine;
use editor::ImageFile;
use iced::widget::image::Handle;
use scene::{Element, Scene};

/// Larger than any image Excalidraw would accept, small enough that a
/// hostile file cannot ask for gigabytes.
const MAX_DIMENSION: u32 = 16_384;
const MAX_ALLOC: u64 = 512 * 1024 * 1024;
const SVG: &str = "image/svg+xml";
/// An SVG image is drawn this many pixels along its longer side.
const SVG_SIDE: u32 = 2048;
/// `DEFAULT_MAX_IMAGE_WIDTH_OR_HEIGHT`: larger images are shrunk on the way in.
const MAX_SIDE: u32 = 1440;
/// `MAX_ALLOWED_FILE_BYTES`, after shrinking.
const MAX_BYTES: usize = 4 * 1024 * 1024;
/// What is read of a picked file at most.
pub const MAX_READ: u64 = 64 * 1024 * 1024;
/// The extensions Excalidraw's image tool offers (`IMAGE_MIME_TYPES`).
pub const EXTENSIONS: [&str; 10] = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "avif", "jfif", "svg"];

/// The pixels an image element shows: its file, cropped and flipped as the
/// element says. None when the file is missing or cannot be decoded, which
/// Excalidraw draws as a grey placeholder.
pub fn decode(scene: &Scene, element: &Element) -> Option<Handle> {
    let img = element.image.as_ref()?;
    let (mime, data_url) = scene.file(img.file_id.as_deref()?)?;
    let (_, payload) = data_url.split_once(";base64,")?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(payload.trim()).ok()?;
    let mut rgba = if mime == SVG || data_url.starts_with("data:image/svg+xml") {
        let (w, h, px) = export::svg_rgba(&bytes, SVG_SIDE)?;
        image::RgbaImage::from_raw(w, h, px)?
    } else {
        raster(&bytes)?.to_rgba8()
    };

    if let Some([x, y, w, h, natural_w, natural_h]) = img.crop {
        // Crop is in the natural size's pixels; the decoded image may differ.
        let sx = f64::from(rgba.width()) / natural_w.max(1.0);
        let sy = f64::from(rgba.height()) / natural_h.max(1.0);
        let cx = (x * sx).round().max(0.0) as u32;
        let cy = (y * sy).round().max(0.0) as u32;
        let cw = ((w * sx).round() as u32).min(rgba.width().saturating_sub(cx)).max(1);
        let ch = ((h * sy).round() as u32).min(rgba.height().saturating_sub(cy)).max(1);
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

/// A raster image, decoded within the limits.
fn raster(bytes: &[u8]) -> Option<image::DynamicImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    reader.decode().ok()
}

fn is_svg(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_ascii_lowercase();
    head.contains("<svg") || (head.trim_start().starts_with("<?xml") && String::from_utf8_lossy(bytes).contains("<svg"))
}

/// An image file as Excalidraw's `initializeImage` takes it in: its id the
/// SHA-1 of the file as given, shrunk to 1440 pixels along its longer side
/// (SVG kept as it is), refused above 4 MB after that.
pub fn prepare(bytes: Vec<u8>) -> Result<ImageFile, String> {
    let id = sha1_smol::Sha1::from(&bytes).digest().to_string();
    let (mime, bytes, width, height) = if is_svg(&bytes) {
        let (w, h) = export::svg_size(&bytes).ok_or("This SVG image could not be read.")?;
        (SVG.to_owned(), bytes, w, h)
    } else {
        let format = image::guess_format(&bytes).map_err(|_| "This is not an image Excalidraw can show.".to_owned())?;
        let img = raster(&bytes).ok_or("This image could not be read.")?;
        let (w, h) = (img.width(), img.height());
        if w.max(h) > MAX_SIDE {
            // Shrunk, in the same format where that can be written.
            let small = img.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Lanczos3);
            let mut out = Vec::new();
            let (mime, encoded) = match format {
                image::ImageFormat::Jpeg => {
                    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92);
                    let rgb = small.to_rgb8();
                    enc.encode_image(&rgb).map_err(|e| e.to_string())?;
                    ("image/jpeg", out)
                }
                _ => {
                    small.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).map_err(|e| e.to_string())?;
                    ("image/png", out)
                }
            };
            (mime.to_owned(), encoded, f64::from(small.width()), f64::from(small.height()))
        } else {
            let mime = match format {
                image::ImageFormat::Png => "image/png",
                image::ImageFormat::Jpeg => "image/jpeg",
                image::ImageFormat::Gif => "image/gif",
                image::ImageFormat::WebP => "image/webp",
                image::ImageFormat::Bmp => "image/bmp",
                image::ImageFormat::Ico => "image/x-icon",
                image::ImageFormat::Avif => "image/avif",
                _ => return Err("This is not an image Excalidraw can show.".into()),
            };
            (mime.to_owned(), bytes, f64::from(w), f64::from(h))
        }
    };
    if bytes.len() > MAX_BYTES {
        return Err("The image is larger than 4 MB, Excalidraw's limit.".into());
    }
    let data_url = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(&bytes));
    Ok(ImageFile { id, mime, data_url, width, height })
}

/// Pixels from the clipboard, as a PNG file.
pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Result<ImageFile, String> {
    let img = image::RgbaImage::from_raw(width, height, rgba).ok_or("The clipboard image is damaged.")?;
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    prepare(png)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_are_prepared_as_excalidraw_takes_them_in() {
        // A small PNG stays as it is, with its SHA-1 as its id.
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(30, 20, image::Rgba([255, 0, 0, 255])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let f = prepare(png.clone()).unwrap();
        assert_eq!(f.id, sha1_smol::Sha1::from(&png).digest().to_string());
        assert_eq!((f.mime.as_str(), f.width, f.height), ("image/png", 30.0, 20.0));
        assert!(f.data_url.starts_with("data:image/png;base64,"));

        // A large one is shrunk to 1440 along its longer side; the id is
        // still the original's.
        let mut big = Vec::new();
        image::RgbImage::from_pixel(2880, 1000, image::Rgb([0, 128, 0])).write_to(&mut std::io::Cursor::new(&mut big), image::ImageFormat::Jpeg).unwrap();
        let f = prepare(big.clone()).unwrap();
        assert_eq!((f.mime.as_str(), f.width, f.height), ("image/jpeg", 1440.0, 500.0));
        assert_eq!(f.id, sha1_smol::Sha1::from(&big).digest().to_string());

        // SVG keeps its text, and its size comes from it.
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="10"><rect width="40" height="10" fill="red"/></svg>"#.to_vec();
        let f = prepare(svg).unwrap();
        assert_eq!((f.mime.as_str(), f.width, f.height), ("image/svg+xml", 40.0, 10.0));
        let (w, h, px) = export::svg_rgba(br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="10"><rect width="40" height="10" fill="red"/></svg>"#, 80).unwrap();
        assert_eq!((w, h), (80, 20));
        assert_eq!(&px[..4], &[255, 0, 0, 255]);

        assert!(prepare(b"not an image".to_vec()).is_err());
        // An SVG that links a file on this machine draws without it.
        let linked = br#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10"><image width="10" height="10" xlink:href="/etc/hostname"/></svg>"#;
        let (_, _, px) = export::svg_rgba(linked, 10).unwrap();
        assert!(px.iter().all(|b| *b == 0), "nothing drawn from outside the file");
    }
}
