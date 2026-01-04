// Demo Manager module for RAPS Demo Workflows
//
// This module provides workflow discovery, metadata management, and execution
// orchestration for demo workflows.

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::workflow::{
    WorkflowDefinition, WorkflowDiscovery, WorkflowId, WorkflowMetadata
};

/// Manages demo workflow discovery and organization
pub struct DemoManager {
    /// Path to workflows directory
    workflows_dir: PathBuf,
    /// Workflow discovery instance
    discovery: Option<WorkflowDiscovery>,
    /// Cached workflow metadata
    workflow_metadata: Vec<WorkflowMetadata>,
}

impl DemoManager {
    /// Create a new demo manager instance
    pub fn new() -> Result<Self> {
        tracing::debug!("Initializing demo manager");

        let workflows_dir = PathBuf::from("./workflows");

        Ok(Self {
            workflows_dir,
            discovery: None,
            workflow_metadata: Vec::new(),
        })
    }

    /// Create a demo manager with a custom workflows directory
    pub fn with_workflows_dir<P: Into<PathBuf>>(path: P) -> Result<Self> {
        let workflows_dir = path.into();
        
        Ok(Self {
            workflows_dir,
            discovery: None,
            workflow_metadata: Vec::new(),
        })
    }

    /// Initialize workflow discovery
    pub fn initialize(&mut self) -> Result<()> {
        // Ensure workflows directory exists
        if !self.workflows_dir.exists() {
            std::fs::create_dir_all(&self.workflows_dir)?;
        }

        let mut discovery = WorkflowDiscovery::new(&self.workflows_dir)?;
        self.workflow_metadata = discovery.discover_workflows()?;
        self.discovery = Some(discovery);

        tracing::info!(
            "Demo manager initialized with {} workflows",
            self.workflow_metadata.len()
        );

        Ok(())
    }

    /// Get all discovered workflow metadata
    pub fn get_workflows(&self) -> &[WorkflowMetadata] {
        &self.workflow_metadata
    }

    /// Get a specific workflow definition
    pub fn get_workflow(&self, id: &WorkflowId) -> Option<&WorkflowDefinition> {
        self.discovery.as_ref()?.get_workflow(id)
    }

    /// Refresh workflow discovery
    pub fn refresh(&mut self) -> Result<Vec<WorkflowMetadata>> {
        if let Some(discovery) = &mut self.discovery {
            self.workflow_metadata = discovery.refresh()?;
        } else {
            self.initialize()?;
        }
        Ok(self.workflow_metadata.clone())
    }

    /// Get workflows grouped by category
    pub fn get_workflows_by_category(&self) -> HashMap<String, Vec<&WorkflowMetadata>> {
        let mut grouped: HashMap<String, Vec<&WorkflowMetadata>> = HashMap::new();

        for metadata in &self.workflow_metadata {
            grouped
                .entry(metadata.category.to_string())
                .or_default()
                .push(metadata);
        }

        grouped
    }
}

impl Default for DemoManager {
    fn default() -> Self {
        Self::new().expect("Failed to create default DemoManager")
    }
}
