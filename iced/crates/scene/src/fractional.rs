//! fractional-indexing 3.2.0: the order keys Excalidraw stores as each
//! element's `index`, so that z-order survives merging and reordering.
//!
//! Keys compare as strings. A key is an integer part, whose length its first
//! character encodes, then a base-62 fraction with no trailing zero.

const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const ZERO: u8 = b'0';

fn digit(c: u8) -> usize {
    DIGITS.iter().position(|d| *d == c).unwrap_or(0)
}

fn midpoint(a: &[u8], b: Option<&[u8]>) -> Option<Vec<u8>> {
    if let Some(b) = b {
        if a >= b {
            return None;
        }
        if b.last() == Some(&ZERO) {
            return None;
        }
    }
    if a.last() == Some(&ZERO) {
        return None;
    }
    if let Some(b) = b {
        let mut n = 0;
        while n < b.len() && a.get(n).copied().unwrap_or(ZERO) == b[n] {
            n += 1;
        }
        if n > 0 {
            let mut out = b[..n].to_vec();
            out.extend(midpoint(a.get(n..).unwrap_or(&[]), Some(&b[n..]))?);
            return Some(out);
        }
    }
    let digit_a = a.first().map_or(0, |c| digit(*c));
    let digit_b = b.map_or(DIGITS.len(), |b| b.first().map_or(0, |c| digit(*c)));
    if digit_b - digit_a > 1 {
        // `Math.round(0.5 * (digitA + digitB))`: halves go up.
        let mid = (digit_a + digit_b).div_ceil(2);
        Some(vec![DIGITS[mid]])
    } else if let Some(b) = b.filter(|b| b.len() > 1) {
        Some(b[..1].to_vec())
    } else {
        let mut out = vec![DIGITS[digit_a]];
        out.extend(midpoint(a.get(1..).unwrap_or(&[]), None)?);
        Some(out)
    }
}

fn integer_length(head: u8) -> Option<usize> {
    match head {
        b'a'..=b'z' => Some(usize::from(head - b'a') + 2),
        b'A'..=b'Z' => Some(usize::from(b'Z' - head) + 2),
        _ => None,
    }
}

fn integer_part(key: &[u8]) -> Option<&[u8]> {
    let len = integer_length(*key.first()?)?;
    key.get(..len)
}

/// Whether `key` is a key at all: `validateOrderKey`.
pub fn is_valid_key(key: &str) -> bool {
    let k = key.as_bytes();
    if k.len() == 27 && k[0] == b'A' && k[1..].iter().all(|c| *c == ZERO) {
        return false;
    }
    match integer_part(k) {
        Some(i) => k[i.len()..].last() != Some(&ZERO) && k.iter().all(|c| DIGITS.contains(c)),
        None => false,
    }
}

fn increment_integer(x: &[u8]) -> Option<Option<Vec<u8>>> {
    if x.len() != integer_length(x[0])? {
        return None;
    }
    let head = x[0];
    let mut digs = x[1..].to_vec();
    let mut carry = true;
    for d in digs.iter_mut().rev() {
        if !carry {
            break;
        }
        let next = digit(*d) + 1;
        if next == DIGITS.len() {
            *d = ZERO;
        } else {
            *d = DIGITS[next];
            carry = false;
        }
    }
    if !carry {
        let mut out = vec![head];
        out.extend(digs);
        return Some(Some(out));
    }
    if head == b'Z' {
        return Some(Some(vec![b'a', ZERO]));
    }
    if head == b'z' {
        return Some(None);
    }
    let h = head + 1;
    if h > b'a' {
        digs.push(ZERO);
    } else {
        digs.pop();
    }
    let mut out = vec![h];
    out.extend(digs);
    Some(Some(out))
}

fn decrement_integer(x: &[u8]) -> Option<Option<Vec<u8>>> {
    if x.len() != integer_length(x[0])? {
        return None;
    }
    let head = x[0];
    let last = DIGITS[DIGITS.len() - 1];
    let mut digs = x[1..].to_vec();
    let mut borrow = true;
    for d in digs.iter_mut().rev() {
        if !borrow {
            break;
        }
        let i = digit(*d);
        if i == 0 {
            *d = last;
        } else {
            *d = DIGITS[i - 1];
            borrow = false;
        }
    }
    if !borrow {
        let mut out = vec![head];
        out.extend(digs);
        return Some(Some(out));
    }
    if head == b'a' {
        return Some(Some(vec![b'Z', last]));
    }
    if head == b'A' {
        return Some(None);
    }
    let h = head - 1;
    if h < b'Z' {
        digs.push(last);
    } else {
        digs.pop();
    }
    let mut out = vec![h];
    out.extend(digs);
    Some(Some(out))
}

fn string(v: Vec<u8>) -> String {
    // Only ever built from DIGITS and the key heads, all ASCII.
    String::from_utf8(v).unwrap_or_default()
}

/// `generateKeyBetween(a, b)`: a key that sorts strictly between them. None
/// where the original throws: an invalid key, or `a >= b`.
pub fn key_between(a: Option<&str>, b: Option<&str>) -> Option<String> {
    if a.is_some_and(|a| !is_valid_key(a)) || b.is_some_and(|b| !is_valid_key(b)) {
        return None;
    }
    if let (Some(a), Some(b)) = (a, b) {
        if a >= b {
            return None;
        }
    }
    let (a, b) = (a.map(str::as_bytes), b.map(str::as_bytes));
    match (a, b) {
        (None, None) => Some("a0".into()),
        (None, Some(b)) => {
            let ib = integer_part(b)?;
            let fb = &b[ib.len()..];
            if ib.len() == 27 && ib[0] == b'A' && ib[1..].iter().all(|c| *c == ZERO) {
                let mut out = ib.to_vec();
                out.extend(midpoint(&[], Some(fb))?);
                return Some(string(out));
            }
            if ib < b {
                return Some(string(ib.to_vec()));
            }
            decrement_integer(ib)?.map(string)
        }
        (Some(a), None) => {
            let ia = integer_part(a)?;
            let fa = &a[ia.len()..];
            match increment_integer(ia)? {
                Some(i) => Some(string(i)),
                None => {
                    let mut out = ia.to_vec();
                    out.extend(midpoint(fa, None)?);
                    Some(string(out))
                }
            }
        }
        (Some(a), Some(b)) => {
            let ia = integer_part(a)?;
            let fa = &a[ia.len()..];
            let ib = integer_part(b)?;
            let fb = &b[ib.len()..];
            if ia == ib {
                let mut out = ia.to_vec();
                out.extend(midpoint(fa, Some(fb))?);
                return Some(string(out));
            }
            let i = increment_integer(ia)??;
            if i.as_slice() < b {
                return Some(string(i));
            }
            let mut out = ia.to_vec();
            out.extend(midpoint(fa, None)?);
            Some(string(out))
        }
    }
}

/// `generateNKeysBetween(a, b, n)`: `n` keys in order between them.
pub fn n_keys_between(a: Option<&str>, b: Option<&str>, n: usize) -> Option<Vec<String>> {
    match n {
        0 => return Some(Vec::new()),
        1 => return Some(vec![key_between(a, b)?]),
        _ => {}
    }
    if b.is_none() {
        let mut c = key_between(a, b)?;
        let mut out = vec![c.clone()];
        for _ in 0..n - 1 {
            c = key_between(Some(&c), b)?;
            out.push(c.clone());
        }
        return Some(out);
    }
    if a.is_none() {
        let mut c = key_between(a, b)?;
        let mut out = vec![c.clone()];
        for _ in 0..n - 1 {
            c = key_between(a, Some(&c))?;
            out.push(c.clone());
        }
        out.reverse();
        return Some(out);
    }
    let mid = n / 2;
    let c = key_between(a, b)?;
    let mut out = n_keys_between(a, Some(&c), mid)?;
    out.push(c.clone());
    out.extend(n_keys_between(Some(&c), b, n - mid - 1)?);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// From the original, run on the same inputs.
    #[test]
    fn matches_fractional_indexing() {
        let cases: &[(Option<&str>, Option<&str>, &str)] = &[
            (None, None, "a0"),
            (Some("a0"), None, "a1"),
            (None, Some("a0"), "Zz"),
            (Some("a0"), Some("a1"), "a0V"),
            (Some("a0"), Some("a0V"), "a0G"),
            (Some("a1"), Some("a2"), "a1V"),
            (Some("Zz"), Some("a0"), "ZzV"),
            (Some("a0V"), Some("a1"), "a0l"),
            (Some("az"), None, "b00"),
            (Some("b00"), None, "b01"),
            (None, Some("Zz"), "Zy"),
            (Some("a0"), Some("a0G"), "a08"),
            (Some("a5"), Some("b10"), "a6"),
            (
                Some("zzzzzzzzzzzzzzzzzzzzzzzzzzz"),
                None,
                "zzzzzzzzzzzzzzzzzzzzzzzzzzzV",
            ),
        ];
        for (a, b, want) in cases {
            assert_eq!(key_between(*a, *b).as_deref(), Some(*want), "{a:?}..{b:?}");
        }
        assert_eq!(
            n_keys_between(Some("a0"), Some("a1"), 5).unwrap(),
            ["a08", "a0G", "a0V", "a0d", "a0l"]
        );
        assert_eq!(
            n_keys_between(None, Some("a0"), 3).unwrap(),
            ["Zx", "Zy", "Zz"]
        );
        assert_eq!(
            n_keys_between(Some("a0"), None, 4).unwrap(),
            ["a1", "a2", "a3", "a4"]
        );
    }

    #[test]
    fn invalid_keys_are_refused() {
        for bad in ["", "a", "a90", "a00", "A000000000000000000000000000", "a1 "] {
            assert!(!is_valid_key(bad), "{bad:?}");
        }
        assert_eq!(key_between(Some("a1"), Some("a0")), None);
    }
}
