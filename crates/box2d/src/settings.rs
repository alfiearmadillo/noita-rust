//! Tuning constants (`b2Settings.h`, Box2D 2.3.1).
//!
//! The C++ constants are `float` literals and expressions evaluated in float; Noita's build then
//! uses them in `double` arithmetic. Each constant is therefore computed in f32 and widened.

use crate::math::Real;

const PI_F: f32 = std::f32::consts::PI;

pub const EPSILON: Real = f32::EPSILON as Real;
pub const FLOAT_MAX: Real = f32::MAX as Real;
pub const PI: Real = PI_F as Real;

pub const MAX_MANIFOLD_POINTS: usize = 2;
pub const MAX_POLYGON_VERTICES: usize = 8;
pub const AABB_EXTENSION: Real = 0.1f32 as Real;
pub const AABB_MULTIPLIER: Real = 2.0;
pub const LINEAR_SLOP: Real = 0.005f32 as Real;
pub const ANGULAR_SLOP: Real = (2.0f32 / 180.0f32 * PI_F) as Real;
pub const POLYGON_RADIUS: Real = (2.0f32 * 0.005f32) as Real;
pub const MAX_SUB_STEPS: i32 = 8;

pub const MAX_TOI_CONTACTS: usize = 32;
/// Noita's cap on TOI events per step (global at 0x11d29cc) and on failed TOIs.
pub const MAX_TOI_EVENTS: usize = 512;
pub const VELOCITY_THRESHOLD: Real = 1.0;
pub const MAX_LINEAR_CORRECTION: Real = 0.2f32 as Real;
pub const MAX_ANGULAR_CORRECTION: Real = (8.0f32 / 180.0f32 * PI_F) as Real;
pub const MAX_TRANSLATION: Real = 2.0;
pub const MAX_TRANSLATION_SQUARED: Real = 4.0;
/// Noita uses double-precision values here: 0.5 * 3.14159265359.
#[allow(clippy::approx_constant)]
pub const MAX_ROTATION: Real = 1.570796326795;
pub const MAX_ROTATION_SQUARED: Real = 2.4674011002726646;
/// Noita's added linear speed cap, in meters per second.
pub const MAX_LINEAR_SPEED: Real = 250.0;
pub const BAUMGARTE: Real = 0.2f32 as Real;
pub const TOI_BAUMGARTE: Real = 0.75;

pub const TIME_TO_SLEEP: Real = 0.5;
pub const LINEAR_SLEEP_TOLERANCE: Real = 0.01f32 as Real;
pub const ANGULAR_SLEEP_TOLERANCE: Real = (2.0f32 / 180.0f32 * PI_F) as Real;
