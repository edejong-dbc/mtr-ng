//! User Interface Module
//!
//! This module provides terminal-based user interface components for mtr-ng.

pub mod events;
pub mod state;
pub mod visualization;
pub mod widgets;

// Re-export commonly used types
pub use events::EventHandler;
pub use state::UiState;
pub use visualization::{ColorSupport, VisualizationMode};
pub use widgets::ColumnSelectorState;

// Include the main UI functionality
mod main;
pub use main::*; 