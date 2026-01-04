// Configuration Manager module for RAPS Demo Workflows
//
// This module handles APS authentication, environment configuration, and
// integration with existing RAPS CLI settings.

pub mod auth;
pub mod manager;
pub mod types;

// Re-export commonly used types
pub use manager::ConfigManager;
pub use types::{RapsConfig, DemoConfig, AuthTokens, Profile, ValidationResult};
