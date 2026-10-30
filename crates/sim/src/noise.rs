//! 3D simplex noise as the reference implements it (`0x872560`): Gustavson's algorithm in f64 with
//! the permutation table at `0xFDF530`, gradients at `0x1155338`, `0.6` falloff and a `32.0` scale.
//! Particles use it for their swirl term.

/// `perm[i & 255]` (`0xFDF530`, 256 shorts; the classic Perlin permutation).
const P: [u8; 256] = [
    151, 160, 137, 91, 90, 15, 131, 13, 201, 95, 96, 53, 194, 233, 7, 225, 140, 36, 103, 30, 69,
    142, 8, 99, 37, 240, 21, 10, 23, 190, 6, 148, 247, 120, 234, 75, 0, 26, 197, 62, 94, 252, 219,
    203, 117, 35, 11, 32, 57, 177, 33, 88, 237, 149, 56, 87, 174, 20, 125, 136, 171, 168, 68, 175,
    74, 165, 71, 134, 139, 48, 27, 166, 77, 146, 158, 231, 83, 111, 229, 122, 60, 211, 133, 230,
    220, 105, 92, 41, 55, 46, 245, 40, 244, 102, 143, 54, 65, 25, 63, 161, 1, 216, 80, 73, 209, 76,
    132, 187, 208, 89, 18, 169, 200, 196, 135, 130, 116, 188, 159, 86, 164, 100, 109, 198, 173,
    186, 3, 64, 52, 217, 226, 250, 124, 123, 5, 202, 38, 147, 118, 126, 255, 82, 85, 212, 207, 206,
    59, 227, 47, 16, 58, 17, 182, 189, 28, 42, 223, 183, 170, 213, 119, 248, 152, 2, 44, 154, 163,
    70, 221, 153, 101, 155, 167, 43, 172, 9, 129, 22, 39, 253, 19, 98, 108, 110, 79, 113, 224, 232,
    178, 185, 112, 104, 218, 246, 97, 228, 251, 34, 242, 193, 238, 210, 144, 12, 191, 179, 162,
    241, 81, 51, 145, 235, 249, 14, 239, 107, 49, 192, 214, 31, 181, 199, 106, 157, 184, 84, 204,
    176, 115, 121, 50, 45, 127, 4, 150, 254, 138, 236, 205, 93, 222, 114, 67, 29, 24, 72, 243, 141,
    128, 195, 78, 66, 215, 61, 156, 180,
];

/// `0x1155338`: twelve edge-midpoint gradients (stride 0x20; only x, y, z are used).
const GRAD3: [[f64; 3]; 12] = [
    [1.0, 1.0, 0.0],
    [-1.0, 1.0, 0.0],
    [1.0, -1.0, 0.0],
    [-1.0, -1.0, 0.0],
    [1.0, 0.0, 1.0],
    [-1.0, 0.0, 1.0],
    [1.0, 0.0, -1.0],
    [-1.0, 0.0, -1.0],
    [0.0, 1.0, 1.0],
    [0.0, -1.0, 1.0],
    [0.0, 1.0, -1.0],
    [0.0, -1.0, -1.0],
];

const F3: f64 = 1.0 / 3.0;
const G3: f64 = 1.0 / 6.0;

/// `perm[i]` over the doubled table (`P[i & 255]`).
fn perm(i: usize) -> usize {
    usize::from(P[i & 255])
}

fn perm_mod12(i: usize) -> usize {
    perm(i) % 12
}

fn floor(v: f64) -> i32 {
    let t = v as i32;
    if f64::from(t) > v { t - 1 } else { t }
}

/// One corner's contribution: `(0.6 - |d|^2)^4 * dot(gradient, d)`, or 0 outside the radius.
fn corner(gradient: usize, x: f64, y: f64, z: f64) -> f64 {
    let t = 0.6 - x * x - y * y - z * z;
    if t < 0.0 {
        return 0.0;
    }
    let t = t * t;
    let g = GRAD3[gradient];
    (g[1] * y + g[0] * x + g[2] * z) * (t * t)
}

/// `noise(x, y, z)`, roughly in [-1, 1].
pub fn simplex3(xin: f64, yin: f64, zin: f64) -> f64 {
    let s = (xin + yin + zin) * F3;
    let i = floor(xin + s);
    let j = floor(yin + s);
    let k = floor(zin + s);
    let t = f64::from(i + j + k) * G3;
    let x0 = xin - (f64::from(i) - t);
    let y0 = yin - (f64::from(j) - t);
    let z0 = zin - (f64::from(k) - t);

    let ((i1, j1, k1), (i2, j2, k2)): ((u8, u8, u8), (u8, u8, u8)) = if x0 >= y0 {
        if y0 >= z0 {
            ((1, 0, 0), (1, 1, 0))
        } else if x0 >= z0 {
            ((1, 0, 0), (1, 0, 1))
        } else {
            ((0, 0, 1), (1, 0, 1))
        }
    } else if y0 < z0 {
        ((0, 0, 1), (0, 1, 1))
    } else if x0 < z0 {
        ((0, 1, 0), (0, 1, 1))
    } else {
        ((0, 1, 0), (1, 1, 0))
    };

    let (x1, y1, z1) = (
        x0 - f64::from(i1) + G3,
        y0 - f64::from(j1) + G3,
        z0 - f64::from(k1) + G3,
    );
    // 2 * G3 is the same double as F3.
    let (x2, y2, z2) = (
        x0 - f64::from(i2) + F3,
        y0 - f64::from(j2) + F3,
        z0 - f64::from(k2) + F3,
    );
    let (x3, y3, z3) = (x0 - 1.0 + 0.5, y0 - 1.0 + 0.5, z0 - 1.0 + 0.5);

    let (ii, jj, kk) = ((i & 255) as usize, (j & 255) as usize, (k & 255) as usize);
    let g0 = perm_mod12(ii + perm(jj + perm(kk)));
    let g1 =
        perm_mod12(ii + usize::from(i1) + perm(jj + usize::from(j1) + perm(kk + usize::from(k1))));
    let g2 =
        perm_mod12(ii + usize::from(i2) + perm(jj + usize::from(j2) + perm(kk + usize::from(k2))));
    let g3 = perm_mod12(ii + 1 + perm(jj + 1 + perm(kk + 1)));

    let n0 = corner(g0, x0, y0, z0);
    let n1 = corner(g1, x1, y1, z1);
    let n2 = corner(g2, x2, y2, z2);
    let n3 = corner(g3, x3, y3, z3);
    (n1 + n0 + n2 + n3) * 32.0
}

/// `0xFDFFE0`: the second permutation, used by the 2D value and gradient noise.
const P2: [u8; 256] = [
    23, 125, 161, 52, 103, 117, 70, 37, 247, 101, 203, 169, 124, 126, 44, 123, 152, 238, 145, 45,
    171, 114, 253, 10, 192, 136, 4, 157, 249, 30, 35, 72, 175, 63, 77, 90, 181, 16, 96, 111, 133,
    104, 75, 162, 93, 56, 66, 240, 8, 50, 84, 229, 49, 210, 173, 239, 141, 1, 87, 18, 2, 198, 143,
    57, 225, 160, 58, 217, 168, 206, 245, 204, 199, 6, 73, 60, 20, 230, 211, 233, 94, 200, 88, 9,
    74, 155, 33, 15, 219, 130, 226, 202, 83, 236, 42, 172, 165, 218, 55, 222, 46, 107, 98, 154,
    109, 67, 196, 178, 127, 158, 13, 243, 65, 79, 166, 248, 25, 224, 115, 80, 68, 51, 184, 128,
    232, 208, 151, 122, 26, 212, 105, 43, 179, 213, 235, 148, 146, 89, 14, 195, 28, 78, 112, 76,
    250, 47, 24, 251, 140, 108, 186, 190, 228, 170, 183, 139, 39, 188, 244, 246, 132, 48, 119, 144,
    180, 138, 134, 193, 82, 182, 120, 121, 86, 220, 209, 3, 91, 241, 149, 85, 205, 150, 113, 216,
    31, 100, 41, 164, 177, 214, 153, 231, 38, 71, 185, 174, 97, 201, 29, 95, 7, 92, 54, 254, 191,
    118, 34, 221, 131, 11, 163, 99, 234, 81, 227, 147, 156, 176, 17, 142, 69, 12, 110, 62, 27, 255,
    0, 194, 59, 116, 242, 252, 19, 21, 187, 53, 207, 129, 64, 135, 61, 40, 167, 237, 102, 223, 106,
    159, 197, 189, 215, 137, 36, 32, 22, 5,
];

/// `0x11D2960`: the eight 2D gradients of `perlin2`.
const GRAD2: [[f32; 2]; 8] = [
    [-1.0, 0.0],
    [1.0, 0.0],
    [0.0, -1.0],
    [0.0, 1.0],
    [-1.0, -1.0],
    [-1.0, 1.0],
    [1.0, -1.0],
    [1.0, 1.0],
];

/// Truncate, then step down when that rounded up.
fn floor_i(v: f32) -> i32 {
    let t = v as i32;
    if t as f32 > v { t - 1 } else { t }
}

/// The quintic fade `((6t - 15) t + 10) t^3`.
fn fade(t: f32) -> f32 {
    let mut f = t * 6.0 - 15.0;
    f = f * t + 10.0;
    f * t * t * t
}

fn perm2(i: usize) -> usize {
    usize::from(P2[i & 255])
}

/// 2D value noise (`0x872BE0`) in [-1, 1].
pub fn value2(x: f32, y: f32) -> f32 {
    let (xi, yi) = (floor_i(x), floor_i(y));
    let (fx, fy) = (x - xi as f32, y - yi as f32);
    let (y0, y1) = (yi as usize & 255, (yi + 1) as usize & 255);
    let (a, b) = (perm2(xi as usize & 255), perm2((xi + 1) as usize & 255));
    let wy = fade(fy);
    let side = |base: usize| {
        let v0 = perm2(base + y0) as f32 / 255.0;
        let v1 = perm2(base + y1) as f32 / 255.0;
        (v1 - v0) * wy + v0
    };
    let (l0, l1) = (side(a), side(b));
    ((l1 - l0) * fade(fx) + l0 - 0.5) * 2.0
}

/// 2D gradient noise (`0x872A20`) in about [-1, 1].
pub fn perlin2(x: f32, y: f32) -> f32 {
    let (xi, yi) = (floor_i(x), floor_i(y));
    let (fx, fy) = (x - xi as f32, y - yi as f32);
    let (y0, y1) = (yi as usize & 255, (yi + 1) as usize & 255);
    let (a, b) = (perm2(xi as usize & 255), perm2((xi + 1) as usize & 255));
    let grad = |h: usize| GRAD2[perm2(h) & 7];
    let (g00, g01) = (grad(a + y0), grad(a + y1));
    let (g10, g11) = (grad(b + y0), grad(b + y1));
    let (fx1, fy1) = (fx - 1.0, fy - 1.0);
    let d00 = g00[1] * fy + fx * g00[0];
    let d10 = g10[1] * fy + g10[0] * fx1;
    let d01 = g01[1] * fy1 + fx * g01[0];
    let d11 = g11[1] * fy1 + g11[0] * fx1;
    let wy = fade(fy);
    let low = (d01 - d00) * wy + d00;
    let high = (d11 - d10) * wy + d10;
    (high - low) * fade(fx) + low
}

/// Gustavson's gradient for a 2D simplex corner.
fn simplex_grad2(hash: usize, x: f32, y: f32) -> f32 {
    let h = hash & 7;
    let (u, v) = if h < 4 { (x, y) } else { (y, x) };
    let u = if h & 1 != 0 { -u } else { u };
    let v = if h & 2 != 0 { v * -2.0 } else { v * 2.0 };
    v + u
}

/// 2D simplex noise (`0x872D40`) in single precision; its floor is `v > 0 ? trunc : trunc - 1`.
pub fn simplex2(x: f32, y: f32) -> f32 {
    const F2: f32 = 0.366_025_4;
    const G2: f32 = 0.211_324_87;
    let fast = |v: f32| if v > 0.0 { v as i32 } else { v as i32 - 1 };
    let s = (x + y) * F2;
    let (i, j) = (fast(s + x), fast(s + y));
    let t = (i + j) as f32 * G2;
    let (x0, y0) = (x - (i as f32 - t), y - (j as f32 - t));
    let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
    let (x1, y1) = (x0 - i1 as f32 + G2, y0 - j1 as f32 + G2);
    let (x2, y2) = (x0 - 1.0 + 0.422_649_74, y0 - 1.0 + 0.422_649_74);
    let (ii, jj) = (i as usize & 255, j as usize & 255);
    let permb = |k: usize| usize::from(P[k & 255]);
    let corner = |t: f32, hash: usize, dx: f32, dy: f32| {
        if t >= 0.0 {
            let t = t * t;
            simplex_grad2(hash, dx, dy) * (t * t)
        } else {
            0.0
        }
    };
    let n0 = corner(0.5 - x0 * x0 - y0 * y0, permb(permb(jj) + ii), x0, y0);
    let n1 = corner(
        0.5 - x1 * x1 - y1 * y1,
        permb(permb(jj + j1) + ii + i1),
        x1,
        y1,
    );
    let n2 = corner(
        0.5 - x2 * x2 - y2 * y2,
        permb(permb(jj + 1) + ii + 1),
        x2,
        y2,
    );
    (n1 + n0 + n2) * 40.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_a_permutation_starting_like_perlins() {
        let mut seen = [false; 256];
        for &v in &P {
            seen[usize::from(v)] = true;
        }
        assert!(seen.iter().all(|&s| s));
        assert_eq!(&P[..6], [151, 160, 137, 91, 90, 15]);
        assert_eq!(P[255], 180);
    }

    #[test]
    fn noise_is_bounded_deterministic_and_nearly_continuous() {
        let mut max: f64 = 0.0;
        for n in 0..4000 {
            let (x, y, z) = (
                f64::from(n) * 0.173,
                f64::from(n % 97) * 0.31,
                f64::from(n % 13) * 0.77,
            );
            let v = simplex3(x, y, z);
            assert_eq!(v, simplex3(x, y, z));
            assert!(v.abs() <= 1.05, "{v} at {x},{y},{z}");
            max = max.max(v.abs());
            let near = simplex3(x + 1e-6, y, z);
            assert!(
                (near - v).abs() < 2e-3,
                "jump of {} at {x},{y},{z}",
                near - v
            );
        }
        assert!(max > 0.5, "the range is actually used: {max}");
    }

    #[test]
    fn floor_rounds_toward_negative_infinity() {
        assert_eq!(
            (floor(1.5), floor(-0.5), floor(-2.0), floor(2.0)),
            (1, -1, -2, 2)
        );
    }
}
