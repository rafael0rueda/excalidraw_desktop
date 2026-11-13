use super::*;

const FIXTURE: &str = include_str!("../../scene/tests/fixtures/every-element.excalidraw");

fn scene(text: &str) -> Scene {
    Scene::parse(text).unwrap()
}

/// Virgil from the Excalidraw package, for every family; None without
/// `node_modules`.
fn virgil() -> Option<Font> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../node_modules/@excalidraw/excalidraw/dist/prod/fonts/Virgil/Virgil-Regular.woff2");
    let ttf = wuff::decompress_woff2(&std::fs::read(path).ok()?).ok()?;
    Some(Font { family: "Virgil", data: Box::leak(ttf.into_boxed_slice()) })
}

fn no_fonts(_: f64) -> Option<Font> {
    None
}

fn export<'a>(scene: &'a Scene, font: &'a dyn Fn(f64) -> Option<Font>) -> Export<'a> {
    Export { scene, only: None, canvas: "#ffffff", transparent: false, font }
}

const RECT: &str = r##"{"type":"excalidraw","elements":[{"type":"rectangle","id":"r","x":100,"y":50,"width":200,"height":100,"strokeColor":"#e03131","backgroundColor":"transparent","strokeWidth":4,"roughness":0,"seed":1}],"appState":{"viewBackgroundColor":"#ffffff"}}"##;

#[test]
fn the_svg_is_sized_and_marked_as_excalidraw_sizes_and_marks_it() {
    let s = scene(RECT);
    let svg = export(&s, &no_fonts).svg(None).unwrap();
    assert!(svg.starts_with(r#"<svg version="1.1" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 220 120" width="220" height="120">"#), "{svg}");
    assert!(svg.contains("<!-- svg-source:excalidraw -->"));
    assert!(svg.contains(r##"<rect x="0" y="0" width="220" height="120" fill="#ffffff"/>"##));
    assert!(svg.contains("translate(10 10)"), "the element sits PADDING in");
    assert!(!svg.contains("payload-start"));
    resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).expect("well-formed");
}

#[test]
fn every_kind_of_element_exports() {
    let s = scene(FIXTURE);
    let font = |_: f64| virgil();
    for transparent in [false, true] {
        let x = Export { transparent, ..export(&s, &font) };
        let svg = x.svg(None).unwrap();
        resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).expect("well-formed");
        assert!(svg.contains("<image href=\"data:image/"), "the image is inline");
        assert_eq!(svg.contains("fill=\"#ffffff\"/>"), !transparent);
        let png = x.png(1.0, None).unwrap();
        assert_eq!(&png[1..4], b"PNG");
    }
}

#[test]
fn fonts_are_inlined_as_subsets() {
    let Some(_) = virgil() else {
        eprintln!("Excalidraw fonts not installed; skipped");
        return;
    };
    let s = scene(r##"{"type":"excalidraw","elements":[{"type":"text","id":"t","x":0,"y":0,"width":50,"height":25,"text":"Hi","fontSize":20,"fontFamily":1,"seed":1}]}"##);
    let font = |_: f64| virgil();
    let svg = export(&s, &font).svg(None).unwrap();
    assert!(svg.contains(r#"font-family="Virgil, Segoe UI Emoji""#), "{svg}");
    let at = svg.find("@font-face { font-family: Virgil; src: url(data:font/ttf;base64,").expect("inline font");
    let data = &svg[at..];
    let b64 = &data[data.find("base64,").unwrap() + 7..data.find(')').unwrap()];
    let ttf = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
    let face = ttf_parser::Face::parse(&ttf, 0).unwrap();
    assert!(face.glyph_index('H').is_some() && face.glyph_index('i').is_some() && face.glyph_index('x').is_none());
    assert!(ttf.len() < 8_000, "{} bytes", ttf.len());
}

#[test]
fn the_drawing_rides_inside_both_formats() {
    let s = scene(FIXTURE);
    let payload = payload(&s, None);
    let x = export(&s, &no_fonts);
    let svg = x.svg(Some(&payload)).unwrap();
    assert!(svg.contains("<metadata><!-- payload-type:application/vnd.excalidraw+json --><!-- payload-version:2 --><!-- payload-start -->"));
    assert_eq!(decode_svg(&svg).as_deref(), Some(payload.as_str()));
    let png = x.png(1.0, Some(&payload)).unwrap();
    assert_eq!(decode_png(&png).as_deref(), Some(payload.as_str()));
    assert_eq!(decode_png(&x.png(1.0, None).unwrap()), None);
    // Still a PNG that decodes.
    let img = image_size(&png);
    assert!(img.0 > 0 && img.1 > 0);
    // What comes back opens, without the deleted element.
    let back = Scene::parse(&payload).unwrap();
    assert_eq!(back.elements.len(), s.visible().count());
}

#[test]
fn a_selection_exports_alone_with_its_files() {
    let s = scene(FIXTURE);
    let only: HashSet<String> = ["rect".to_owned()].into();
    let p = payload(&s, Some(&only));
    let back: serde_json::Value = serde_json::from_str(&p).unwrap();
    assert_eq!(back["elements"].as_array().unwrap().len(), 1);
    assert!(back["files"].as_object().unwrap().is_empty(), "the image was not exported");
    let with_image: HashSet<String> = ["img".to_owned()].into();
    let back: serde_json::Value = serde_json::from_str(&payload(&s, Some(&with_image))).unwrap();
    assert!(back["files"].get("f1").is_some());

    let whole = export(&s, &no_fonts).svg(None).unwrap();
    let part = Export { only: Some(&only), ..export(&s, &no_fonts) }.svg(None).unwrap();
    assert!(part.len() < whole.len() / 2);
}

fn image_size(png: &[u8]) -> (u32, u32) {
    (u32::from_be_bytes(png[16..20].try_into().unwrap()), u32::from_be_bytes(png[20..24].try_into().unwrap()))
}

#[test]
fn pngs_are_scaled_and_coloured() {
    let s = scene(RECT);
    let x = Export { canvas: "#282a36", ..export(&s, &no_fonts) };
    for scale in [1.0, 2.0, 3.0] {
        let (w, h, px) = x.rgba(scale).unwrap();
        assert_eq!((w, h), ((220.0 * scale) as u32, (120.0 * scale) as u32));
        assert_eq!(&px[..4], &[0x28, 0x2a, 0x36, 255], "the canvas colour");
        // The red stroke along the top edge, PADDING in.
        let y = (10.0 * scale) as u32;
        let at = ((y * w + w / 2) * 4) as usize;
        assert!(px[at] > 180 && px[at + 1] < 120, "{:?}", &px[at..at + 4]);
        assert_eq!(image_size(&x.png(scale, None).unwrap()), (w, h));
    }
    let (_, _, px) = Export { transparent: true, ..x }.rgba(1.0).unwrap();
    assert_eq!(px[3], 0, "a transparent corner");
}

#[test]
fn what_cannot_be_made_says_so() {
    let empty = scene(r#"{"type":"excalidraw","elements":[]}"#);
    assert_eq!(export(&empty, &no_fonts).svg(None).unwrap_err(), Error::Empty);
    let huge = scene(r##"{"type":"excalidraw","elements":[{"type":"rectangle","id":"r","x":0,"y":0,"width":20000,"height":20000,"seed":1}]}"##);
    assert!(matches!(export(&huge, &no_fonts).png(1.0, None), Err(Error::TooLarge { .. })));
}

#[test]
fn text_and_links_are_escaped() {
    let s = scene(r##"{"type":"excalidraw","elements":[{"type":"text","id":"t","x":0,"y":0,"width":50,"height":25,"text":"a < b & \"c\"","fontSize":20,"fontFamily":5,"seed":1,"link":"https://example.com/?a=1&b=2"},{"type":"rectangle","id":"r","x":0,"y":40,"width":10,"height":10,"seed":1,"link":"javascript:alert(1)"}]}"##);
    let svg = export(&s, &no_fonts).svg(None).unwrap();
    assert!(svg.contains("a &lt; b &amp; &quot;c&quot;</text>"));
    assert!(svg.contains(r#"<a href="https://example.com/?a=1&amp;b=2">"#));
    assert!(!svg.contains("javascript"));
    resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).expect("well-formed");
}

#[test]
fn a_frame_clips_its_children_and_sizes_the_export() {
    let s = scene(r##"{"type":"excalidraw","elements":[
        {"type":"frame","id":"f","x":0,"y":0,"width":100,"height":100,"name":"Box","seed":1},
        {"type":"rectangle","id":"r","x":50,"y":50,"width":500,"height":500,"frameId":"f","seed":1}]}"##);
    let svg = export(&s, &no_fonts).svg(None).unwrap();
    // 100 wide, and the name's line above the frame: 14 × 1.25 + 3.
    assert!(svg.contains(r#"viewBox="0 0 120 140.5""#), "{}", &svg[..200]);
    assert!(svg.contains(r#"<clipPath id="frame-clip-"#) && svg.contains(r#"clip-path="url(#frame-clip-"#));
    assert!(svg.contains(">Box</text>"));
}

#[test]
fn an_arrow_label_is_cut_out_of_its_arrow() {
    let s = scene(r##"{"type":"excalidraw","elements":[
        {"type":"arrow","id":"a","x":0,"y":0,"width":200,"height":0,"points":[[0,0],[200,0]],"seed":1,"boundElements":[{"type":"text","id":"t"}]},
        {"type":"text","id":"t","x":80,"y":-12,"width":40,"height":25,"text":"hi","fontSize":20,"fontFamily":5,"containerId":"a","seed":1}]}"##);
    let svg = export(&s, &no_fonts).svg(None).unwrap();
    assert!(svg.contains(r#"<g mask="url(#mask-"#));
    // The label straight after its arrow.
    assert!(svg.find("</text>").unwrap() > svg.find("<g mask=").unwrap());
}
