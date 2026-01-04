// Resource Manager module for RAPS Demo Workflows
//
// This module tracks and manages APS resources created during demo execution
// for proper cleanup and cost control.

pub mod cleanup;
pub mod tracker;
pub mod types;

use anyhow::Result;
use std::path::PathBuf;

// Re-export commonly used types
pub use tracker::FileBasedResourceTracker;
pub use types::{CleanupPolicy, CleanupResult, ResourceId, ResourceType, TrackedResource};

/// High-level resource manager that coordinates tracking and cleanup
pub struct ResourceManager {
    tracker: FileBasedResourceTracker,
}

impl ResourceManager {
    /// Create a new resource manager instance
    pub fn new() -> Result<Self> {
        tracing::debug!("Initializing resource manager");

        // Use default state file location
        let state_file = Self::default_state_file()?;
        let tracker = FileBasedResourceTracker::new(state_file)?;

        Ok(Self { tracker })
    }

    /// Create a resource manager with a custom state file
    pub fn with_state_file<P: Into<PathBuf>>(state_file: P) -> Result<Self> {
        let tracker = FileBasedResourceTracker::new(state_file.into())?;
        Ok(Self { tracker })
    }

    /// Get the default state file location
    fn default_state_file() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;
        
        let raps_dir = config_dir.join("raps-demo");
        std::fs::create_dir_all(&raps_dir)?;
        
        Ok(raps_dir.join("resource_tracker.json"))
    }

    /// Get access to the underlying tracker
    pub fn tracker(&self) -> &FileBasedResourceTracker {
        &self.tracker
    }

    /// Get mutable access to the underlying tracker
    pub fn tracker_mut(&mut self) -> &mut FileBasedResourceTracker {
        &mut self.tracker
    }
}