//! Excalidraw's fonts, decoded from the WOFF2 files the Excalidraw package
//! ships, and merged into one TrueType font per family for iced to load.
//!
//! Taken from `node_modules` at build time, as the Tauri app takes them, so
//! that no font binaries are committed. Without `node_modules` (CI's Rust
//! job) the app still builds, and text falls back to system fonts.
//!
//! Excalidraw splits most fonts into one file per Unicode range, which a
//! browser stitches together; cosmic-text uses one file per family and never
//! looks at the others. So each family's files are merged (`fontmerge`),
//! the Basic Latin file first so its kerning stays valid. Xiaolai, which
//! Excalidraw falls back to for CJK in Excalifont only, is merged into
//! Excalifont, which is what that fallback amounts to.

use std::fmt::Write as _;
use std::path::PathBuf;

/// (directory, extra directories merged into it).
const FAMILIES: &[(&str, &[&str])] = &[
    ("Excalifont", &["Xiaolai"]),
    ("Virgil", &[]),
    ("Nunito", &[]),
    ("Lilita", &[]),
    ("ComicShanns", &[]),
    ("Cascadia", &[]),
    ("Liberation", &[]),
];

/// The OS/2 weight class, which font matching compares with the one asked for.
fn weight(ttf: &[u8]) -> u16 {
    ttf_parser::Face::parse(ttf, 0).map_or(400, |f| f.weight().to_number())
}

/// Whether this file covers Basic Latin: letters and digits.
fn covers_latin(ttf: &[u8]) -> bool {
    let Ok(face) = ttf_parser::Face::parse(ttf, 0) else {
        return false;
    };
    "azAZ09".chars().all(|c| face.glyph_index(c).is_some())
}

/// The family name font matching uses: the typographic family (name ID 16)
/// when there is one, as fontdb does, else the legacy family (ID 1). They
/// differ for Nunito, whose ID 1 is "Nunito ExtraLight Medium".
fn family_name(ttf: &[u8]) -> Option<String> {
    let face = ttf_parser::Face::parse(ttf, 0).ok()?;
    let find = |id| {
        face.names()
            .into_iter()
            .find(|n| n.name_id == id && n.is_unicode())
            .and_then(|n| n.to_string())
    };
    find(ttf_parser::name_id::TYPOGRAPHIC_FAMILY).or_else(|| find(ttf_parser::name_id::FAMILY))
}

/// Every WOFF2 file in `dir`, decoded, in name order.
fn decoded(dir: &std::path::Path) -> Vec<Vec<u8>> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "woff2"))
        .collect();
    files.sort();
    files
        .iter()
        .filter_map(|f| match wuff::decompress_woff2(&std::fs::read(f).ok()?) {
            Ok(ttf) => Some(ttf),
            Err(_) => {
                println!("cargo:warning=could not decode {}", f.display());
                None
            }
        })
        .collect()
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let src = manifest.join("../../../node_modules/@excalidraw/excalidraw/dist/prod/fonts");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let fonts = out.join("fonts");
    std::fs::create_dir_all(&fonts).unwrap();
    println!("cargo:rerun-if-changed={}", src.display());

    let mut table = String::from("/// (family name, TrueType bytes), merged by build.rs.\npub const FONTS: &[(&str, &[u8])] = &[\n");
    let mut names = String::from("/// (directory, family name, weight) of each bundled family.\npub const FAMILY_NAMES: &[(&str, &str, u16)] = &[\n");
    for (family, extra) in FAMILIES {
        let mut files = decoded(&src.join(family));
        let Some(latin) = files.iter().position(|f| covers_latin(f)) else {
            continue;
        };
        files.swap(0, latin);
        for dir in *extra {
            files.extend(decoded(&src.join(dir)));
        }
        let slices: Vec<&[u8]> = files.iter().map(Vec::as_slice).collect();
        let merged = match fontmerge::merge(&slices) {
            Ok(m) => m,
            Err(e) => {
                // Better one file than none: the Latin one alone.
                println!("cargo:warning=could not merge {family}: {e:?}; bundling its Basic Latin file only");
                files[0].clone()
            }
        };
        let name = family_name(&merged).unwrap_or_else(|| (*family).to_owned());
        let target = fonts.join(format!("{family}.ttf"));
        std::fs::write(&target, &merged).unwrap();
        writeln!(
            table,
            "    ({name:?}, include_bytes!({:?})),",
            target.display().to_string()
        )
        .unwrap();
        writeln!(names, "    ({family:?}, {name:?}, {}),", weight(&merged)).unwrap();
    }
    table.push_str("];\n");
    names.push_str("];\n");
    if !table.contains("include_bytes") {
        println!("cargo:warning=Excalidraw fonts not found at {} (run npm install); text will use system fonts", src.display());
    }
    table.push_str(&names);
    std::fs::write(out.join("fonts.rs"), table).unwrap();
}
