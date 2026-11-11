//! Excalidraw's fonts, decoded from the WOFF2 files the Excalidraw package
//! ships, into TrueType for iced to load.
//!
//! Taken from `node_modules` at build time, as the Tauri app takes them, so
//! that no font binaries are committed. Without `node_modules` (CI's Rust
//! job) the app still builds, and text falls back to system fonts.
//!
//! Xiaolai, the CJK fallback, is left out: 209 files, 12 MB. CJK text uses
//! whatever the system has.
//!
//! Excalidraw splits Excalifont, Nunito, Lilita One and Comic Shanns into
//! one file per Unicode range. Loaded side by side under one family name,
//! cosmic-text matches one of them and, for a glyph it lacks, goes to system
//! fonts rather than to the family's other files: Latin text came out in a
//! system sans. Until the files are merged into one font (phase 3, text),
//! only the file covering Basic Latin is bundled; other scripts fall back
//! to system fonts.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const FAMILIES: &[&str] = &[
    "Excalifont",
    "Virgil",
    "Nunito",
    "Lilita",
    "ComicShanns",
    "Cascadia",
    "Liberation",
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

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let src = manifest.join("../../../node_modules/@excalidraw/excalidraw/dist/prod/fonts");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let fonts = out.join("fonts");
    std::fs::create_dir_all(&fonts).unwrap();
    println!("cargo:rerun-if-changed={}", src.display());

    let mut table = String::from("/// (family name, TrueType bytes), decoded by build.rs.\npub const FONTS: &[(&str, &[u8])] = &[\n");
    let mut count = 0;
    let mut names = String::from("/// (directory, family name, weight) of each bundled family.\npub const FAMILY_NAMES: &[(&str, &str, u16)] = &[\n");
    for family in FAMILIES {
        let dir = src.join(family);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "woff2"))
            .collect();
        files.sort();
        for file in files {
            let Ok(ttf) = wuff::decompress_woff2(&std::fs::read(&file).unwrap()) else {
                println!("cargo:warning=could not decode {}", file.display());
                continue;
            };
            if !covers_latin(&ttf) {
                continue;
            }
            let name = family_name(&ttf).unwrap_or_else(|| (*family).to_owned());
            let target = fonts.join(Path::new(file.file_stem().unwrap()).with_extension("ttf"));
            std::fs::write(&target, &ttf).unwrap();
            writeln!(
                table,
                "    ({name:?}, include_bytes!({:?})),",
                target.display().to_string()
            )
            .unwrap();
            if !names.contains(&format!("({family:?},")) {
                writeln!(names, "    ({family:?}, {name:?}, {}),", weight(&ttf)).unwrap();
            }
            count += 1;
        }
    }
    table.push_str("];\n");
    names.push_str("];\n");
    table.push_str(&names);
    if count == 0 {
        println!("cargo:warning=Excalidraw fonts not found at {} (run npm install); text will use system fonts", src.display());
    }
    std::fs::write(out.join("fonts.rs"), table).unwrap();
}
