//! Port of Box2D 2.3.1 (zlib license; Copyright (c) 2006-2011 Erin Catto, http://www.box2d.org).
//!
//! Altered source version: this is a Rust translation that keeps the original Real operation order.
//! Noita ships Box2D 2.3.x, so the aim is identical simulation results, not a new design.

pub mod broad_phase;
pub mod collide;
pub mod contact_solver;
pub mod distance;
pub mod math;
pub mod settings;
pub mod shape;
pub mod toi;
pub mod world;
