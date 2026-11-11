//! The JavaScript `Math` behaviour rough.js and Excalidraw depend on, where
//! Rust's own differs.

/// `Math.round`: halves go up, towards +∞. Rust's `round` sends them away
/// from zero, so `-2.5` would become `-3` instead of `-2`.
pub fn round(x: f64) -> f64 {
    let floor = x.floor();
    if x - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// ECMAScript `ToInt32`, what `Math.imul` and `|` apply to their operands.
pub fn to_int32(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    x.trunc().rem_euclid(4294967296.0) as u32 as i32
}

/// `Math.max` of two: NaN if either is.
pub fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b || (a == b && b.is_sign_negative()) {
        a
    } else {
        b
    }
}

/// `Math.min` of two: NaN if either is.
pub fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b || (a == b && b.is_sign_positive()) {
        a
    } else {
        b
    }
}

/// `Math.sin` and `Math.cos`: V8's fdlibm, bit for bit (see `fdlibm.rs`).
pub fn sin(x: f64) -> f64 {
    crate::fdlibm::sin(x)
}
pub fn cos(x: f64) -> f64 {
    crate::fdlibm::cos(x)
}

/// The rest are fdlibm in V8, as `libm` is.
pub fn tan(x: f64) -> f64 {
    libm::tan(x)
}
pub fn atan(x: f64) -> f64 {
    libm::atan(x)
}
pub fn atan2(y: f64, x: f64) -> f64 {
    libm::atan2(y, x)
}
pub fn asin(x: f64) -> f64 {
    libm::asin(x)
}

/// V8's `Math.hypot`: scale by the largest magnitude, then a compensated sum.
/// Not `libm::hypot`, which can differ in the last bit.
pub fn hypot(values: &[f64]) -> f64 {
    let mut max = 0.0f64;
    let mut one_arg_is_nan = false;
    for &v in values {
        let a = v.abs();
        if a.is_nan() {
            one_arg_is_nan = true;
        } else if a > max {
            max = a;
        }
    }
    if max == f64::INFINITY {
        return f64::INFINITY;
    }
    if one_arg_is_nan {
        return f64::NAN;
    }
    if max == 0.0 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut compensation = 0.0;
    for &v in values {
        let n = v.abs() / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

#[cfg(test)]
mod tests {
    #[test]
    fn math_round_sends_halves_up() {
        assert_eq!(super::round(2.5), 3.0);
        assert_eq!(super::round(-2.5), -2.0);
        assert_eq!(super::round(-2.6), -3.0);
        assert_eq!(super::round(0.49999999999999994), 0.0);
    }

    #[test]
    fn to_int32_wraps() {
        assert_eq!(super::to_int32(4294967296.0 + 5.0), 5);
        assert_eq!(super::to_int32(2147483648.0), i32::MIN);
        assert_eq!(super::to_int32(-1.9), -1);
    }
}
