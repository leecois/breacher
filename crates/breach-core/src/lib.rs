//! Breach Protocol core: the screen reader, the exact solver and the planner.

pub mod api;
mod clock;
pub mod follow;
pub mod glyph;
pub mod image;
pub mod plan;
pub mod progress;
pub mod reader;
pub mod solver;
mod templates_data;

/// The six tokens the game uses, indexed by code id.
pub const CODES: [&str; 6] = ["1C", "55", "7A", "BD", "E9", "FF"];
