//! GUI shape layer: pane geometry + software rendering.
//! egui widgets plug the same rects in a display env (deferred).

pub mod layout;
pub mod render;

// eframe shell (binary target); kept out of docs builds without a display.
#[cfg(not(target_arch = "wasm32"))]
pub mod app;

pub use layout::{layout, Panes, Rect};
