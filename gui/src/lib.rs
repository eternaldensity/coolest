//! GUI shape layer: pane geometry + software rendering.
//! egui widgets plug the same rects in a display env (deferred).

pub mod layout;
pub mod render;

pub use layout::{layout, Panes, Rect};
