// Workflow Engine module for RAPS Demo Workflows
//
// This module provides the core execution engine for running individual workflow
// scripts with progress tracking and error handling.

pub mod client;
pub mod discovery;
pub mod executor;
pub mod types;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::mpsc;

// Re-export commonly used types
pub use discovery::*;
pub use executor::*;
pub use types::*;

/// High-level workflow engine that coordinates discovery and execution
pub struct WorkflowEngine {
    /// Workflow discovery instance
    discovery: WorkflowDiscovery,
    /// Workflow executor
    executor: Arc<WorkflowExecutor>,
    /// Progress receiver for execution updates
    update_receiver: Option<mpsc::UnboundedReceiver<ExecutionUpdate>>,
}

impl WorkflowEngine {
    /// Create a new workflow engine instance
    pub fn new<P: AsRef<std::path::Path>>(workflows_dir: P) -> Result<Self> {
        tracing::debug!("Initializing workflow engine");

        let discovery = WorkflowDiscovery::new(workflows_dir)?;
        let (executor, receiver) = WorkflowExecutor::new().with_progress_reporting();

        Ok(Self {
            discovery,
            executor: Arc::new(executor),
            update_receiver: Some(receiver),
        })
    }

    /// Get discovered workflows
    pub fn get_workflows(&self) -> &std::collections::HashMap<WorkflowId, WorkflowDefinition> {
        self.discovery.get_workflows()
    }

    /// Get a specific workflow by ID
    pub fn get_workflow(&self, id: &WorkflowId) -> Option<&WorkflowDefinition> {
        self.discovery.get_workflow(id)
    }

    /// Refresh workflow discovery
    pub fn refresh(&mut self) -> Result<Vec<WorkflowMetadata>> {
        self.discovery.refresh()
    }

    /// Execute a workflow by ID
    pub async fn execute(&self, workflow_id: &WorkflowId, options: ExecutionOptions) -> Result<ExecutionHandle> {
        let workflow = self.discovery.get_workflow(workflow_id)
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?
            .clone();

        self.executor.execute_workflow(workflow, options).await
    }

    /// Get the executor for direct access
    pub fn executor(&self) -> &Arc<WorkflowExecutor> {
        &self.executor
    }

    /// Take the update receiver (can only be called once)
    pub fn take_update_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<ExecutionUpdate>> {
        self.update_receiver.take()
    }
}
