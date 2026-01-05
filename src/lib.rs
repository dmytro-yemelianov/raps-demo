//! RAPS Demo Workflows Library
//!
//! This library provides the core functionality for the RAPS Demo Workflows system,
//! including workflow discovery, execution, and resource management.

pub mod assets;
pub mod config;
pub mod demo;
pub mod resource;
pub mod tui;
pub mod utils;
pub mod workflow;

// Re-export main types for convenience
pub use config::ConfigManager;
pub use demo::DemoManager;
pub use resource::ResourceManager;
pub use tui::TuiApp;
pub use workflow::WorkflowEngine;
