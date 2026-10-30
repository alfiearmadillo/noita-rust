//! World coordinates to Wang map coordinates (`0x87ce50`). One Wang pixel covers 10x10 world
//! pixels; with a positive strength the lookup position is displaced by simplex, gradient and
//! value noise so cave outlines do not follow the 10-pixel grid.

use sim::noise::{perlin2, simplex2, value2};

/// `floorf` as the exe calls it: `floor` on the widened value, narrowed again (`0x44a390`).
fn floorf(v: f32) -> f32 {
    f64::from(v).floor() as f32
}

/// `(u, v)` in Wang pixels for the world position `(x, y)`.
///
/// `offset` is the pair of globals `0x1206fb0` / `0x12051b0` (random x, y fixed at 7168 by
/// the world generator); `strength <= 0` skips the displacement.
pub fn world_to_wang(offset: (f64, f64), strength: f32, x: f64, y: f64) -> (f32, f32) {
    let big_x = offset.0 + x;
    let big_y = offset.1 + y;
    let u = ((big_x + 0.5) * 0.1) as f32;
    let v = ((big_y + 0.5) * 0.1) as f32;
    if 0.0 >= strength {
        return (u, v);
    }
    let fu = floorf(u) as i32;
    let fv = floorf(v) as i32;
    let k = simplex2((big_x * 0.13715) as f32, (big_y * 0.13717) as f32) * 0.45 + 0.1;
    // 0.1111111119389534 is the f32 nearest 1/9, widened.
    let y9 = (big_y * 0.111_111_111_938_953_4) as f32;
    let x9 = (big_x * 0.111_111_111_938_953_4) as f32;
    let p = perlin2(y9, x9);
    let factor = ((1.0 - k) * 0.33 + 0.111) * strength;
    let displaced_v = p * factor + ((f64::from(v) - f64::from(fv)) as f32);
    let q = value2(x9, y9);
    let displaced_u = q * (k * strength) + ((f64::from(u) - f64::from(fu)) as f32) + fu as f32;
    (displaced_u, fv as f32 + displaced_v)
}
