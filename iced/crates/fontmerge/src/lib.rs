//! Excalidraw ships each of its fonts as several files, one per Unicode
//! range, which a browser stitches together through `unicode-range`. iced's
//! text engine (cosmic-text) picks one file per family name and never looks
//! at the others, so text in Excalifont came out in a system font wherever a
//! character lived in another file. This puts the files back into one font.
//!
//! The first font is the primary: its glyphs keep their numbers, so its
//! layout tables (GSUB, GPOS, GDEF: kerning, ligatures) stay valid and are
//! copied unchanged. The other fonts' glyphs are appended, composite glyphs
//! re-pointed at their components' new numbers, and one character map covers
//! everything. Glyphs from the other files lose their kerning; outlines,
//! advances and metrics are kept exactly.
//!
//! Only what the subsets use is supported: TrueType outlines (`glyf`), one
//! units-per-em. Anything else is refused rather than merged wrongly.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use ttf_parser::{RawFace, Tag};

#[derive(Debug, PartialEq)]
pub enum Error {
    Parse(usize),
    Missing(usize, &'static str),
    NotTrueType(usize),
    UnitsPerEm(usize),
    TooManyGlyphs,
}

fn be16(d: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([d[at], d[at + 1]])
}

fn be32(d: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

struct Source<'a> {
    raw: RawFace<'a>,
    face: ttf_parser::Face<'a>,
    num_glyphs: u16,
    glyphs: Vec<&'a [u8]>,
    /// (advance, left side bearing) per glyph.
    metrics: Vec<(u16, i16)>,
}

impl<'a> Source<'a> {
    fn parse(i: usize, data: &'a [u8]) -> Result<Self, Error> {
        let raw = RawFace::parse(data, 0).map_err(|_| Error::Parse(i))?;
        let face = ttf_parser::Face::parse(data, 0).map_err(|_| Error::Parse(i))?;
        let table = |tag: &'static str| {
            raw.table(Tag::from_bytes(tag.as_bytes().try_into().unwrap()))
                .ok_or(Error::Missing(i, tag))
        };
        if raw.table(Tag::from_bytes(b"CFF ")).is_some() {
            return Err(Error::NotTrueType(i));
        }
        let head = table("head")?;
        let maxp = table("maxp")?;
        let hhea = table("hhea")?;
        let hmtx = table("hmtx")?;
        let loca = table("loca")?;
        let glyf = table("glyf")?;
        let num_glyphs = be16(maxp, 4);
        let long = be16(head, 50) == 1;
        let offset = |g: usize| {
            if long {
                be32(loca, g * 4) as usize
            } else {
                be16(loca, g * 2) as usize * 2
            }
        };
        let glyphs = (0..usize::from(num_glyphs))
            .map(|g| glyf.get(offset(g)..offset(g + 1)).unwrap_or(&[]))
            .collect();
        let n_metrics = usize::from(be16(hhea, 34)).max(1);
        let metrics = (0..usize::from(num_glyphs))
            .map(|g| {
                if g < n_metrics {
                    (be16(hmtx, g * 4), be16(hmtx, g * 4 + 2) as i16)
                } else {
                    let advance = be16(hmtx, (n_metrics - 1) * 4);
                    let at = n_metrics * 4 + (g - n_metrics) * 2;
                    (
                        advance,
                        hmtx.get(at..at + 2).map_or(0, |_| be16(hmtx, at) as i16),
                    )
                }
            })
            .collect();
        Ok(Self {
            raw,
            face,
            num_glyphs,
            glyphs,
            metrics,
        })
    }

    fn table(&self, tag: &[u8; 4]) -> Option<&'a [u8]> {
        self.raw.table(Tag::from_bytes(tag))
    }
}

/// A composite glyph with its components' glyph numbers passed through `map`.
fn remap_composite(data: &[u8], map: impl Fn(u16) -> u16) -> Vec<u8> {
    let mut out = data.to_vec();
    if data.len() < 10 || (be16(data, 0) as i16) >= 0 {
        return out;
    }
    let mut at = 10;
    loop {
        if at + 4 > data.len() {
            break;
        }
        let flags = be16(data, at);
        let glyph = map(be16(data, at + 2));
        out[at + 2..at + 4].copy_from_slice(&glyph.to_be_bytes());
        at += 4;
        at += if flags & 0x0001 != 0 { 4 } else { 2 };
        if flags & 0x0008 != 0 {
            at += 2;
        } else if flags & 0x0040 != 0 {
            at += 4;
        } else if flags & 0x0080 != 0 {
            at += 8;
        }
        if flags & 0x0020 == 0 {
            break;
        }
    }
    out
}

/// A format 12 character map for both Unicode platforms.
fn cmap(map: &BTreeMap<u32, u16>) -> Vec<u8> {
    let mut groups: Vec<(u32, u32, u32)> = Vec::new();
    for (&cp, &g) in map {
        match groups.last_mut() {
            Some((start, end, first))
                if cp == *end + 1 && u32::from(g) == *first + (cp - *start) =>
            {
                *end = cp
            }
            _ => groups.push((cp, cp, u32::from(g))),
        }
    }
    let sub_len = 16 + 12 * groups.len() as u32;
    let mut out = Vec::new();
    out.extend(0u16.to_be_bytes()); // version
    out.extend(2u16.to_be_bytes()); // tables
    let offset = 4 + 8 * 2u32;
    for (platform, encoding) in [(0u16, 4u16), (3, 10)] {
        out.extend(platform.to_be_bytes());
        out.extend(encoding.to_be_bytes());
        out.extend(offset.to_be_bytes());
    }
    out.extend(12u16.to_be_bytes());
    out.extend(0u16.to_be_bytes());
    out.extend(sub_len.to_be_bytes());
    out.extend(0u32.to_be_bytes()); // language
    out.extend((groups.len() as u32).to_be_bytes());
    for (start, end, glyph) in groups {
        out.extend(start.to_be_bytes());
        out.extend(end.to_be_bytes());
        out.extend(glyph.to_be_bytes());
    }
    out
}

fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, c| {
        let mut w = [0u8; 4];
        w[..c.len()].copy_from_slice(c);
        sum.wrapping_add(u32::from_be_bytes(w))
    })
}

/// An sfnt file from its tables.
fn assemble(mut tables: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
    tables.sort_by_key(|t| t.0);
    let n = tables.len() as u16;
    let pow = 1u16 << (15 - n.leading_zeros());
    let mut out = Vec::new();
    out.extend(0x0001_0000u32.to_be_bytes());
    out.extend(n.to_be_bytes());
    out.extend((pow * 16).to_be_bytes());
    out.extend((15 - n.leading_zeros() as u16).to_be_bytes());
    out.extend((n * 16 - pow * 16).to_be_bytes());
    let mut offset = 12 + 16 * tables.len();
    let mut body = Vec::new();
    let mut head_at = None;
    for (tag, data) in &tables {
        out.extend(tag);
        out.extend(checksum(data).to_be_bytes());
        out.extend((offset as u32).to_be_bytes());
        out.extend((data.len() as u32).to_be_bytes());
        if tag == b"head" {
            head_at = Some(offset);
        }
        body.extend(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset = 12 + 16 * tables.len() + body.len();
    }
    out.extend(body);
    if let Some(at) = head_at {
        let adjust = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out));
        out[at + 8..at + 12].copy_from_slice(&adjust.to_be_bytes());
    }
    out
}

/// Merges `fonts` into one; the first is the primary (see the module docs).
pub fn merge(fonts: &[&[u8]]) -> Result<Vec<u8>, Error> {
    let sources: Vec<Source> = fonts
        .iter()
        .enumerate()
        .map(|(i, d)| Source::parse(i, d))
        .collect::<Result<_, _>>()?;
    let Some(primary) = sources.first() else {
        return Err(Error::Parse(0));
    };
    let upem = primary.face.units_per_em();
    if let Some(i) = sources.iter().position(|s| s.face.units_per_em() != upem) {
        return Err(Error::UnitsPerEm(i));
    }

    // Glyph numbers: the primary's unchanged, the others' after it, each
    // without its own .notdef.
    let mut maps: Vec<Vec<u16>> = Vec::new();
    let mut next: u32 = 0;
    for (i, s) in sources.iter().enumerate() {
        let mut map = vec![0u16; usize::from(s.num_glyphs)];
        for (g, slot) in map.iter_mut().enumerate() {
            if i > 0 && g == 0 {
                continue;
            }
            *slot = u16::try_from(next).map_err(|_| Error::TooManyGlyphs)?;
            next += 1;
        }
        maps.push(map);
    }
    let total = u16::try_from(next).map_err(|_| Error::TooManyGlyphs)?;

    let mut glyf = Vec::new();
    let mut loca: Vec<u32> = Vec::new();
    let mut hmtx = Vec::new();
    let mut advance_max = 0u16;
    for (i, s) in sources.iter().enumerate() {
        for g in 0..usize::from(s.num_glyphs) {
            if i > 0 && g == 0 {
                continue;
            }
            loca.push(glyf.len() as u32);
            let map = &maps[i];
            glyf.extend(remap_composite(s.glyphs[g], |c| {
                map.get(usize::from(c)).copied().unwrap_or(0)
            }));
            while glyf.len() % 4 != 0 {
                glyf.push(0);
            }
            let (advance, lsb) = s.metrics[g];
            advance_max = advance_max.max(advance);
            hmtx.extend(advance.to_be_bytes());
            hmtx.extend(lsb.to_be_bytes());
        }
    }
    loca.push(glyf.len() as u32);

    let mut chars = BTreeMap::new();
    for (i, s) in sources.iter().enumerate() {
        let Some(cmap) = s.face.tables().cmap else {
            continue;
        };
        for sub in cmap.subtables.into_iter().filter(|t| t.is_unicode()) {
            sub.codepoints(|cp| {
                if let Some(g) = sub.glyph_index(cp) {
                    // The primary wins where files overlap (the space, mostly).
                    chars.entry(cp).or_insert(maps[i][usize::from(g.0)]);
                }
            });
        }
    }

    let primary_table = |tag: &[u8; 4]| primary.table(tag).map(<[u8]>::to_vec);
    let mut head = primary_table(b"head").ok_or(Error::Missing(0, "head"))?;
    head[50..52].copy_from_slice(&1i16.to_be_bytes());
    head[8..12].copy_from_slice(&[0; 4]);
    let mut hhea = primary_table(b"hhea").ok_or(Error::Missing(0, "hhea"))?;
    hhea[10..12].copy_from_slice(&advance_max.to_be_bytes());
    hhea[34..36].copy_from_slice(&total.to_be_bytes());
    let mut maxp = primary_table(b"maxp").ok_or(Error::Missing(0, "maxp"))?;
    maxp[4..6].copy_from_slice(&total.to_be_bytes());
    // Version 1.0 limits: the largest any file needs.
    if maxp.len() >= 32 {
        for at in (6..32).step_by(2) {
            let m = sources
                .iter()
                .filter_map(|s| s.table(b"maxp"))
                .filter(|t| t.len() >= 32)
                .map(|t| be16(t, at))
                .max()
                .unwrap_or(0);
            maxp[at..at + 2].copy_from_slice(&m.to_be_bytes());
        }
    }
    // post version 3: no glyph names, which would not survive renumbering.
    let mut post = primary_table(b"post").ok_or(Error::Missing(0, "post"))?;
    post.truncate(32);
    post.resize(32, 0);
    post[0..4].copy_from_slice(&0x0003_0000u32.to_be_bytes());

    let mut tables = vec![
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"maxp", maxp),
        (*b"post", post),
        (*b"glyf", glyf),
        (
            *b"loca",
            loca.iter().flat_map(|o| o.to_be_bytes()).collect(),
        ),
        (*b"hmtx", hmtx),
        (*b"cmap", cmap(&chars)),
    ];
    // Copied from the primary, whose glyph numbers did not change.
    for tag in [
        b"OS/2", b"name", b"gasp", b"GDEF", b"GSUB", b"GPOS", b"cvt ", b"fpgm", b"prep", b"STAT",
    ] {
        if let Some(t) = primary.table(tag) {
            tables.push((*tag, t.to_vec()));
        }
    }
    Ok(assemble(tables))
}

/// A `name` table without the licence's full text (ID 13), which is most
/// of a small subset's size; the copyright and the licence's URL stay.
fn short_names(table: &[u8]) -> Vec<u8> {
    if table.len() < 6 || be16(table, 0) != 0 {
        return table.to_vec();
    }
    let count = usize::from(be16(table, 2));
    let strings = usize::from(be16(table, 4));
    let mut records = Vec::new();
    let mut data: Vec<u8> = Vec::new();
    for i in 0..count {
        let at = 6 + i * 12;
        let Some(r) = table.get(at..at + 12) else { break };
        if be16(r, 6) == 13 {
            continue;
        }
        let (len, off) = (usize::from(be16(r, 8)), usize::from(be16(r, 10)));
        let Some(text) = table.get(strings + off..strings + off + len) else { continue };
        let mut rec = r.to_vec();
        rec[10..12].copy_from_slice(&(data.len() as u16).to_be_bytes());
        records.push(rec);
        data.extend(text);
    }
    let mut out = Vec::new();
    out.extend(0u16.to_be_bytes());
    out.extend((records.len() as u16).to_be_bytes());
    out.extend((6 + 12 * records.len() as u16).to_be_bytes());
    for r in records {
        out.extend(r);
    }
    out.extend(data);
    out
}

/// A font with only the glyphs `chars` need (and `.notdef`), as Excalidraw
/// embeds a subset of each font in the SVGs it exports. Glyphs are
/// renumbered, so the layout tables (kerning, ligatures), which name glyphs
/// by number, are left out; outlines, advances and metrics are kept.
pub fn subset(font: &[u8], chars: impl IntoIterator<Item = char>) -> Result<Vec<u8>, Error> {
    subset_named(font, chars, None)
}

/// A `name` table naming the font `family`, Windows Unicode English only.
fn names_for(family: &str) -> Vec<u8> {
    let postscript: String = family.chars().filter(char::is_ascii_alphanumeric).collect();
    let records = [(1u16, family.to_owned()), (2, "Regular".to_owned()), (4, family.to_owned()), (6, postscript)];
    let mut out = Vec::new();
    out.extend(0u16.to_be_bytes());
    out.extend((records.len() as u16).to_be_bytes());
    out.extend((6 + 12 * records.len() as u16).to_be_bytes());
    let mut data: Vec<u8> = Vec::new();
    for (id, text) in &records {
        let bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        for v in [3u16, 1, 0x409, *id, bytes.len() as u16, data.len() as u16] {
            out.extend(v.to_be_bytes());
        }
        data.extend(bytes);
    }
    out.extend(data);
    out
}

/// `subset`, renamed to `family` if given: a font that can only ever be
/// asked for by that name.
pub fn subset_named(font: &[u8], chars: impl IntoIterator<Item = char>, family: Option<&str>) -> Result<Vec<u8>, Error> {
    let s = Source::parse(0, font)?;
    let mut chars_kept = BTreeMap::new();
    let mut keep = vec![0u16];
    let mut wanted: Vec<u16> = Vec::new();
    for c in chars {
        if let Some(g) = s.face.glyph_index(c) {
            chars_kept.insert(u32::from(c), g.0);
            wanted.push(g.0);
        }
    }
    // Composite glyphs bring their components.
    while let Some(g) = wanted.pop() {
        if keep.contains(&g) || g >= s.num_glyphs {
            continue;
        }
        keep.push(g);
        let found = std::cell::RefCell::new(Vec::new());
        remap_composite(s.glyphs[usize::from(g)], |c| {
            found.borrow_mut().push(c);
            c
        });
        wanted.extend(found.into_inner());
    }
    keep.sort_unstable();
    let new_of = |g: u16| keep.binary_search(&g).map_or(0, |i| i as u16);

    let mut glyf = Vec::new();
    let mut loca: Vec<u32> = Vec::new();
    let mut hmtx = Vec::new();
    let mut advance_max = 0u16;
    for &g in &keep {
        loca.push(glyf.len() as u32);
        glyf.extend(remap_composite(s.glyphs[usize::from(g)], new_of));
        while glyf.len() % 4 != 0 {
            glyf.push(0);
        }
        let (advance, lsb) = s.metrics[usize::from(g)];
        advance_max = advance_max.max(advance);
        hmtx.extend(advance.to_be_bytes());
        hmtx.extend(lsb.to_be_bytes());
    }
    loca.push(glyf.len() as u32);
    let total = keep.len() as u16;
    let cmap_of: BTreeMap<u32, u16> = chars_kept.into_iter().map(|(c, g)| (c, new_of(g))).collect();

    let table = |tag: &[u8; 4]| s.table(tag).map(<[u8]>::to_vec);
    let mut head = table(b"head").ok_or(Error::Missing(0, "head"))?;
    head[50..52].copy_from_slice(&1i16.to_be_bytes());
    head[8..12].copy_from_slice(&[0; 4]);
    let mut hhea = table(b"hhea").ok_or(Error::Missing(0, "hhea"))?;
    hhea[10..12].copy_from_slice(&advance_max.to_be_bytes());
    hhea[34..36].copy_from_slice(&total.to_be_bytes());
    let mut maxp = table(b"maxp").ok_or(Error::Missing(0, "maxp"))?;
    maxp[4..6].copy_from_slice(&total.to_be_bytes());
    let mut post = table(b"post").ok_or(Error::Missing(0, "post"))?;
    post.truncate(32);
    post.resize(32, 0);
    post[0..4].copy_from_slice(&0x0003_0000u32.to_be_bytes());
    let mut tables = vec![
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"maxp", maxp),
        (*b"post", post),
        (*b"glyf", glyf),
        (*b"loca", loca.iter().flat_map(|o| o.to_be_bytes()).collect()),
        (*b"hmtx", hmtx),
        (*b"cmap", cmap(&cmap_of)),
    ];
    match (family, s.table(b"name")) {
        (Some(family), _) => tables.push((*b"name", names_for(family))),
        (None, Some(t)) => tables.push((*b"name", short_names(t))),
        (None, None) => {}
    }
    for tag in [b"OS/2", b"gasp", b"cvt ", b"fpgm", b"prep"] {
        if let Some(t) = s.table(tag) {
            tables.push((*tag, t.to_vec()));
        }
    }
    Ok(assemble(tables))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subset_keeps_what_it_was_asked_for() {
        let Some(fonts) = excalifont() else {
            eprintln!("Excalidraw fonts not installed; skipped");
            return;
        };
        let full = ttf_parser::Face::parse(&fonts[0], 0).unwrap();
        let small = subset(&fonts[0], "Hello, wörld".chars()).unwrap();
        assert!(small.len() * 5 < fonts[0].len(), "{} of {}", small.len(), fonts[0].len());
        let out = ttf_parser::Face::parse(&small, 0).expect("the subset parses");
        assert_eq!(out.units_per_em(), full.units_per_em());
        assert_eq!(out.number_of_glyphs() as usize, "Helo, wörd".chars().collect::<std::collections::BTreeSet<_>>().len() + 1);
        for c in "Hello, wörld".chars() {
            let (a, b) = (full.glyph_index(c).unwrap(), out.glyph_index(c).expect("kept"));
            assert_eq!(full.glyph_hor_advance(a), out.glyph_hor_advance(b), "{c}");
            assert_eq!(full.glyph_bounding_box(a), out.glyph_bounding_box(b), "{c}");
        }
        assert!(out.glyph_index('z').is_none());
    }

    /// Excalifont's subsets from the Excalidraw package, decoded; skipped
    /// when `node_modules` is not there.
    fn excalifont() -> Option<Vec<Vec<u8>>> {
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../node_modules/@excalidraw/excalidraw/dist/prod/fonts/Excalifont"
        );
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .collect();
        files.sort();
        let mut fonts: Vec<Vec<u8>> = files
            .iter()
            .filter_map(|f| wuff::decompress_woff2(&std::fs::read(f).ok()?).ok())
            .collect();
        // The Basic Latin file first, as build.rs orders them.
        let latin = fonts.iter().position(|f| {
            ttf_parser::Face::parse(f, 0).is_ok_and(|face| face.glyph_index('a').is_some())
        })?;
        fonts.swap(0, latin);
        Some(fonts)
    }

    #[test]
    fn every_character_of_every_subset_keeps_its_glyph() {
        let Some(fonts) = excalifont() else {
            eprintln!("Excalidraw fonts not installed; skipped");
            return;
        };
        let slices: Vec<&[u8]> = fonts.iter().map(Vec::as_slice).collect();
        let merged = merge(&slices).unwrap();
        let out = ttf_parser::Face::parse(&merged, 0).expect("the merged font parses");
        assert_eq!(out.units_per_em(), 1000);
        let mut checked = 0;
        for (i, data) in fonts.iter().enumerate() {
            let face = ttf_parser::Face::parse(data, 0).unwrap();
            for sub in face
                .tables()
                .cmap
                .unwrap()
                .subtables
                .into_iter()
                .filter(|t| t.is_unicode())
            {
                sub.codepoints(|cp| {
                    let Some(c) = char::from_u32(cp) else { return };
                    let (Some(g0), Some(g1)) = (face.glyph_index(c), out.glyph_index(c)) else {
                        panic!("U+{cp:04X} lost");
                    };
                    if i == 0 {
                        assert_eq!(g0, g1, "the primary's glyph numbers are kept");
                    }
                    assert_eq!(
                        face.glyph_hor_advance(g0),
                        out.glyph_hor_advance(g1),
                        "advance of U+{cp:04X}"
                    );
                    struct Sink(Vec<(f32, f32)>);
                    impl ttf_parser::OutlineBuilder for Sink {
                        fn move_to(&mut self, x: f32, y: f32) {
                            self.0.push((x, y));
                        }
                        fn line_to(&mut self, x: f32, y: f32) {
                            self.0.push((x, y));
                        }
                        fn quad_to(&mut self, _: f32, _: f32, x: f32, y: f32) {
                            self.0.push((x, y));
                        }
                        fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, x: f32, y: f32) {
                            self.0.push((x, y));
                        }
                        fn close(&mut self) {}
                    }
                    let (mut a, mut b) = (Sink(Vec::new()), Sink(Vec::new()));
                    face.outline_glyph(g0, &mut a);
                    out.outline_glyph(g1, &mut b);
                    assert_eq!(a.0, b.0, "outline of U+{cp:04X}");
                    checked += 1;
                });
            }
        }
        assert!(checked > 500, "{checked} characters");
        for c in ['a', 'é', 'Ж', 'λ', '€', 'ő'] {
            assert!(out.glyph_index(c).is_some(), "{c}");
        }
    }

    #[test]
    fn what_is_not_a_font_is_refused() {
        assert_eq!(merge(&[b"not a font"]).unwrap_err(), Error::Parse(0));
    }
}
