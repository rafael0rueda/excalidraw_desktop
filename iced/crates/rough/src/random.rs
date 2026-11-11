/// rough.js's `Random`: a Park–Miller generator over 32-bit integers.
///
/// ```js
/// next() {
///   if (this.seed) {
///     return ((2 ** 31 - 1) & (this.seed = Math.imul(48271, this.seed))) / 2 ** 31;
///   } else {
///     return Math.random();
///   }
/// }
/// ```
///
/// A seed of 0 makes rough.js fall back to `Math.random()`, which nothing can
/// reproduce; this falls back to a fixed sequence instead, so a drawing at
/// least looks the same on every repaint.
#[derive(Clone, Debug)]
pub struct Random {
    state: State,
}

#[derive(Clone, Debug)]
enum State {
    Seeded(i32),
    Unseeded(u64),
}

impl Random {
    pub fn new(seed: f64) -> Self {
        let state = if crate::truthy(seed) {
            State::Seeded(crate::jsmath::to_int32(seed))
        } else {
            State::Unseeded(0x9E37_79B9_7F4A_7C15)
        };
        Self { state }
    }

    #[allow(clippy::should_implement_trait)] // rough.js's name; it never ends
    pub fn next(&mut self) -> f64 {
        match &mut self.state {
            State::Seeded(seed) => {
                let next = 48271i32.wrapping_mul(*seed);
                let value = f64::from(next & 0x7fff_ffff) / 2147483648.0;
                // `if (this.seed)` on the next call: an integer now.
                if next == 0 {
                    self.state = State::Unseeded(0x9E37_79B9_7F4A_7C15);
                } else {
                    *seed = next;
                }
                value
            }
            State::Unseeded(x) => {
                // xorshift64*, in [0, 1).
                *x ^= *x >> 12;
                *x ^= *x << 25;
                *x ^= *x >> 27;
                let r = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
                (r >> 11) as f64 / (1u64 << 53) as f64
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Random;

    #[test]
    fn matches_the_first_values_rough_js_gives() {
        // node -e "import('roughjs/bin/math.js').then(m => { const r = new m.Random(1234); console.log(r.next(), r.next(), r.next()) })"
        let mut r = Random::new(1234.0);
        let got = [r.next(), r.next(), r.next()];
        let want = [
            0.027737773023545742,
            0.9300416195765138,
            0.03901857789605856,
        ];
        for (g, w) in got.iter().zip(want) {
            assert_eq!(*g, w);
        }
    }
}
