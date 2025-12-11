//! UI module for PenEnv
//!
//! This module contains all UI components organized into submodules.

pub mod dialogs;
pub mod editor;
pub mod terminal;
pub mod drawer;
pub mod window;

pub use window::build_ui;
