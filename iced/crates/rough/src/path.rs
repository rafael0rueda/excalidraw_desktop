//! path-data-parser 0.1.0 and points-on-path 0.2.1: SVG path data, reduced
//! to absolute `M`, `L`, `C` and `Z`.

use crate::jsmath;
use crate::points::{points_on_bezier_curves, simplify};
use crate::Point;

#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub key: char,
    pub data: Vec<f64>,
}

fn params(key: char) -> Option<usize> {
    Some(match key {
        'A' | 'a' => 7,
        'C' | 'c' => 6,
        'H' | 'h' | 'V' | 'v' => 1,
        'L' | 'l' | 'M' | 'm' | 'T' | 't' => 2,
        'Q' | 'q' | 'S' | 's' => 4,
        'Z' | 'z' => 0,
        _ => return None,
    })
}

enum Token {
    Command(char),
    Number(f64),
}

/// The original's regular expressions, by hand. None where it gives up.
fn tokenize(d: &str) -> Option<Vec<Token>> {
    let bytes = d.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if matches!(c, b' ' | b'\t' | b'\r' | b'\n' | b',') {
            i += 1;
        } else if params(c as char).is_some() {
            tokens.push(Token::Command(c as char));
            i += 1;
        } else {
            // ([-+]?[0-9]+(\.[0-9]*)?|[-+]?\.[0-9]+)([eE][-+]?[0-9]+)?
            let start = i;
            let mut j = i;
            if j < bytes.len() && matches!(bytes[j], b'-' | b'+') {
                j += 1;
            }
            let int_start = j;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            let has_int = j > int_start;
            let mut ok = has_int;
            if j < bytes.len() && bytes[j] == b'.' {
                let frac_start = j + 1;
                let mut k = frac_start;
                while k < bytes.len() && bytes[k].is_ascii_digit() {
                    k += 1;
                }
                if has_int || k > frac_start {
                    ok = true;
                    j = k;
                }
            }
            if !ok {
                return None;
            }
            if j < bytes.len() && matches!(bytes[j], b'e' | b'E') {
                let mut k = j + 1;
                if k < bytes.len() && matches!(bytes[k], b'-' | b'+') {
                    k += 1;
                }
                let exp_start = k;
                while k < bytes.len() && bytes[k].is_ascii_digit() {
                    k += 1;
                }
                if k > exp_start {
                    j = k;
                }
            }
            let text = &d[start..j];
            // `parseFloat` then `+`: both exact, so one correctly rounded parse.
            let text = text.strip_suffix('.').unwrap_or(text);
            tokens.push(Token::Number(text.parse().ok()?));
            i = j;
        }
    }
    Some(tokens)
}

/// `parsePath`. Where the original throws, this returns what it had parsed.
pub fn parse_path(d: &str) -> Vec<Segment> {
    let Some(tokens) = tokenize(d) else {
        return Vec::new();
    };
    let mut segments = Vec::new();
    let mut mode: Option<char> = None;
    let mut index = 0;
    while index < tokens.len() {
        let params_count;
        match (&tokens[index], mode) {
            (Token::Command(c @ ('M' | 'm')), None) => {
                index += 1;
                params_count = 2;
                mode = Some(*c);
            }
            (_, None) => return parse_path(&format!("M0,0{d}")),
            (Token::Number(_), Some(m)) => {
                params_count = params(m).unwrap_or(0);
            }
            (Token::Command(c), Some(_)) => {
                index += 1;
                params_count = params(*c).unwrap_or(0);
                mode = Some(*c);
            }
        }
        // `index + paramsCount < tokens.length`, where tokens end in EOD.
        if index + params_count > tokens.len() {
            return segments;
        }
        let mut data = Vec::with_capacity(params_count);
        for token in &tokens[index..index + params_count] {
            match token {
                Token::Number(n) => data.push(*n),
                Token::Command(_) => return segments,
            }
        }
        let key = mode.unwrap_or('M');
        segments.push(Segment { key, data });
        index += params_count;
        if key == 'M' {
            mode = Some('L');
        }
        if key == 'm' {
            mode = Some('l');
        }
        if params_count == 0 && index < tokens.len() && matches!(tokens[index], Token::Number(_)) {
            // `Z 5`: the original would loop forever; stop instead.
            return segments;
        }
    }
    segments
}

fn rel(data: &[f64], cx: f64, cy: f64) -> Vec<f64> {
    data.iter()
        .enumerate()
        .map(|(i, d)| if i % 2 == 1 { d + cy } else { d + cx })
        .collect()
}

/// `absolutize`: relative commands to absolute ones.
pub fn absolutize(segments: &[Segment]) -> Vec<Segment> {
    let (mut cx, mut cy, mut subx, mut suby) = (0.0, 0.0, 0.0, 0.0);
    let mut out = Vec::new();
    for Segment { key, data } in segments {
        let seg = |key: char, data: Vec<f64>| Segment { key, data };
        match key {
            'M' => {
                out.push(seg('M', data.clone()));
                (cx, cy) = (data[0], data[1]);
                (subx, suby) = (data[0], data[1]);
            }
            'm' => {
                cx += data[0];
                cy += data[1];
                out.push(seg('M', vec![cx, cy]));
                subx = cx;
                suby = cy;
            }
            'L' => {
                out.push(seg('L', data.clone()));
                (cx, cy) = (data[0], data[1]);
            }
            'l' => {
                cx += data[0];
                cy += data[1];
                out.push(seg('L', vec![cx, cy]));
            }
            'C' => {
                out.push(seg('C', data.clone()));
                cx = data[4];
                cy = data[5];
            }
            'c' => {
                let d = rel(data, cx, cy);
                cx = d[4];
                cy = d[5];
                out.push(seg('C', d));
            }
            'Q' => {
                out.push(seg('Q', data.clone()));
                cx = data[2];
                cy = data[3];
            }
            'q' => {
                let d = rel(data, cx, cy);
                cx = d[2];
                cy = d[3];
                out.push(seg('Q', d));
            }
            'A' => {
                out.push(seg('A', data.clone()));
                cx = data[5];
                cy = data[6];
            }
            'a' => {
                cx += data[5];
                cy += data[6];
                out.push(seg(
                    'A',
                    vec![data[0], data[1], data[2], data[3], data[4], cx, cy],
                ));
            }
            'H' => {
                out.push(seg('H', data.clone()));
                cx = data[0];
            }
            'h' => {
                cx += data[0];
                out.push(seg('H', vec![cx]));
            }
            'V' => {
                out.push(seg('V', data.clone()));
                cy = data[0];
            }
            'v' => {
                cy += data[0];
                out.push(seg('V', vec![cy]));
            }
            'S' => {
                out.push(seg('S', data.clone()));
                cx = data[2];
                cy = data[3];
            }
            's' => {
                let d = rel(data, cx, cy);
                cx = d[2];
                cy = d[3];
                out.push(seg('S', d));
            }
            'T' => {
                out.push(seg('T', data.clone()));
                cx = data[0];
                cy = data[1];
            }
            't' => {
                cx += data[0];
                cy += data[1];
                out.push(seg('T', vec![cx, cy]));
            }
            'Z' | 'z' => {
                out.push(seg('Z', vec![]));
                cx = subx;
                cy = suby;
            }
            _ => {}
        }
    }
    out
}

/// `normalize`: only `M`, `L`, `C` and `Z` remain.
pub fn normalize(segments: &[Segment]) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut last_type = ' ';
    let (mut cx, mut cy, mut subx, mut suby, mut lcx, mut lcy) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for Segment { key, data } in segments {
        let seg = |key: char, data: Vec<f64>| Segment { key, data };
        match key {
            'M' => {
                out.push(seg('M', data.clone()));
                (cx, cy) = (data[0], data[1]);
                (subx, suby) = (data[0], data[1]);
            }
            'C' => {
                out.push(seg('C', data.clone()));
                cx = data[4];
                cy = data[5];
                lcx = data[2];
                lcy = data[3];
            }
            'L' => {
                out.push(seg('L', data.clone()));
                (cx, cy) = (data[0], data[1]);
            }
            'H' => {
                cx = data[0];
                out.push(seg('L', vec![cx, cy]));
            }
            'V' => {
                cy = data[0];
                out.push(seg('L', vec![cx, cy]));
            }
            'S' => {
                let (cx1, cy1) = if last_type == 'C' || last_type == 'S' {
                    (cx + (cx - lcx), cy + (cy - lcy))
                } else {
                    (cx, cy)
                };
                out.push(seg('C', vec![cx1, cy1, data[0], data[1], data[2], data[3]]));
                lcx = data[0];
                lcy = data[1];
                cx = data[2];
                cy = data[3];
            }
            'T' => {
                let (x, y) = (data[0], data[1]);
                let (x1, y1) = if last_type == 'Q' || last_type == 'T' {
                    (cx + (cx - lcx), cy + (cy - lcy))
                } else {
                    (cx, cy)
                };
                let cx1 = cx + 2.0 * (x1 - cx) / 3.0;
                let cy1 = cy + 2.0 * (y1 - cy) / 3.0;
                let cx2 = x + 2.0 * (x1 - x) / 3.0;
                let cy2 = y + 2.0 * (y1 - y) / 3.0;
                out.push(seg('C', vec![cx1, cy1, cx2, cy2, x, y]));
                lcx = x1;
                lcy = y1;
                cx = x;
                cy = y;
            }
            'Q' => {
                let (x1, y1, x, y) = (data[0], data[1], data[2], data[3]);
                let cx1 = cx + 2.0 * (x1 - cx) / 3.0;
                let cy1 = cy + 2.0 * (y1 - cy) / 3.0;
                let cx2 = x + 2.0 * (x1 - x) / 3.0;
                let cy2 = y + 2.0 * (y1 - y) / 3.0;
                out.push(seg('C', vec![cx1, cy1, cx2, cy2, x, y]));
                lcx = x1;
                lcy = y1;
                cx = x;
                cy = y;
            }
            'A' => {
                let r1 = data[0].abs();
                let r2 = data[1].abs();
                let (angle, large_arc, sweep, x, y) = (data[2], data[3], data[4], data[5], data[6]);
                if r1 == 0.0 || r2 == 0.0 {
                    out.push(seg('C', vec![cx, cy, x, y, x, y]));
                    cx = x;
                    cy = y;
                } else if cx != x || cy != y {
                    for curve in
                        arc_to_cubic_curves(cx, cy, x, y, r1, r2, angle, large_arc, sweep, None)
                    {
                        out.push(seg('C', curve.to_vec()));
                    }
                    cx = x;
                    cy = y;
                }
            }
            'Z' => {
                out.push(seg('Z', vec![]));
                cx = subx;
                cy = suby;
            }
            _ => {}
        }
        last_type = *key;
    }
    out
}

fn rotate(x: f64, y: f64, angle_rad: f64) -> Point {
    [
        x * jsmath::cos(angle_rad) - y * jsmath::sin(angle_rad),
        x * jsmath::sin(angle_rad) + y * jsmath::cos(angle_rad),
    ]
}

/// `parseFloat(x.toFixed(9))`.
fn fixed9(x: f64) -> f64 {
    format!("{x:.9}").parse().unwrap_or(x)
}

#[allow(clippy::too_many_arguments)]
fn arc_to_cubic_curves(
    mut x1: f64,
    mut y1: f64,
    mut x2: f64,
    mut y2: f64,
    mut r1: f64,
    mut r2: f64,
    angle: f64,
    large_arc_flag: f64,
    sweep_flag: f64,
    recursive: Option<[f64; 4]>,
) -> Vec<[f64; 6]> {
    use std::f64::consts::PI;
    let angle_rad = (PI * angle) / 180.0;
    let mut params: Vec<Point> = Vec::new();
    let (mut f1, mut f2, cx, cy);
    let sweep = sweep_flag != 0.0;
    if let Some([rf1, rf2, rcx, rcy]) = recursive {
        (f1, f2, cx, cy) = (rf1, rf2, rcx, rcy);
    } else {
        [x1, y1] = rotate(x1, y1, -angle_rad);
        [x2, y2] = rotate(x2, y2, -angle_rad);
        let x = (x1 - x2) / 2.0;
        let y = (y1 - y2) / 2.0;
        let mut h = (x * x) / (r1 * r1) + (y * y) / (r2 * r2);
        if h > 1.0 {
            h = h.sqrt();
            r1 *= h;
            r2 *= h;
        }
        let sign = if large_arc_flag == sweep_flag {
            -1.0
        } else {
            1.0
        };
        let r1_pow = r1 * r1;
        let r2_pow = r2 * r2;
        let left = r1_pow * r2_pow - r1_pow * y * y - r2_pow * x * x;
        let right = r1_pow * y * y + r2_pow * x * x;
        let k = sign * (left / right).abs().sqrt();
        cx = k * r1 * y / r2 + (x1 + x2) / 2.0;
        cy = k * -r2 * x / r1 + (y1 + y2) / 2.0;
        f1 = jsmath::asin(fixed9((y1 - cy) / r2));
        f2 = jsmath::asin(fixed9((y2 - cy) / r2));
        if x1 < cx {
            f1 = PI - f1;
        }
        if x2 < cx {
            f2 = PI - f2;
        }
        if f1 < 0.0 {
            f1 += PI * 2.0;
        }
        if f2 < 0.0 {
            f2 += PI * 2.0;
        }
        if sweep && f1 > f2 {
            f1 -= PI * 2.0;
        }
        if !sweep && f2 > f1 {
            f2 -= PI * 2.0;
        }
    }
    let mut df = f2 - f1;
    if df.abs() > (PI * 120.0 / 180.0) {
        let f2old = f2;
        let x2old = x2;
        let y2old = y2;
        // `f1 + (PI * 120 / 180) * (±1)`; adding the negation is the same
        // operation bit for bit.
        if sweep && f2 > f1 {
            f2 = f1 + (PI * 120.0 / 180.0);
        } else {
            f2 = f1 - (PI * 120.0 / 180.0);
        }
        x2 = cx + r1 * jsmath::cos(f2);
        y2 = cy + r2 * jsmath::sin(f2);
        let rest = arc_to_cubic_curves(
            x2,
            y2,
            x2old,
            y2old,
            r1,
            r2,
            angle,
            0.0,
            sweep_flag,
            Some([f2, f2old, cx, cy]),
        );
        // The recursive form returns points, three per curve.
        params = rest
            .iter()
            .flat_map(|c| [[c[0], c[1]], [c[2], c[3]], [c[4], c[5]]])
            .collect();
    }
    df = f2 - f1;
    let c1 = jsmath::cos(f1);
    let s1 = jsmath::sin(f1);
    let c2 = jsmath::cos(f2);
    let s2 = jsmath::sin(f2);
    let t = jsmath::tan(df / 4.0);
    let hx = 4.0 / 3.0 * r1 * t;
    let hy = 4.0 / 3.0 * r2 * t;
    let m1 = [x1, y1];
    let mut m2 = [x1 + hx * s1, y1 - hy * c1];
    let m3 = [x2 + hx * s2, y2 - hy * c2];
    let m4 = [x2, y2];
    m2[0] = 2.0 * m1[0] - m2[0];
    m2[1] = 2.0 * m1[1] - m2[1];
    let mut all = vec![m2, m3, m4];
    all.extend(params);
    let curves = (0..all.len() / 3).map(|i| [all[3 * i], all[3 * i + 1], all[3 * i + 2]]);
    if recursive.is_some() {
        curves
            .map(|c| [c[0][0], c[0][1], c[1][0], c[1][1], c[2][0], c[2][1]])
            .collect()
    } else {
        curves
            .map(|c| {
                let r1 = rotate(c[0][0], c[0][1], angle_rad);
                let r2 = rotate(c[1][0], c[1][1], angle_rad);
                let r3 = rotate(c[2][0], c[2][1], angle_rad);
                [r1[0], r1[1], r2[0], r2[1], r3[0], r3[1]]
            })
            .collect()
    }
}

/// `normalize(absolutize(parsePath(d)))`.
pub fn segments(d: &str) -> Vec<Segment> {
    normalize(&absolutize(&parse_path(d)))
}

/// `pointsOnPath`: each subpath of `d` as a polyline.
pub fn points_on_path(d: &str, tolerance: f64, distance: Option<f64>) -> Vec<Vec<Point>> {
    let mut sets = Vec::new();
    let mut current: Vec<Point> = Vec::new();
    let mut start = [0.0, 0.0];
    let mut pending: Vec<Point> = Vec::new();

    fn append_pending_curve(pending: &mut Vec<Point>, current: &mut Vec<Point>, tolerance: f64) {
        if pending.len() >= 4 {
            current.extend(points_on_bezier_curves(pending, tolerance, None));
        }
        pending.clear();
    }

    for Segment { key, data } in segments(d) {
        match key {
            'M' => {
                append_pending_curve(&mut pending, &mut current, tolerance);
                if !current.is_empty() {
                    sets.push(std::mem::take(&mut current));
                }
                start = [data[0], data[1]];
                current.push(start);
            }
            'L' => {
                append_pending_curve(&mut pending, &mut current, tolerance);
                current.push([data[0], data[1]]);
            }
            'C' => {
                if pending.is_empty() {
                    let last = current.last().copied().unwrap_or(start);
                    pending.push(last);
                }
                pending.push([data[0], data[1]]);
                pending.push([data[2], data[3]]);
                pending.push([data[4], data[5]]);
            }
            'Z' => {
                append_pending_curve(&mut pending, &mut current, tolerance);
                current.push(start);
            }
            _ => {}
        }
    }
    append_pending_curve(&mut pending, &mut current, tolerance);
    if !current.is_empty() {
        sets.push(current);
    }
    let Some(distance) = distance.filter(|d| crate::truthy(*d)) else {
        return sets;
    };
    sets.iter()
        .map(|set| simplify(set, distance))
        .filter(|set| !set.is_empty())
        .collect()
}
