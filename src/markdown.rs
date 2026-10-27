//! Our own small Markdown parser and the egui preview renderer.
//! The parser only feeds the preview; it never writes to the file.

pub mod blocks;
pub mod inlines;
pub mod render;
