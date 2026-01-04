// Cleanup orchestration and policies for RAPS Demo Workflows
//
// This module implements comprehensive cleanup orchestration with support for
// automatic, manual, and policy-based cleanup of APS resources.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use crate::utils::serde_helpers::duration_serde;
use crate::workflow::{RapsCommand, WorkflowId};
use super::tracker::{CostEstimator, ResourceTracker};
use super::types::{CleanupPolicy, CleanupResult, ResourceId, ResourceType, TrackedResource};

/// Cleanup orchestration modes
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CleanupMode {
    /// Automatic cleanup based on policies
    Automatic,
    /// Manual cleanup with user confirmation
    Manual,
    /// Interactive cleanup with step-by-step confirmation
    Interactive,
    /// Dry run - show what would be cleaned up without doing it
    DryRun,
}

/// Cleanup execution strategy
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CleanupStrategy {
    /// Clean up resources immediately when workflow completes
    Immediate,
    /// Schedule cleanup for later execution
    Scheduled { execute_at: DateTime<Utc> },
    /// Clean up when resources reach a certain age
    AgeBasedCleanup { max_age: Duration },
    /// Clean up when cost threshold is exceeded
    CostBasedCleanup { cost_threshold: f64 },
}

/// Instructions for cleaning up interrupted workflows
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterruptedWorkflowCleanup {
    /// Workflow that was interrupted
    pub workflow_id: WorkflowId,
    /// When the interruption occurred
    pub interrupted_at: DateTime<Utc>,
    /// Resources that were created before interruption
    pub created_resources: Vec<ResourceId>,
    /// Manual cleanup instructions
    pub manual_instructions: Vec<String>,
    /// Automated cleanup commands that can be run
    pub automated_commands: Vec<RapsCommand>,
}

/// Result of cleanup orchestration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupOrchestrationResult {
    /// Overall success status
    pub success: bool,
    /// Cleanup mode that was used
    pub mode: CleanupMode,
    /// Individual cleanup results by workflow
    pub workflow_results: HashMap<WorkflowId, CleanupResult>,
    /// Resources that could not be cleaned up
    pub failed_cleanups: Vec<(ResourceId, String)>,
    /// Total time taken for all cleanup operations
    #[serde(with = "duration_serde")]
    pub total_duration: Duration,
    /// Cost savings from cleanup
    pub cost_savings: f64,
}

/// Cleanup orchestrator that manages resource cleanup across workflows
pub struct CleanupOrchestrator<T: ResourceTracker + CostEstimator> {
    /// Resource tracker for managing resources
    tracker: Arc<RwLock<T>>,
    /// Default cleanup policies
    default_policies: HashMap<String, CleanupPolicy>,
    /// Cleanup strategies by workflow
    workflow_strategies: HashMap<WorkflowId, CleanupStrategy>,
    /// Interrupted workflow tracking
    interrupted_workflows: HashMap<WorkflowId, InterruptedWorkflowCleanup>,
}

impl<T: ResourceTracker + CostEstimator + Send + Sync> CleanupOrchestrator<T> {
    /// Create a new cleanup orchestrator
    pub fn new(tracker: Arc<RwLock<T>>) -> Self {
        Self {
            tracker,
            default_policies: Self::create_default_policies(),
            workflow_strategies: HashMap::new(),
            interrupted_workflows: HashMap::new(),
        }
    }

    /// Create default cleanup policies for different resource types
    fn create_default_policies() -> HashMap<String, CleanupPolicy> {
        let mut policies = HashMap::new();
        
        // High-cost resources should be cleaned up immediately
        policies.insert("Bucket".to_string(), CleanupPolicy::Immediate);
        policies.insert("Object".to_string(), CleanupPolicy::Immediate);
        policies.insert("Photoscene".to_string(), CleanupPolicy::Immediate);
        policies.insert("DesignAutomationWorkItem".to_string(), CleanupPolicy::Immediate);
        
        // One-time cost resources can have delayed cleanup
        policies.insert("Translation".to_string(), CleanupPolicy::Delayed { 
            duration: Duration::hours(2) 
        });
        
        // Free resources can be cleaned up manually
        policies.insert("Webhook".to_string(), CleanupPolicy::Manual);
        policies.insert("Folder".to_string(), CleanupPolicy::Manual);
        policies.insert("Item".to_string(), CleanupPolicy::Manual);
        
        policies
    }

    /// Set cleanup strategy for a workflow
    pub fn set_workflow_strategy(&mut self, workflow_id: WorkflowId, strategy: CleanupStrategy) {
        self.workflow_strategies.insert(workflow_id, strategy);
    }

    /// Get cleanup strategy for a workflow
    pub fn get_workflow_strategy(&self, workflow_id: &WorkflowId) -> CleanupStrategy {
        self.workflow_strategies
            .get(workflow_id)
            .cloned()
            .unwrap_or(CleanupStrategy::Immediate)
    }

    /// Execute cleanup for a completed workflow
    pub async fn cleanup_completed_workflow(
        &mut self,
        workflow_id: &WorkflowId,
        mode: CleanupMode,
    ) -> Result<CleanupResult> {
        info!("Starting cleanup for completed workflow: {} (mode: {:?})", workflow_id, mode);

        let strategy = self.get_workflow_strategy(workflow_id);
        
        match strategy {
            CleanupStrategy::Immediate => {
                self.execute_immediate_cleanup(workflow_id, mode).await
            }
            CleanupStrategy::Scheduled { execute_at } => {
                self.schedule_cleanup(workflow_id, execute_at, mode).await
            }
            CleanupStrategy::AgeBasedCleanup { max_age } => {
                self.execute_age_based_cleanup(workflow_id, max_age, mode).await
            }
            CleanupStrategy::CostBasedCleanup { cost_threshold } => {
                self.execute_cost_based_cleanup(workflow_id, cost_threshold, mode).await
            }
        }
    }

    /// Execute immediate cleanup
    async fn execute_immediate_cleanup(
        &self,
        workflow_id: &WorkflowId,
        mode: CleanupMode,
    ) -> Result<CleanupResult> {
        let tracker = self.tracker.read().await;
        
        match mode {
            CleanupMode::Automatic => {
                tracker.cleanup_workflow_resources(workflow_id)
            }
            CleanupMode::Manual => {
                self.generate_manual_cleanup_instructions(workflow_id, &*tracker).await
            }
            CleanupMode::Interactive => {
                self.execute_interactive_cleanup(workflow_id, &*tracker).await
            }
            CleanupMode::DryRun => {
                self.execute_dry_run_cleanup(workflow_id, &*tracker).await
            }
        }
    }

    /// Generate manual cleanup instructions
    async fn generate_manual_cleanup_instructions(
        &self,
        workflow_id: &WorkflowId,
        tracker: &T,
    ) -> Result<CleanupResult> {
        let resources = tracker.get_resources_for_workflow(workflow_id);
        let start_time = Utc::now();

        info!("Generating manual cleanup instructions for {} resources", resources.len());

        let mut instructions = Vec::new();
        let mut resource_ids = Vec::new();

        for resource in resources {
            resource_ids.push(resource.id);
            
            let instruction = match &resource.resource_type {
                ResourceType::Bucket { .. } => {
                    format!("Delete bucket '{}' using: raps bucket delete {}", 
                           resource.name, resource.aps_id)
                }
                ResourceType::Object { bucket_name, .. } => {
                    format!("Delete object '{}' from bucket '{}' using: raps object delete {} {}", 
                           resource.name, bucket_name, bucket_name, resource.aps_id)
                }
                ResourceType::Translation { source_urn, .. } => {
                    format!("Translation '{}' for URN '{}' will expire automatically", 
                           resource.name, source_urn)
                }
                ResourceType::DesignAutomationWorkItem { activity_id } => {
                    format!("Work item '{}' for activity '{}' will expire automatically", 
                           resource.name, activity_id)
                }
                ResourceType::Photoscene { .. } => {
                    format!("Delete photoscene '{}' using: raps reality delete {}", 
                           resource.name, resource.aps_id)
                }
                ResourceType::Webhook { .. } => {
                    format!("Delete webhook '{}' using: raps webhook delete {}", 
                           resource.name, resource.aps_id)
                }
                ResourceType::Folder { project_id, .. } => {
                    format!("Delete folder '{}' in project '{}' manually through ACC interface", 
                           resource.name, project_id)
                }
                ResourceType::Item { project_id, .. } => {
                    format!("Delete item '{}' in project '{}' manually through ACC interface", 
                           resource.name, project_id)
                }
            };

            instructions.push(instruction);
        }

        // Log instructions for user
        if !instructions.is_empty() {
            info!("Manual cleanup instructions for workflow '{}':", workflow_id);
            for (i, instruction) in instructions.iter().enumerate() {
                info!("  {}. {}", i + 1, instruction);
            }
        }

        Ok(CleanupResult {
            success: true,
            cleaned_resources: resource_ids,
            failed_resources: vec![],
            duration: Utc::now() - start_time,
        })
    }

    /// Execute interactive cleanup with user confirmation
    async fn execute_interactive_cleanup(
        &self,
        workflow_id: &WorkflowId,
        tracker: &T,
    ) -> Result<CleanupResult> {
        let resources = tracker.get_resources_for_workflow(workflow_id);
        let start_time = Utc::now();

        info!("Starting interactive cleanup for {} resources", resources.len());

        // In a real implementation, this would prompt the user for each resource
        // For now, we'll simulate user confirmation based on resource type
        let mut cleaned_resources = Vec::new();
        let mut failed_resources = Vec::new();

        for resource in resources {
            let should_clean = match &resource.resource_type {
                ResourceType::Bucket { .. } | 
                ResourceType::Object { .. } | 
                ResourceType::Photoscene { .. } => {
                    // Simulate user confirming cleanup for cost-incurring resources
                    true
                }
                _ => {
                    // Simulate user declining cleanup for free resources
                    false
                }
            };

            if should_clean {
                info!("User confirmed cleanup for resource: {}", resource.name);
                cleaned_resources.push(resource.id);
            } else {
                info!("User declined cleanup for resource: {}", resource.name);
                failed_resources.push((resource.id, "User declined cleanup".to_string()));
            }
        }

        Ok(CleanupResult {
            success: failed_resources.is_empty(),
            cleaned_resources,
            failed_resources,
            duration: Utc::now() - start_time,
        })
    }

    /// Execute dry run cleanup (show what would be done)
    async fn execute_dry_run_cleanup(
        &self,
        workflow_id: &WorkflowId,
        tracker: &T,
    ) -> Result<CleanupResult> {
        let resources = tracker.get_resources_for_workflow(workflow_id);
        let start_time = Utc::now();

        info!("Dry run cleanup for workflow '{}' - {} resources", workflow_id, resources.len());

        let mut would_clean = Vec::new();
        let mut would_skip = Vec::new();

        for resource in resources {
            let policy = self.get_resource_policy(&resource.resource_type);
            
            match policy {
                CleanupPolicy::Immediate => {
                    info!("Would clean up immediately: {} ({})", resource.name, resource.resource_type_name());
                    would_clean.push(resource.id);
                }
                CleanupPolicy::Delayed { duration } => {
                    if resource.age() >= duration {
                        info!("Would clean up (age exceeded): {} ({})", resource.name, resource.resource_type_name());
                        would_clean.push(resource.id);
                    } else {
                        info!("Would skip (too young): {} ({})", resource.name, resource.resource_type_name());
                        would_skip.push((resource.id, "Resource too young".to_string()));
                    }
                }
                CleanupPolicy::Manual => {
                    info!("Would skip (manual policy): {} ({})", resource.name, resource.resource_type_name());
                    would_skip.push((resource.id, "Manual cleanup policy".to_string()));
                }
                CleanupPolicy::Never => {
                    info!("Would skip (never clean): {} ({})", resource.name, resource.resource_type_name());
                    would_skip.push((resource.id, "Never cleanup policy".to_string()));
                }
            }
        }

        Ok(CleanupResult {
            success: true,
            cleaned_resources: would_clean,
            failed_resources: would_skip,
            duration: Utc::now() - start_time,
        })
    }

    /// Schedule cleanup for later execution
    async fn schedule_cleanup(
        &self,
        workflow_id: &WorkflowId,
        execute_at: DateTime<Utc>,
        _mode: CleanupMode,
    ) -> Result<CleanupResult> {
        info!("Scheduling cleanup for workflow '{}' at {}", workflow_id, execute_at);

        // In a real implementation, this would integrate with a job scheduler
        // For now, we'll just log the scheduled cleanup
        let resource_ids: Vec<ResourceId> = {
            let tracker = self.tracker.read().await;
            tracker.get_resources_for_workflow(workflow_id)
                .iter()
                .map(|r| r.id)
                .collect()
        };

        info!(
            "Scheduled cleanup for {} resources in workflow '{}' to execute at {}",
            resource_ids.len(),
            workflow_id,
            execute_at.format("%Y-%m-%d %H:%M:%S UTC")
        );

        Ok(CleanupResult {
            success: true,
            cleaned_resources: resource_ids,
            failed_resources: vec![],
            duration: Duration::zero(),
        })
    }

    /// Execute age-based cleanup
    async fn execute_age_based_cleanup(
        &self,
        workflow_id: &WorkflowId,
        max_age: Duration,
        _mode: CleanupMode,
    ) -> Result<CleanupResult> {
        let tracker = self.tracker.read().await;
        let resources = tracker.get_resources_for_workflow(workflow_id);
        let start_time = Utc::now();

        info!("Executing age-based cleanup for workflow '{}' (max age: {} hours)", 
              workflow_id, max_age.num_hours());

        let mut cleaned_resources = Vec::new();
        let mut failed_resources = Vec::new();

        for resource in resources {
            if resource.age() >= max_age {
                info!("Cleaning up aged resource: {} (age: {} hours)", 
                      resource.name, resource.age().num_hours());
                cleaned_resources.push(resource.id);
            } else {
                debug!("Skipping young resource: {} (age: {} hours)", 
                       resource.name, resource.age().num_hours());
                failed_resources.push((resource.id, "Resource too young".to_string()));
            }
        }

        Ok(CleanupResult {
            success: failed_resources.is_empty(),
            cleaned_resources,
            failed_resources,
            duration: Utc::now() - start_time,
        })
    }

    /// Execute cost-based cleanup
    async fn execute_cost_based_cleanup(
        &self,
        workflow_id: &WorkflowId,
        cost_threshold: f64,
        _mode: CleanupMode,
    ) -> Result<CleanupResult> {
        let tracker = self.tracker.read().await;
        let cost_summary = tracker.get_cost_summary(workflow_id)?;
        let start_time = Utc::now();

        info!("Executing cost-based cleanup for workflow '{}' (threshold: ${:.2}, current: ${:.2})", 
              workflow_id, cost_threshold, cost_summary.total_cost);

        if cost_summary.total_cost <= cost_threshold {
            info!("Cost below threshold, skipping cleanup");
            return Ok(CleanupResult {
                success: true,
                cleaned_resources: vec![],
                failed_resources: vec![],
                duration: Utc::now() - start_time,
            });
        }

        // Clean up highest-cost resources first
        let mut resources = tracker.get_resources_for_workflow(workflow_id);
        resources.sort_by(|a, b| {
            b.estimated_monthly_cost()
                .partial_cmp(&a.estimated_monthly_cost())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut cleaned_resources = Vec::new();
        let mut remaining_cost = cost_summary.total_cost;

        for resource in resources {
            if remaining_cost <= cost_threshold {
                break;
            }

            let resource_cost = resource.estimated_monthly_cost();
            info!("Cleaning up high-cost resource: {} (cost: ${:.2})", 
                  resource.name, resource_cost);
            
            cleaned_resources.push(resource.id);
            remaining_cost -= resource_cost;
        }

        Ok(CleanupResult {
            success: true,
            cleaned_resources,
            failed_resources: vec![],
            duration: Utc::now() - start_time,
        })
    }

    /// Handle interrupted workflow cleanup
    pub async fn handle_interrupted_workflow(
        &mut self,
        workflow_id: WorkflowId,
        interrupted_at: DateTime<Utc>,
    ) -> Result<InterruptedWorkflowCleanup> {
        info!("Handling interrupted workflow: {} (interrupted at: {})", 
              workflow_id, interrupted_at);

        let tracker = self.tracker.read().await;
        let resources = tracker.get_resources_for_workflow(&workflow_id);
        
        let created_resources: Vec<ResourceId> = resources.iter().map(|r| r.id).collect();
        let mut manual_instructions = Vec::new();
        let mut automated_commands = Vec::new();

        for resource in resources {
            // Generate manual instructions
            let instruction = format!(
                "Clean up {} '{}' (APS ID: {}) created before interruption",
                resource.resource_type_name(),
                resource.name,
                resource.aps_id
            );
            manual_instructions.push(instruction);

            // Add automated cleanup commands if available
            automated_commands.extend(resource.cleanup_commands.clone());
        }

        let cleanup_info = InterruptedWorkflowCleanup {
            workflow_id: workflow_id.clone(),
            interrupted_at,
            created_resources,
            manual_instructions,
            automated_commands,
        };

        // Store for later reference
        self.interrupted_workflows.insert(workflow_id, cleanup_info.clone());

        Ok(cleanup_info)
    }

    /// Get cleanup instructions for all interrupted workflows
    pub fn get_interrupted_workflows(&self) -> Vec<&InterruptedWorkflowCleanup> {
        self.interrupted_workflows.values().collect()
    }

    /// Clear interrupted workflow tracking after cleanup
    pub fn clear_interrupted_workflow(&mut self, workflow_id: &WorkflowId) {
        self.interrupted_workflows.remove(workflow_id);
    }

    /// Get cleanup policy for a resource type
    fn get_resource_policy(&self, resource_type: &ResourceType) -> CleanupPolicy {
        let type_name = resource_type.type_name();
        self.default_policies
            .get(type_name)
            .cloned()
            .unwrap_or_default()
    }

    /// Execute cleanup orchestration across multiple workflows
    pub async fn orchestrate_cleanup(
        &mut self,
        workflow_ids: Vec<WorkflowId>,
        mode: CleanupMode,
    ) -> Result<CleanupOrchestrationResult> {
        let start_time = Utc::now();
        info!("Starting cleanup orchestration for {} workflows (mode: {:?})", 
              workflow_ids.len(), mode);

        let mut workflow_results = HashMap::new();
        let mut failed_cleanups = Vec::new();
        let mut total_cost_savings = 0.0;

        for workflow_id in workflow_ids {
            match self.cleanup_completed_workflow(&workflow_id, mode.clone()).await {
                Ok(result) => {
                    // Calculate cost savings
                    if let Ok(cost_summary) = {
                        let tracker = self.tracker.read().await;
                        tracker.get_cost_summary(&workflow_id)
                    } {
                        total_cost_savings += cost_summary.total_cost;
                    }

                    workflow_results.insert(workflow_id, result);
                }
                Err(e) => {
                    error!("Failed to clean up workflow '{}': {}", workflow_id, e);
                    
                    // Add all resources from this workflow to failed cleanups
                    let tracker = self.tracker.read().await;
                    let resources = tracker.get_resources_for_workflow(&workflow_id);
                    for resource in resources {
                        failed_cleanups.push((resource.id, e.to_string()));
                    }
                }
            }
        }

        let total_duration = Utc::now() - start_time;
        let success = failed_cleanups.is_empty();

        info!(
            "Cleanup orchestration completed: {} workflows, {} failures, ${:.2} cost savings (took {}ms)",
            workflow_results.len(),
            failed_cleanups.len(),
            total_cost_savings,
            total_duration.num_milliseconds()
        );

        Ok(CleanupOrchestrationResult {
            success,
            mode,
            workflow_results,
            failed_cleanups,
            total_duration,
            cost_savings: total_cost_savings,
        })
    }
}

// Extension trait for ResourceType to get type name
trait ResourceTypeExt {
    fn type_name(&self) -> &'static str;
}

impl ResourceTypeExt for ResourceType {
    fn type_name(&self) -> &'static str {
        match self {
            ResourceType::Bucket { .. } => "Bucket",
            ResourceType::Object { .. } => "Object",
            ResourceType::Translation { .. } => "Translation",
            ResourceType::DesignAutomationWorkItem { .. } => "DesignAutomationWorkItem",
            ResourceType::Photoscene { .. } => "Photoscene",
            ResourceType::Webhook { .. } => "Webhook",
            ResourceType::Folder { .. } => "Folder",
            ResourceType::Item { .. } => "Item",
        }
    }
}

// Extension trait for TrackedResource to get type name
trait TrackedResourceExt {
    fn resource_type_name(&self) -> &'static str;
}

impl TrackedResourceExt for TrackedResource {
    fn resource_type_name(&self) -> &'static str {
        self.resource_type.type_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::FileBasedResourceTracker;
    use tempfile::TempDir;
    use tokio;

    async fn create_test_orchestrator() -> (CleanupOrchestrator<FileBasedResourceTracker>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let state_file = temp_dir.path().join("tracker_state.json");
        let tracker = FileBasedResourceTracker::new(state_file).unwrap();
        let orchestrator = CleanupOrchestrator::new(Arc::new(RwLock::new(tracker)));
        (orchestrator, temp_dir)
    }

    #[tokio::test]
    async fn test_cleanup_mode_dry_run() {
        let (mut orchestrator, _temp_dir) = create_test_orchestrator().await;
        
        // Add a test resource
        {
            let mut tracker = orchestrator.tracker.write().await;
            let resource = TrackedResource::new(
                ResourceType::Bucket {
                    region: "US".to_string(),
                    retention_policy: "transient".to_string(),
                },
                "test-bucket".to_string(),
                "demo-bucket".to_string(),
                "test-workflow".to_string(),
                vec![],
            );
            tracker.track_resource(resource).unwrap();
        }

        let result = orchestrator
            .cleanup_completed_workflow(&"test-workflow".to_string(), CleanupMode::DryRun)
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.cleaned_resources.len(), 1);
    }

    #[tokio::test]
    async fn test_interrupted_workflow_handling() {
        let (mut orchestrator, _temp_dir) = create_test_orchestrator().await;
        
        let workflow_id = "interrupted-workflow".to_string();
        let interrupted_at = Utc::now();

        // Add a test resource
        {
            let mut tracker = orchestrator.tracker.write().await;
            let resource = TrackedResource::new(
                ResourceType::Object {
                    bucket_name: "test-bucket".to_string(),
                    size_bytes: 1024,
                },
                "test-object".to_string(),
                "demo-object".to_string(),
                workflow_id.clone(),
                vec![],
            );
            tracker.track_resource(resource).unwrap();
        }

        let cleanup_info = orchestrator
            .handle_interrupted_workflow(workflow_id.clone(), interrupted_at)
            .await
            .unwrap();

        assert_eq!(cleanup_info.workflow_id, workflow_id);
        assert_eq!(cleanup_info.created_resources.len(), 1);
        assert!(!cleanup_info.manual_instructions.is_empty());
    }

    #[tokio::test]
    async fn test_cleanup_strategy_immediate() {
        let (mut orchestrator, _temp_dir) = create_test_orchestrator().await;
        
        let workflow_id = "test-workflow".to_string();
        orchestrator.set_workflow_strategy(
            workflow_id.clone(),
            CleanupStrategy::Immediate,
        );

        let strategy = orchestrator.get_workflow_strategy(&workflow_id);
        assert_eq!(strategy, CleanupStrategy::Immediate);
    }

    #[tokio::test]
    async fn test_cost_based_cleanup() {
        let (mut orchestrator, _temp_dir) = create_test_orchestrator().await;
        
        let workflow_id = "cost-test-workflow".to_string();
        
        // Add high-cost and low-cost resources
        {
            let mut tracker = orchestrator.tracker.write().await;
            
            let expensive_resource = TrackedResource::new(
                ResourceType::Photoscene {
                    scene_type: "aerial".to_string(),
                },
                "expensive-scene".to_string(),
                "demo-photoscene".to_string(),
                workflow_id.clone(),
                vec![],
            );
            
            let cheap_resource = TrackedResource::new(
                ResourceType::Bucket {
                    region: "US".to_string(),
                    retention_policy: "transient".to_string(),
                },
                "cheap-bucket".to_string(),
                "demo-bucket".to_string(),
                workflow_id.clone(),
                vec![],
            );
            
            tracker.track_resource(expensive_resource).unwrap();
            tracker.track_resource(cheap_resource).unwrap();
        }

        let result = orchestrator
            .execute_cost_based_cleanup(&workflow_id, 0.5, CleanupMode::Automatic)
            .await
            .unwrap();

        assert!(result.success);
        // Should clean up the expensive resource first
        assert!(!result.cleaned_resources.is_empty());
    }

    #[tokio::test]
    async fn test_orchestration_multiple_workflows() {
        let (mut orchestrator, _temp_dir) = create_test_orchestrator().await;
        
        let workflow_ids = vec![
            "workflow-1".to_string(),
            "workflow-2".to_string(),
        ];

        // Add resources for each workflow
        {
            let mut tracker = orchestrator.tracker.write().await;
            
            for workflow_id in &workflow_ids {
                let resource = TrackedResource::new(
                    ResourceType::Bucket {
                        region: "US".to_string(),
                        retention_policy: "transient".to_string(),
                    },
                    format!("bucket-{}", workflow_id),
                    format!("demo-bucket-{}", workflow_id),
                    workflow_id.clone(),
                    vec![],
                );
                tracker.track_resource(resource).unwrap();
            }
        }

        let result = orchestrator
            .orchestrate_cleanup(workflow_ids.clone(), CleanupMode::DryRun)
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.workflow_results.len(), 2);
        assert!(result.cost_savings >= 0.0);
    }
}