//! The reference's `Random` class and the script-visible random functions built on it.
//!
//! A `Random` is a single `double` holding a Park-Miller state (multiplier 16807, modulus
//! 2^31-1). The game keeps two global instances: `0x1222388` (used by particle code, see
//! [`crate::particles::ParticleRng`]) and `0x1221d18` (used by the `Random*` script functions and
//! a handful of engine rolls such as the Sampo orb in `0x78e640`). Both start at 0.0.
//!
//! Ported from: `Random::Next()` (`0x485a60`), `NextF(min, max)` (`0x485ab0` / `0x44d0e0`),
//! `Next(min, max)` (`0x485b10` / `0x44d140`), `SetRandomSeed` (`0x485890`, hash `0x8720c0`),
//! the distribution sampler (`0x844100`, `0x8442f0`) and the Lua bindings `SetRandomSeed`
//! (`0x7c9300`), `Random` (`0x7c9610`), `Randomf` (`0x7c9b40`), `RandomDistribution` (`0x7ca080`),
//! `RandomDistributionf` (`0x7ca460`), `ProceduralRandom` (`0x7ca810`), `ProceduralRandomf`
//! (`0x7caec0`) and `ProceduralRandomi` (`0x7cb550`).
//!
//! Checked against the exe by `tests/{set_seed,random}_golden.rs` (vectors from Ghidra's emulator).

const SCALE: f64 = 4.656_612_875e-10;
const MODULUS: i32 = 0x7FFF_FFFF;
/// The engine's own (truncated) pi in the distribution sampler (`0x844100`).
#[allow(clippy::approx_constant)]
const SAMPLER_PI: f32 = 3.1415;

/// `cvttsd2si`: truncates toward zero; anything unrepresentable becomes `i32::MIN`. This is also
/// what the 32-bit `lua_tointeger` does to a number.
pub fn cvtt32(v: f64) -> i32 {
    if v.is_nan() || v >= 2_147_483_648.0 || v <= -2_147_483_649.0 {
        i32::MIN
    } else {
        v as i32
    }
}

/// CRT double-to-int64 conversion (`0xdfdfa1`), keeping the low 32 bits the caller reads.
/// Values outside the int64 range or NaN give the "integer indefinite" 0x8000...0 pattern.
fn dtol_low(v: f64) -> u32 {
    if v.is_nan() || v >= 9_223_372_036_854_775_808.0 || v < -9_223_372_036_854_775_808.0 {
        0
    } else {
        (v as i64) as u32
    }
}

/// Bob Jenkins' 96-bit mix (lookup2), returning `c` (`0x8720c0`).
pub fn mix(mut a: u32, mut b: u32, mut c: u32) -> u32 {
    a = a.wrapping_sub(b).wrapping_sub(c) ^ (c >> 13);
    b = b.wrapping_sub(c).wrapping_sub(a) ^ (a << 8);
    c = c.wrapping_sub(a).wrapping_sub(b) ^ (b >> 13);
    a = a.wrapping_sub(b).wrapping_sub(c) ^ (c >> 12);
    b = b.wrapping_sub(c).wrapping_sub(a) ^ (a << 16);
    c = c.wrapping_sub(a).wrapping_sub(b) ^ (b >> 5);
    a = a.wrapping_sub(b).wrapping_sub(c) ^ (c >> 3);
    b = b.wrapping_sub(c).wrapping_sub(a) ^ (a << 10);
    c = c.wrapping_sub(a).wrapping_sub(b) ^ (b >> 15);
    c
}

/// A Park-Miller generator whose state lives in a double.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Random {
    state: f64,
}

impl Random {
    /// An engine global before anything touched it: state 0.0 (its first step yields 2^31-1).
    pub fn zero() -> Self {
        Self { state: 0.0 }
    }

    /// `seed` is mapped into the valid state range `1..2^31-1`. The engine never builds a
    /// generator this way; it is a convenience for tests and the game harness.
    pub fn new(seed: u32) -> Self {
        Self {
            state: f64::from(seed % 0x7FFF_FFFE + 1),
        }
    }

    /// `Random::Random(double seed)` (`0x44d070`): the seed (halved once if it reaches 2^31-1)
    /// becomes the state, then one step is taken. World generation builds its generators this way.
    pub fn from_seed(seed: f64) -> Self {
        let mut r = Self {
            state: if seed >= 2_147_483_647.0 {
                seed * 0.5
            } else {
                seed
            },
        };
        r.step();
        r
    }

    pub fn state(&self) -> i32 {
        cvtt32(self.state)
    }

    /// One Park-Miller step, `s * 16807 mod (2^31-1)`, in the engine's Schrage form with 32-bit
    /// wrapping arithmetic. Returns the new state.
    pub(crate) fn step(&mut self) -> i32 {
        let s = cvtt32(self.state);
        let q = s / 0x1F31D;
        let mut v = s.wrapping_mul(0x41A7).wrapping_sub(q.wrapping_mul(MODULUS));
        if v < 1 {
            v = v.wrapping_add(MODULUS);
        }
        self.state = f64::from(v);
        v
    }

    /// `Next()`: a double in `(0, 1)` (`0x485a60`).
    pub fn next_unit(&mut self) -> f64 {
        f64::from(self.step()) * SCALE
    }

    /// The single-precision unit value the script functions return (`(float)(state * 4.66e-10)`).
    pub fn next_unit_f32(&mut self) -> f32 {
        let v = self.step();
        (f64::from(v) * SCALE) as f32
    }

    /// `min + next * (max - min)` in single precision (`0x485ab0`, `0x44d0e0`).
    pub fn float(&mut self, min: f32, max: f32) -> f32 {
        let span = max - min;
        self.next_unit_f32() * span + min
    }

    /// Integer in `min..=max` (`0x485b10`, `0x44d140`). The product is formed as
    /// `(max - min + 1) * (state * -scale)`, as in the original.
    pub fn int(&mut self, min: i32, max: i32) -> i32 {
        let v = self.step();
        let range = max.wrapping_sub(min).wrapping_add(1);
        min.wrapping_sub(cvtt32(f64::from(range) * (f64::from(v) * -SCALE)))
    }

    /// `SetRandomSeed` (`0x485890`): derives the state from the world seed (plus the new-game+
    /// count) and a position. The state is then stepped once, plus `seed & 3` more times.
    pub fn set_seed(&mut self, seed: i32, x: f64, y: f64) {
        let t = seed as u32 ^ 0x9326_2E6F;
        let a = f64::from(t & 0xFFF) + x;
        let b = f64::from((t >> 12) & 0xFFF) + y;
        let ia = dtol_low(a * 134_217_727.0);
        let wide = if 102_400.0 > b.abs() && a.abs() > 1.0 {
            (b * 3483.328 + f64::from(ia)) * b
        } else {
            b * 134_217_727.0
        };
        let ib = dtol_low(wide);
        let hash = mix(ia, ib, seed as u32);
        let mut state = f64::from(hash) / 4_294_967_295.0 * 2_147_483_639.0 + 1.0;
        if state >= 2_147_483_647.0 {
            state *= 0.5;
        }
        self.state = state;
        self.step();
        for _ in 0..(seed & 3) {
            self.step();
        }
    }

    /// Rejection sampler for a peaked distribution on `0..1` (`0x844100`). `mean` is the peak
    /// position, `sharpness` the exponent applied to the sine bump and `baseline` the floor
    /// probability. Falls back to a plain draw after 100 rejections.
    pub fn distribution_unit(&mut self, mean: f32, sharpness: f32, baseline: f32) -> f32 {
        for _ in 0..100 {
            let candidate = self.next_unit_f32() + 0.0;
            let roll = self.next_unit_f32() + 0.0;
            let dist = (candidate - mean).abs();
            if (1.0 - dist) * baseline > roll {
                return candidate;
            }
            if dist < 0.5 {
                let t = ((0.5 - mean) + candidate) * SAMPLER_PI;
                let bump = f64::from(t).sin() as f32;
                let p = f64::from(bump).powf(f64::from(sharpness)) as f32;
                if p >= roll {
                    return candidate;
                }
            }
        }
        self.next_unit_f32() + 0.0
    }

    /// `RandomDistributionf` core (`0x8442f0`): uniform when `sharpness` is 0, otherwise the
    /// sampler above scaled to `min..max`.
    pub fn distribution_float(
        &mut self,
        min: f32,
        max: f32,
        mean: f32,
        sharpness: f32,
        baseline: f32,
    ) -> f32 {
        if sharpness == 0.0 {
            return self.float(min, max);
        }
        let span = max - min;
        let unit = self.distribution_unit((mean - min) / span, sharpness, baseline);
        unit * span + min
    }

    /// `RandomDistribution` (`0x7ca080`): the integer variant, rounded half away from zero.
    pub fn distribution_int(
        &mut self,
        min: i32,
        max: i32,
        mean: i32,
        sharpness: f32,
        baseline: f32,
    ) -> i32 {
        if sharpness == 0.0 {
            return self.int(min, max);
        }
        let span = max.wrapping_sub(min);
        let unit = self.distribution_unit(
            mean.wrapping_sub(min) as f32 / span as f32,
            sharpness,
            baseline,
        );
        let rounded = (unit * span as f32).round();
        cvtt32(f64::from(rounded)).wrapping_add(min)
    }
}

/// The script RNG: the shared generator at `0x1221d18` plus the seed parts the position-based
/// functions mix in (`0x1205004` world seed, `0x1205024` `NEW_GAME_PLUS_COUNT`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScriptRandom {
    pub rng: Random,
    pub world_seed: i32,
    pub new_game_plus: i32,
}

impl ScriptRandom {
    pub fn new(world_seed: i32) -> Self {
        Self {
            rng: Random::zero(),
            world_seed,
            new_game_plus: 0,
        }
    }

    /// The seed handed to `Random::set_seed` by every position-based function.
    pub fn seed(&self) -> i32 {
        self.world_seed.wrapping_add(self.new_game_plus)
    }

    /// A temporary generator seeded for a position, as the `Procedural*` functions use.
    fn at(&self, x: f64, y: f64) -> Random {
        let mut rng = Random::zero();
        rng.set_seed(self.seed(), x, y);
        rng
    }

    /// `SetRandomSeed(x, y)`: reseeds the shared generator.
    pub fn set_random_seed(&mut self, x: f64, y: f64) {
        let seed = self.seed();
        self.rng.set_seed(seed, x, y);
    }

    /// `Random()` and `Randomf()` with no arguments: a single-precision value in `(0, 1)`.
    pub fn random(&mut self) -> f64 {
        f64::from(self.rng.next_unit_f32() + 0.0)
    }

    /// `Random(min, max)`; `Random(max)` is `random_int(0, max)`.
    pub fn random_int(&mut self, min: i32, max: i32) -> i32 {
        self.rng.int(min, max)
    }

    /// `Randomf(min, max)`; `Randomf(max)` is `random_float(0.0, max)`.
    pub fn random_float(&mut self, min: f32, max: f32) -> f32 {
        self.rng.float(min, max)
    }

    /// `ProceduralRandom(x, y)` and `ProceduralRandomf(x, y)`: a double in `(0, 1)`.
    pub fn procedural_unit(&self, x: f64, y: f64) -> f64 {
        self.at(x, y).next_unit()
    }

    /// `ProceduralRandom(x, y, max)` and `ProceduralRandomi(x, y, max)` are
    /// `procedural_int(x, y, 0, max)`; the four-argument form takes `min`.
    pub fn procedural_int(&self, x: f64, y: f64, min: i32, max: i32) -> i32 {
        self.at(x, y).int(min, max)
    }

    /// `ProceduralRandom(x, y, min, max)` and `ProceduralRandomf(x, y, ...)`; the three-argument
    /// form has `min = 0`.
    pub fn procedural_float(&self, x: f64, y: f64, min: f32, max: f32) -> f32 {
        self.at(x, y).float(min, max)
    }

    /// `ProceduralRandomi(x, y)`: 0 or 1 (the unit value rounded half away from zero).
    pub fn procedural_bit(&self, x: f64, y: f64) -> i32 {
        let v = self.at(x, y).next_unit();
        let shifted = if v >= 0.0 { v + 0.5 } else { v - 0.5 };
        cvtt32(shifted.floor())
    }

    /// `RandomDistribution(min, max, mean, sharpness, baseline)`; defaults 1.0 and 0.005.
    pub fn random_distribution(
        &mut self,
        min: i32,
        max: i32,
        mean: i32,
        sharpness: f32,
        baseline: f32,
    ) -> i32 {
        self.rng
            .distribution_int(min, max, mean, sharpness, baseline)
    }

    /// `RandomDistributionf(min, max, mean, sharpness, baseline)`; defaults 1.0 and 0.005.
    pub fn random_distribution_float(
        &mut self,
        min: f32,
        max: f32,
        mean: f32,
        sharpness: f32,
        baseline: f32,
    ) -> f32 {
        self.rng
            .distribution_float(min, max, mean, sharpness, baseline)
    }
}
