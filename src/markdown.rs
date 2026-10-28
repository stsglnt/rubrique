//! Our own small Markdown parser and the egui preview renderer.
//! The parser only feeds the preview; it never writes to the file.

pub mod blocks;
pub mod highlight;
pub mod html;
pub mod inlines;
pub mod notes;
pub mod render;
