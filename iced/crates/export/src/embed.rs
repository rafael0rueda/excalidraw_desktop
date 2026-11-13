//! The drawing inside an exported image, byte for byte as Excalidraw 0.18.1
//! puts it there (`data/encode.ts`, `data/image.ts`, `encodeSvgBase64Payload`):
//!
//! - the `.excalidraw` JSON, zlib-deflated (pako's `deflate`), each byte made
//!   one character (a "byte string");
//! - wrapped as `{"version":"1","encoding":"bstring","compressed":true,"encoded":…}`;
//! - in a PNG, a `tEXt` chunk keyed `application/vnd.excalidraw+json`
//!   before `IEND`, Latin-1 as tEXt is;
//! - in an SVG, base64 of the same Latin-1 bytes between
//!   `<!-- payload-start -->` and `<!-- payload-end -->` (payload version 2).

use base64::Engine;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::io::{Read, Write};

pub const MIME: &str = "application/vnd.excalidraw+json";

/// `JSON.stringify(encode({ text, compress: true }))`, as Latin-1 bytes.
fn encoded(payload: &str) -> Vec<u8> {
    let mut z = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(payload.as_bytes()).expect("writing to memory");
    let deflated = z.finish().expect("writing to memory");
    let bstring: String = deflated.iter().map(|&b| char::from(b)).collect();
    let json = serde_json::json!({
        "version": "1",
        "encoding": "bstring",
        "compressed": true,
        "encoded": bstring,
    })
    .to_string();
    // Every character is below U+0100: the byte string's, and ASCII.
    json.chars().map(|c| c as u32 as u8).collect()
}

pub(crate) fn svg_payload(payload: &str) -> String {
    format!(
        "<!-- payload-type:{MIME} --><!-- payload-version:2 --><!-- payload-start -->{}<!-- payload-end -->",
        base64::engine::general_purpose::STANDARD.encode(encoded(payload))
    )
}

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut crc = flate2::Crc::new();
    crc.update(kind);
    crc.update(data);
    let mut out = Vec::with_capacity(data.len() + 12);
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(kind);
    out.extend(data);
    out.extend(crc.sum().to_be_bytes());
    out
}

pub(crate) fn png_with_payload(png: &[u8], payload: &str) -> Vec<u8> {
    let mut data = MIME.as_bytes().to_vec();
    data.push(0);
    data.extend(encoded(payload));
    // IEND is the last twelve bytes of a PNG.
    let at = png.len().saturating_sub(12);
    let mut out = png[..at].to_vec();
    out.extend(chunk(b"tEXt", &data));
    out.extend(&png[at..]);
    out
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// `decode`: the JSON back from the wrapper.
fn decode(text: String) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    if v.get("type").and_then(|t| t.as_str()) == Some("excalidraw") {
        return Some(text);
    }
    let bytes: Vec<u8> = v.get("encoded")?.as_str()?.chars().map(|c| c as u32 as u8).collect();
    if v.get("compressed")?.as_bool()? {
        let mut s = String::new();
        ZlibDecoder::new(&bytes[..]).read_to_string(&mut s).ok()?;
        Some(s)
    } else {
        String::from_utf8(bytes).ok()
    }
}

/// The drawing inside a PNG Excalidraw (or this app) exported, if any.
pub fn decode_png(png: &[u8]) -> Option<String> {
    let mut at = 8;
    while at + 12 <= png.len() {
        let len = u32::from_be_bytes(png[at..at + 4].try_into().ok()?) as usize;
        let kind = &png[at + 4..at + 8];
        let data = png.get(at + 8..at + 8 + len)?;
        if kind == b"tEXt" {
            let nul = data.iter().position(|&b| b == 0)?;
            if &data[..nul] == MIME.as_bytes() {
                return decode(latin1(&data[nul + 1..]));
            }
        }
        at += 12 + len;
    }
    None
}

/// The drawing inside an SVG Excalidraw (or this app) exported, if any.
pub fn decode_svg(svg: &str) -> Option<String> {
    if !svg.contains(&format!("payload-type:{MIME}")) {
        return None;
    }
    let start = svg.find("<!-- payload-start -->")? + "<!-- payload-start -->".len();
    let end = start + svg[start..].find("<!-- payload-end -->")?;
    let raw = base64::engine::general_purpose::STANDARD.decode(svg[start..end].trim()).ok()?;
    // Version 1 base64'd the UTF-8 text rather than a byte string.
    if svg.contains("<!-- payload-version:2 -->") {
        decode(latin1(&raw))
    } else {
        decode(String::from_utf8(raw).ok()?)
    }
}
