#![deny(clippy::all)]

mod automation;
mod inheritance;
mod color;
mod custom_elements;
mod diff;
mod events;
mod markdown;
mod motion;
mod renderer;
mod retained_tree;
pub mod style;
mod syntax;
mod text;
mod theme;

#[cfg(all(feature = "test-support", target_os = "macos"))]
mod test_renderer;

pub use events::*;
pub use renderer::*;
pub use style::*;
