//! Text: measuring, wrapping and placing it, as Excalidraw 0.18.1 does
//! (`element/textElement.ts`, `element/textWrapping.ts`).
//!
//! Widths come from the app (`Measure`), which shapes with the same fonts
//! Excalidraw uses: the browser's `measureText` advance width there, the
//! shaped advance here.

use scene::Element;

/// `BOUND_TEXT_PADDING`.
pub const BOUND_TEXT_PADDING: f64 = 5.0;
/// `ARROW_LABEL_WIDTH_FRACTION`, `ARROW_LABEL_FONT_SIZE_TO_MIN_WIDTH_RATIO`.
const ARROW_LABEL_WIDTH_FRACTION: f64 = 0.7;
const ARROW_LABEL_FONT_SIZE_TO_MIN_WIDTH_RATIO: f64 = 11.0;

/// How wide a line of text is, in scene units: `getLineWidth`.
pub trait Measure {
    fn line_width(&self, line: &str, font_family: f64, font_size: f64) -> f64;
}

/// The font a text element is measured in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Font {
    pub family: f64,
    pub size: f64,
    pub line_height: f64,
}

/// `normalizeText`: line endings to `\n`, tabs to eight spaces.
pub fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', "        ")
}

/// `measureText`: the widest line, and the line height times the lines.
/// An empty line still counts as a line (it is measured as a space).
pub fn measure(text: &str, font: Font, m: &dyn Measure) -> (f64, f64) {
    let lines: Vec<&str> = text.split('\n').collect();
    let width = lines
        .iter()
        .map(|l| m.line_width(if l.is_empty() { " " } else { l }, font.family, font.size))
        .fold(0.0, f64::max);
    (width, font.size * font.line_height * lines.len() as f64)
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF | 0x2E80..=0x2FDF | 0x3040..=0x30FF | 0x3100..=0x312F | 0x3130..=0x318F
        | 0x31A0..=0x31FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7FF
        | 0xF900..=0xFAFF | 0x20000..=0x2FFFF)
        || "｀＇＾〃〰〆＃＆＊＋－ー／＼＝｜￤〒￢￣".contains(c)
}
fn is_cjk_opening(c: char) -> bool {
    "（［｛〈《｟｢「『【〖〔〘〚＜〝".contains(c)
}
fn is_cjk_closing(c: char) -> bool {
    "）］｝〉》｠｣」』】〗〕〙〛＞。．，、〟‥？！：；・〜〞".contains(c)
}
fn is_opening(c: char) -> bool {
    "<([{".contains(c)
}
fn is_closing(c: char) -> bool {
    ">)]}.,:;!?…/".contains(c)
}
fn is_currency(c: char) -> bool {
    "￥￦￡￠＄".contains(c)
}

/// `parseTokens`: where a line may break, as Excalidraw's line-break rules
/// say. Every whitespace is a token of its own; a hyphen ends a word; CJK
/// characters stand alone unless held by punctuation.
pub fn tokens(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| chars[j]);
        let break_before = !current.is_empty()
            && (c.is_whitespace()
                || prev.is_some_and(|p| p.is_whitespace() || p == '-')
                || ((is_cjk(c) || is_currency(c))
                    && !prev.is_some_and(|p| is_opening(p) || is_cjk_opening(p)))
                || (prev.is_some_and(is_cjk) && !(c == '-' || is_closing(c) || is_cjk_closing(c)))
                || (is_cjk_opening(c)
                    && !prev.is_some_and(|p| is_opening(p) || is_cjk_opening(p)))
                || (prev.is_some_and(is_cjk_closing) && !(is_closing(c) || is_cjk_closing(c)))
                || (prev.is_some_and(is_closing) && is_opening(c)));
        if break_before {
            out.push(std::mem::take(&mut current));
        }
        current.push(c);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// `wrapWord`: a word wider than the line, cut between characters.
fn wrap_word(word: &str, font: Font, max: f64, m: &dyn Measure) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut width = 0.0;
    for c in word.chars() {
        let cw = m.line_width(&c.to_string(), font.family, font.size);
        if width + cw <= max {
            current.push(c);
            width += cw;
            continue;
        }
        if !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        current.push(c);
        width = cw;
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// `trimLine`: trailing whitespace kept only as far as it fits.
fn trim_line(line: &str, font: Font, max: f64, m: &dyn Measure) -> String {
    if m.line_width(line, font.family, font.size) <= max {
        return line.to_owned();
    }
    let trimmed = line.trim_end();
    let mut out = trimmed.to_owned();
    let mut width = m.line_width(trimmed, font.family, font.size);
    for ws in line[trimmed.len()..].chars() {
        let cw = m.line_width(&ws.to_string(), font.family, font.size);
        if width + cw > max {
            break;
        }
        out.push(ws);
        width += cw;
    }
    out
}

/// `wrapLine`.
fn wrap_line(line: &str, font: Font, max: f64, m: &dyn Measure) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut width = 0.0;
    let mut tokens = tokens(line).into_iter().peekable();
    while let Some(token) = tokens.peek().cloned() {
        let test = format!("{current}{token}");
        let single = token.chars().count() == 1;
        let test_width = if single {
            width + m.line_width(&token, font.family, font.size)
        } else {
            m.line_width(&test, font.family, font.size)
        };
        if token.chars().all(char::is_whitespace) || test_width <= max {
            current = test;
            width = test_width;
            tokens.next();
            continue;
        }
        if current.is_empty() {
            let mut wrapped = wrap_word(&token, font, max, m);
            let trailing = wrapped.pop().unwrap_or_default();
            lines.extend(wrapped);
            width = m.line_width(&trailing, font.family, font.size);
            current = trailing;
            tokens.next();
        } else {
            lines.push(current.trim_end().to_owned());
            current.clear();
            width = 0.0;
        }
    }
    if !current.is_empty() {
        lines.push(trim_line(&current, font, max, m));
    }
    lines
}

/// `wrapText`: each line broken to fit `max` wide.
pub fn wrap(text: &str, font: Font, max: f64, m: &dyn Measure) -> String {
    if !max.is_finite() || max < 0.0 {
        return text.to_owned();
    }
    let mut lines = Vec::new();
    for line in text.split('\n') {
        if m.line_width(line, font.family, font.size) <= max {
            lines.push(line.to_owned());
        } else {
            lines.extend(wrap_line(line, font, max, m));
        }
    }
    lines.join("\n")
}

/// `getBoundTextMaxWidth`: how wide text may be inside `container`.
pub fn max_width(container: &Element, font_size: f64) -> f64 {
    let w = container.width;
    match container.kind {
        scene::Kind::Arrow => (ARROW_LABEL_WIDTH_FRACTION * w)
            .max(font_size * ARROW_LABEL_FONT_SIZE_TO_MIN_WIDTH_RATIO),
        scene::Kind::Ellipse => {
            rough::jsmath::round(w / 2.0 * 2f64.sqrt()) - BOUND_TEXT_PADDING * 2.0
        }
        scene::Kind::Diamond => rough::jsmath::round(w / 2.0) - BOUND_TEXT_PADDING * 2.0,
        _ => w - BOUND_TEXT_PADDING * 2.0,
    }
}

/// `getBoundTextMaxHeight`.
pub fn max_height(container: &Element, text_height: f64) -> f64 {
    let h = container.height;
    match container.kind {
        scene::Kind::Arrow => {
            if h - BOUND_TEXT_PADDING * 8.0 * 2.0 <= 0.0 {
                text_height
            } else {
                h
            }
        }
        scene::Kind::Ellipse => {
            rough::jsmath::round(h / 2.0 * 2f64.sqrt()) - BOUND_TEXT_PADDING * 2.0
        }
        scene::Kind::Diamond => rough::jsmath::round(h / 2.0) - BOUND_TEXT_PADDING * 2.0,
        _ => h - BOUND_TEXT_PADDING * 2.0,
    }
}

/// `computeContainerDimensionForBoundText`: how tall a container must be
/// for text `dimension` tall.
pub fn container_dimension(dimension: f64, kind: scene::Kind) -> f64 {
    let d = dimension.ceil();
    let padding = BOUND_TEXT_PADDING * 2.0;
    match kind {
        scene::Kind::Ellipse => rough::jsmath::round((d + padding) / 2f64.sqrt() * 2.0),
        scene::Kind::Arrow => d + padding * 8.0,
        scene::Kind::Diamond => 2.0 * (d + padding),
        _ => d + padding,
    }
}

/// `getContainerCoords`: the top left of the text area inside `container`.
fn container_coords(c: &Element) -> (f64, f64) {
    let (mut ox, mut oy) = (BOUND_TEXT_PADDING, BOUND_TEXT_PADDING);
    match c.kind {
        scene::Kind::Ellipse => {
            ox += c.width / 2.0 * (1.0 - 2f64.sqrt() / 2.0);
            oy += c.height / 2.0 * (1.0 - 2f64.sqrt() / 2.0);
        }
        scene::Kind::Diamond => {
            ox += c.width / 4.0;
            oy += c.height / 4.0;
        }
        _ => {}
    }
    (c.x + ox, c.y + oy)
}

/// `computeBoundTextPosition` for a shape: where text `w × h` sits inside it.
pub fn bound_position(
    container: &Element,
    text_align: &str,
    vertical_align: &str,
    w: f64,
    h: f64,
) -> (f64, f64) {
    let (cx, cy) = container_coords(container);
    let max_h = max_height(container, h);
    let max_w = max_width(container, 0.0);
    let y = match vertical_align {
        "top" => cy,
        "bottom" => cy + (max_h - h),
        _ => cy + (max_h / 2.0 - h / 2.0),
    };
    let x = match text_align {
        "left" => cx,
        "right" => cx + (max_w - w),
        _ => cx + (max_w / 2.0 - w / 2.0),
    };
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character as wide as the font is tall times 0.5.
    pub struct Mono;
    impl Measure for Mono {
        fn line_width(&self, line: &str, _: f64, size: f64) -> f64 {
            line.chars().count() as f64 * size * 0.5
        }
    }

    const F: Font = Font {
        family: 5.0,
        size: 20.0,
        line_height: 1.25,
    };

    #[test]
    fn tokens_follow_excalidraws_break_rules() {
        assert_eq!(tokens("Hello world"), ["Hello", " ", "world"]);
        assert_eq!(tokens("Hello-world"), ["Hello-", "world"]);
        assert_eq!(tokens("你好世界"), ["你", "好", "世", "界"]);
        assert_eq!(tokens("Hello た。"), ["Hello", " ", "た。"]);
        assert_eq!(tokens("Price￥100"), ["Price", "￥100"]);
        assert_eq!(tokens("a  b"), ["a", " ", " ", "b"]);
    }

    #[test]
    fn wrapping_breaks_between_words_then_inside_long_ones() {
        // 10 units per character at size 20.
        assert_eq!(wrap("one two three", F, 75.0, &Mono), "one two\nthree");
        assert_eq!(wrap("abcdefghij", F, 45.0, &Mono), "abcd\nefgh\nij");
        assert_eq!(
            wrap("short\nlines stay", F, 1000.0, &Mono),
            "short\nlines stay"
        );
    }

    #[test]
    fn measuring_counts_empty_lines() {
        let (w, h) = measure("ab\n\nabcd", F, &Mono);
        assert_eq!((w, h), (40.0, 75.0));
    }
}
