use sim::random::{Random, ScriptRandom};

#[test]
fn zero_state_steps_to_the_modulus() {
    let mut r = Random::zero();
    assert_eq!(r.state(), 0);
    r.int(0, 1);
    assert_eq!(r.state(), 0x7FFF_FFFF);
}

#[test]
fn int_matches_the_hand_computed_draw() {
    // State 1 -> 16807 -> 282475249.
    let mut r = Random::new(0);
    assert_eq!(r.int(0, 999), 0);
    assert_eq!(r.state(), 16_807);
    assert_eq!(r.int(0, 999), 131);
    assert_eq!(r.state(), 282_475_249);
    // A shifted range keeps the same fraction.
    let mut r = Random::new(0);
    r.int(0, 1);
    assert_eq!(r.int(-500, 499), -369);
}

#[test]
fn float_and_unit_use_single_precision() {
    let mut a = Random::new(0);
    let mut b = Random::new(0);
    let unit = a.next_unit_f32();
    assert_eq!(b.float(0.0, 1.0), unit);
    assert_eq!(a.state(), b.state());
    let mut c = Random::new(0);
    c.next_unit();
    assert_eq!(c.state(), 16_807);
}

#[test]
fn set_seed_is_deterministic_and_position_sensitive() {
    let mut a = Random::zero();
    let mut b = Random::zero();
    a.set_seed(12345, 10.0, 20.0);
    b.set_seed(12345, 10.0, 20.0);
    assert_eq!(a, b);
    let state = a.state();
    assert!((1..0x7FFF_FFFF).contains(&state));

    let mut states = std::collections::HashSet::new();
    for x in 0..40 {
        for y in 0..40 {
            let mut r = Random::zero();
            r.set_seed(777, f64::from(x), f64::from(y));
            states.insert(r.state());
        }
    }
    assert!(states.len() > 1500, "only {} distinct states", states.len());

    let mut c = Random::zero();
    c.set_seed(12346, 10.0, 20.0);
    assert_ne!(a, c);
}

#[test]
fn set_seed_survives_extreme_positions() {
    for (x, y) in [
        (1e30, -1e30),
        (f64::NAN, 0.0),
        (0.0, f64::INFINITY),
        (-1e300, 1e300),
    ] {
        let mut r = Random::zero();
        r.set_seed(-5, x, y);
        assert!((1..=0x7FFF_FFFF).contains(&r.state()));
    }
}

#[test]
fn procedural_functions_do_not_touch_the_shared_generator() {
    let mut s = ScriptRandom::new(424242);
    s.set_random_seed(3.0, 4.0);
    let before = s;
    let u = s.procedural_unit(100.0, 200.0);
    assert!(u > 0.0 && u < 1.0);
    assert_eq!(s.procedural_unit(100.0, 200.0), u);
    assert_ne!(s.procedural_unit(101.0, 200.0), u);
    let n = s.procedural_int(5.0, 6.0, 0, 9);
    assert!((0..=9).contains(&n));
    let f = s.procedural_float(5.0, 6.0, -2.0, 2.0);
    assert!((-2.0..=2.0).contains(&f));
    assert!([0, 1].contains(&s.procedural_bit(7.0, 8.0)));
    assert_eq!(s, before);
}

#[test]
fn new_game_plus_changes_the_procedural_stream() {
    let mut s = ScriptRandom::new(99);
    let base = s.procedural_unit(1.0, 2.0);
    s.new_game_plus = 1;
    assert_eq!(s.seed(), 100);
    assert_ne!(s.procedural_unit(1.0, 2.0), base);
    let mut t = ScriptRandom::new(100);
    assert_eq!(t.procedural_unit(1.0, 2.0), s.procedural_unit(1.0, 2.0));
    // set_random_seed + random is the same stream as the equivalent procedural call.
    t.set_random_seed(1.0, 2.0);
    let first = f64::from(t.random() as f32);
    let mut u = Random::zero();
    u.set_seed(100, 1.0, 2.0);
    assert_eq!(first, f64::from(u.next_unit_f32()));
}

#[test]
fn uniform_distribution_falls_back_to_plain_draws() {
    let mut a = Random::new(5);
    let mut b = Random::new(5);
    assert_eq!(a.distribution_int(3, 40, 20, 0.0, 0.005), b.int(3, 40));
    assert_eq!(
        a.distribution_float(-1.0, 1.0, 0.0, 0.0, 0.005),
        b.float(-1.0, 1.0)
    );
    assert_eq!(a, b);
}

#[test]
fn distribution_peaks_at_the_mean() {
    let mean_of = |mean: i32, sharpness: f32| {
        let mut r = Random::new(11);
        let total: i64 = (0..20_000)
            .map(|_| i64::from(r.distribution_int(0, 100, mean, sharpness, 0.005)))
            .sum();
        total as f64 / 20_000.0
    };
    let centered = mean_of(50, 1.0);
    assert!((centered - 50.0).abs() < 2.0, "{centered}");
    let low = mean_of(20, 4.0);
    let high = mean_of(80, 4.0);
    assert!(low < 35.0, "{low}");
    assert!(high > 65.0, "{high}");
    for _ in 0..1000 {
        let mut r = Random::new(3);
        let v = r.distribution_int(-10, 10, 0, 2.0, 0.005);
        assert!((-10..=10).contains(&v));
        let f = r.distribution_float(-1.5, 2.5, 1.0, 3.0, 0.005);
        assert!((-1.5..=2.5).contains(&f));
    }
}

#[test]
fn sharper_distributions_are_narrower() {
    let spread = |sharpness: f32| {
        let mut r = Random::new(21);
        let samples: Vec<f64> = (0..20_000)
            .map(|_| f64::from(r.distribution_unit(0.5, sharpness, 0.0)))
            .collect();
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        samples.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / samples.len() as f64
    };
    assert!(spread(8.0) < spread(1.0));
    assert!(spread(1.0) < spread(0.25));
}
