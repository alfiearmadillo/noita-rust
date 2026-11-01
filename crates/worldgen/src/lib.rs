//! World generation. The reference builds each biome from a herringbone Wang tile map (see
//! [`wang`]) and then maps its colors to materials, spawn functions and pixel scenes.

pub mod biome;
pub mod generate;
pub mod rules;
pub mod terrain;
pub mod validity;
pub mod wang;
pub mod warp;
pub mod world;
