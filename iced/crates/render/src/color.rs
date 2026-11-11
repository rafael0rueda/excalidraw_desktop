//! CSS colours, as far as Excalidraw drawings use them.

/// Straight (not premultiplied) RGBA, each 0–1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const TRANSPARENT: Rgba = Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
    pub const BLACK: Rgba = Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    fn bytes(r: u8, g: u8, b: u8, a: f32) -> Self {
        Self {
            r: f32::from(r) / 255.0,
            g: f32::from(g) / 255.0,
            b: f32::from(b) / 255.0,
            a,
        }
    }
}

/// Parses a CSS colour. Anything unrecognised is black, which is what a
/// canvas keeps when given a colour it cannot parse (its default style).
pub fn parse(css: &str) -> Rgba {
    try_parse(css.trim()).unwrap_or(Rgba::BLACK)
}

fn try_parse(s: &str) -> Option<Rgba> {
    if let Some(hex) = s.strip_prefix('#') {
        let digit = |i: usize| u8::from_str_radix(hex.get(i..=i)?, 16).ok();
        let pair = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        return match hex.len() {
            3 | 4 => {
                let (r, g, b) = (digit(0)? * 17, digit(1)? * 17, digit(2)? * 17);
                let a = if hex.len() == 4 {
                    f32::from(digit(3)? * 17) / 255.0
                } else {
                    1.0
                };
                Some(Rgba::bytes(r, g, b, a))
            }
            6 | 8 => {
                let a = if hex.len() == 8 {
                    f32::from(pair(6)?) / 255.0
                } else {
                    1.0
                };
                Some(Rgba::bytes(pair(0)?, pair(2)?, pair(4)?, a))
            }
            _ => None,
        };
    }
    let lower = s.to_ascii_lowercase();
    if let Some(args) = lower
        .strip_prefix("rgba(")
        .or_else(|| lower.strip_prefix("rgb("))
    {
        let args = args.strip_suffix(')')?;
        let parts: Vec<&str> = args
            .split([',', '/', ' '])
            .filter(|p| !p.is_empty())
            .collect();
        if parts.len() < 3 {
            return None;
        }
        let channel = |p: &str| -> Option<u8> {
            Some(match p.strip_suffix('%') {
                Some(pct) => (pct.parse::<f32>().ok()? * 2.55).round().clamp(0.0, 255.0) as u8,
                None => p.parse::<f32>().ok()?.round().clamp(0.0, 255.0) as u8,
            })
        };
        let a = match parts.get(3) {
            Some(p) => match p.strip_suffix('%') {
                Some(pct) => pct.parse::<f32>().ok()? / 100.0,
                None => p.parse::<f32>().ok()?,
            },
            None => 1.0,
        };
        return Some(Rgba::bytes(
            channel(parts[0])?,
            channel(parts[1])?,
            channel(parts[2])?,
            a.clamp(0.0, 1.0),
        ));
    }
    let named = match lower.as_str() {
        "transparent" => return Some(Rgba::TRANSPARENT),
        "black" => 0x000000,
        "white" => 0xffffff,
        "red" => 0xff0000,
        "green" => 0x008000,
        "blue" => 0x0000ff,
        "yellow" => 0xffff00,
        "orange" => 0xffa500,
        "purple" => 0x800080,
        "gray" | "grey" => 0x808080,
        "lightgray" | "lightgrey" => 0xd3d3d3,
        "darkgray" | "darkgrey" => 0xa9a9a9,
        "pink" => 0xffc0cb,
        "brown" => 0xa52a2a,
        "cyan" | "aqua" => 0x00ffff,
        "magenta" | "fuchsia" => 0xff00ff,
        _ => return None,
    };
    Some(Rgba::bytes(
        (named >> 16) as u8,
        (named >> 8) as u8,
        named as u8,
        1.0,
    ))
}

/// Excalidraw's `isTransparent`: `transparent`, or a hex colour whose alpha
/// digits are zero.
pub fn is_transparent(color: &str) -> bool {
    (color.len() == 5 && color.get(4..5) == Some("0"))
        || (color.len() == 9 && color.get(7..9) == Some("00"))
        || color == "transparent"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_what_excalidraw_writes() {
        assert_eq!(parse("#1e1e1e"), Rgba::bytes(0x1e, 0x1e, 0x1e, 1.0));
        assert_eq!(parse("#fff"), Rgba::bytes(255, 255, 255, 1.0));
        assert_eq!(parse("#ff000080").a, 128.0 / 255.0);
        assert_eq!(parse("transparent"), Rgba::TRANSPARENT);
        assert_eq!(parse("rgba(0, 0, 200, 0.04)"), Rgba::bytes(0, 0, 200, 0.04));
        assert_eq!(parse("nonsense"), Rgba::BLACK);
        assert!(
            is_transparent("#ffffff00") && is_transparent("transparent") && !is_transparent("#fff")
        );
    }
}
