//! A retained-mode-free 2D painter: scenes describe each frame as a list of
//! rectangles, images and text, which one instanced pipeline draws in order.
//!
//! Colors are sRGB-encoded and blended without linearization, as browsers
//! do, so fades and antialiasing look the same as in the operator UI.



mod batching;
mod color;
mod draw_list;
mod painter;

pub use color::*;
pub use draw_list::*;
pub use painter::*;
use batching::*;

#[cfg(test)]
mod tests;
