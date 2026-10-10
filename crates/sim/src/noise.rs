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
