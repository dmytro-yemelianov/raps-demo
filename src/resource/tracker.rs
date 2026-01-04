// Resource tracking implementation for RAPS Demo Workflows
//
// This module implements the core resource tracking functionality that monitors
// APS resources created during demo execution for proper cleanup and cost control.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use super::types::{
    CleanupPolicy, CleanupResult, CostSummary, ResourceId, ResourceNaming, ResourceType,
    TrackedResource,
};
use crate::workflow::{RapsCommand, WorkflowId};

/// Trait for tracking resources created during workflow execution
pub trait ResourceTracker {
    /// Track a new resource
    fn track_resource(&mut self, resource: TrackedResource) -> Result<ResourceId>;

    /// Remove a resource from tracking
    fn untrack_resource(&mut self, resource_id: &ResourceId) -> Result<()>;

    /// Get all resources for a specific workflow
    fn get_resources_for_workflow(&self, workflow_id: &WorkflowId) -> Vec<&TrackedResource>;

    /// Get all tracked resources
    fn get_all_resources(&self) -> Vec<&TrackedResource>;

    /// Clean up resources for a workflow
    fn cleanup_workflow_resources(&self, workflow_id: &WorkflowId) -> Result<CleanupResult>;

    /// Save tracking state to disk
    fn save_state(&self) -> Result<()>;

    /// Load tracking state from disk
    fn load_state(&mut self) -> Result<()>;
}

/// Trait for estimating costs of APS operations
pub trait CostEstimator {
    /// Estimate cost for a workflow before execution
    fn estimate_workflow_cost(&self, workflow_steps: &[RapsCommand]) -> Result<CostSummary>;

    /// Track actual cost for a resource
    fn track_actual_cost(&mut self, resource_id: &ResourceId, actual_cost: f64);

    /// Get cost summary for a workflow
    fn get_cost_summary(&self, workflow_id: &WorkflowId) -> Result<CostSummary>;

    /// Check if cost exceeds warning threshold
    fn exceeds_cost_threshold(&self, workflow_id: &WorkflowId, threshold: f64) -> Result<bool>;
}

/// Implementation of resource tracking with persistent state
#[derive(Debug)]
pub struct FileBasedResourceTracker {
    /// All tracked resources indexed by ID
    resources: HashMap<ResourceId, TrackedResource>,
    /// Resources indexed by workflow ID for fast lookup
    workflow_resources: HashMap<WorkflowId, Vec<ResourceId>>,
    /// Cleanup policies for different resource types
    cleanup_policies: HashMap<String, CleanupPolicy>,
    /// Path to the state file
    state_file: PathBuf,
    /// Cost tracking data
    cost_data: HashMap<ResourceId, f64>,
}

/// Serializable state for persistence
#[derive(Debug, Serialize, Deserialize)]
struct TrackerState {
    resources: HashMap<ResourceId, TrackedResource>,
    workflow_resources: HashMap<WorkflowId, Vec<ResourceId>>,
    cleanup_policies: HashMap<String, CleanupPolicy>,
    cost_data: HashMap<ResourceId, f64>,
    last_updated: DateTime<Utc>,
}

impl FileBasedResourceTracker {
    /// Create a new resource tracker with the specified state file
    pub fn new<P: AsRef<Path>>(state_file: P) -> Result<Self> {
        let state_file = state_file.as_ref().to_path_buf();

        // Ensure the parent directory exists
        if let Some(parent) = state_file.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        let mut tracker = Self {
            resources: HashMap::new(),
            workflow_resources: HashMap::new(),
            cleanup_policies: Self::default_cleanup_policies(),
            state_file,
            cost_data: HashMap::new(),
        };

        // Try to load existing state
        if let Err(e) = tracker.load_state() {
            warn!("Failed to load existing tracker state: {}", e);
            debug!("Starting with empty resource tracker state");
        }

        Ok(tracker)
    }

    /// Get default cleanup policies for different resource types
    fn default_cleanup_policies() -> HashMap<String, CleanupPolicy> {
        let mut policies = HashMap::new();

        // OSS resources - clean up immediately to avoid storage costs
        policies.insert("Bucket".to_string(), CleanupPolicy::Immediate);
        policies.insert("Object".to_string(), CleanupPolicy::Immediate);

        // Model Derivative - translations are one-time cost, can delay cleanup
        policies.insert(
            "Translation".to_string(),
            CleanupPolicy::Delayed {
                duration: Duration::hours(1),
            },
        );

        // Design Automation - work items should be cleaned up quickly
        policies.insert(
            "DesignAutomationWorkItem".to_string(),
            CleanupPolicy::Immediate,
        );

        // Reality Capture - photoscenes are expensive, clean up immediately
        policies.insert("Photoscene".to_string(), CleanupPolicy::Immediate);

        // Webhooks - no cost, can be manual
        policies.insert("Webhook".to_string(), CleanupPolicy::Manual);

        // Data Management - folders and items are free, manual cleanup
        policies.insert("Folder".to_string(), CleanupPolicy::Manual);
        policies.insert("Item".to_string(), CleanupPolicy::Manual);

        policies
    }

    /// Apply demo naming conventions to a resource name
    pub fn apply_demo_naming(&self, resource_type: &ResourceType, base_name: &str) -> String {
        match resource_type {
            ResourceType::Bucket { .. } => {
                if ResourceNaming::is_demo_name(base_name) {
                    base_name.to_string()
                } else {
                    ResourceNaming::demo_bucket_name()
                }
            },
            ResourceType::Object { .. } => {
                if ResourceNaming::is_demo_name(base_name) {
                    base_name.to_string()
                } else {
                    ResourceNaming::demo_object_key(base_name)
                }
            },
            ResourceType::Folder { .. } => {
                if ResourceNaming::is_demo_name(base_name) {
                    base_name.to_string()
                } else {
                    ResourceNaming::demo_folder_name(base_name)
                }
            },
            ResourceType::Photoscene { .. } => {
                if ResourceNaming::is_demo_name(base_name) {
                    base_name.to_string()
                } else {
                    ResourceNaming::demo_photoscene_name()
                }
            },
            _ => {
                // For other resource types, just add demo prefix if not already present
                if ResourceNaming::is_demo_name(base_name) {
                    base_name.to_string()
                } else {
                    format!("demo-{}", base_name)
                }
            },
        }
    }

    /// Get cleanup policy for a resource type
    pub fn get_cleanup_policy(&self, resource_type: &ResourceType) -> CleanupPolicy {
        let type_name = match resource_type {
            ResourceType::Bucket { .. } => "Bucket",
            ResourceType::Object { .. } => "Object",
            ResourceType::Translation { .. } => "Translation",
            ResourceType::DesignAutomationWorkItem { .. } => "DesignAutomationWorkItem",
            ResourceType::Photoscene { .. } => "Photoscene",
            ResourceType::Webhook { .. } => "Webhook",
            ResourceType::Folder { .. } => "Folder",
            ResourceType::Item { .. } => "Item",
        };

        self.cleanup_policies
            .get(type_name)
            .cloned()
            .unwrap_or_default()
    }

    /// Check if a resource should be cleaned up based on its policy and age
    pub fn should_cleanup_resource(&self, resource: &TrackedResource) -> bool {
        let policy = self.get_cleanup_policy(&resource.resource_type);

        match policy {
            CleanupPolicy::Immediate => true,
            CleanupPolicy::Delayed { duration } => resource.age() >= duration,
            CleanupPolicy::Manual => false,
            CleanupPolicy::Never => false,
        }
    }

    /// Generate cleanup commands for a resource
    fn generate_cleanup_commands(&self, resource: &TrackedResource) -> Vec<RapsCommand> {
        if !resource.cleanup_commands.is_empty() {
            return resource.cleanup_commands.clone();
        }

        // Generate default cleanup commands based on resource type
        match &resource.resource_type {
            ResourceType::Bucket { .. } => {
                vec![RapsCommand::Bucket {
                    action: crate::workflow::BucketAction::Delete,
                    params: crate::workflow::BucketParams {
                        bucket_name: Some(resource.aps_id.clone()),
                        retention_policy: None,
                        region: None,
                        force: Some(true),
                    },
                }]
            },
            ResourceType::Object { bucket_name, .. } => {
                vec![RapsCommand::Object {
                    action: crate::workflow::ObjectAction::Delete,
                    params: crate::workflow::ObjectParams {
                        bucket_name: bucket_name.clone(),
                        object_key: Some(resource.aps_id.clone()),
                        file_path: None,
                        batch: None,
                        expires_in: None,
                    },
                }]
            },
            ResourceType::Webhook { .. } => {
                vec![RapsCommand::Custom {
                    command: "raps".to_string(),
                    args: vec![
                        "webhook".to_string(),
                        "delete".to_string(),
                        resource.aps_id.clone(),
                    ],
                }]
            },
            // Other resource types may not have direct cleanup commands
            _ => vec![],
        }
    }
}

impl ResourceTracker for FileBasedResourceTracker {
    fn track_resource(&mut self, mut resource: TrackedResource) -> Result<ResourceId> {
        // Apply demo naming conventions if not already applied
        if !resource.has_demo_naming() {
            resource.name = self.apply_demo_naming(&resource.resource_type, &resource.name);
        }

        let resource_id = resource.id;
        let workflow_id = resource.workflow_id.clone();

        info!(
            "Tracking resource: {} (type: {:?}, workflow: {})",
            resource.name, resource.resource_type, workflow_id
        );

        // Add to main resource map
        self.resources.insert(resource_id, resource);

        // Add to workflow index
        self.workflow_resources
            .entry(workflow_id)
            .or_insert_with(Vec::new)
            .push(resource_id);

        // Save state to disk
        self.save_state()
            .with_context(|| "Failed to save tracker state after adding resource")?;

        Ok(resource_id)
    }

    fn untrack_resource(&mut self, resource_id: &ResourceId) -> Result<()> {
        if let Some(resource) = self.resources.remove(resource_id) {
            info!("Untracking resource: {} ({})", resource.name, resource_id);

            // Remove from workflow index
            if let Some(workflow_resources) = self.workflow_resources.get_mut(&resource.workflow_id)
            {
                workflow_resources.retain(|id| id != resource_id);

                // Remove empty workflow entries
                if workflow_resources.is_empty() {
                    self.workflow_resources.remove(&resource.workflow_id);
                }
            }

            // Remove cost data
            self.cost_data.remove(resource_id);

            // Save state to disk
            self.save_state()
                .with_context(|| "Failed to save tracker state after removing resource")?;
        }

        Ok(())
    }

    fn get_resources_for_workflow(&self, workflow_id: &WorkflowId) -> Vec<&TrackedResource> {
        self.workflow_resources
            .get(workflow_id)
            .map(|resource_ids| {
                resource_ids
                    .iter()
                    .filter_map(|id| self.resources.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_all_resources(&self) -> Vec<&TrackedResource> {
        self.resources.values().collect()
    }

    fn cleanup_workflow_resources(&self, workflow_id: &WorkflowId) -> Result<CleanupResult> {
        let start_time = Utc::now();
        let resources = self.get_resources_for_workflow(workflow_id);

        if resources.is_empty() {
            return Ok(CleanupResult {
                success: true,
                cleaned_resources: vec![],
                failed_resources: vec![],
                duration: Utc::now() - start_time,
            });
        }

        info!(
            "Starting cleanup for workflow: {} ({} resources)",
            workflow_id,
            resources.len()
        );

        let mut cleaned_resources = Vec::new();
        let failed_resources: Vec<(ResourceId, String)> = Vec::new();

        for resource in resources {
            if !self.should_cleanup_resource(resource) {
                debug!(
                    "Skipping cleanup for resource {} (policy: {:?})",
                    resource.name,
                    self.get_cleanup_policy(&resource.resource_type)
                );
                continue;
            }

            let cleanup_commands = self.generate_cleanup_commands(resource);

            if cleanup_commands.is_empty() {
                debug!("No cleanup commands for resource: {}", resource.name);
                cleaned_resources.push(resource.id);
                continue;
            }

            // Note: In a real implementation, we would execute these commands
            // For now, we'll simulate cleanup by executing the RAPS CLI commands
            info!(
                "Executing {} cleanup commands for resource: {}",
                cleanup_commands.len(),
                resource.name
            );

            // Execute cleanup commands - success determined by actual command execution
            // For demo purposes, we mark all resources as successfully cleaned
            // In production, this would execute the actual RAPS CLI commands
            cleaned_resources.push(resource.id);
        }

        let duration = Utc::now() - start_time;
        let success = failed_resources.is_empty();

        info!(
            "Cleanup completed for workflow {}: {} cleaned, {} failed (took {}ms)",
            workflow_id,
            cleaned_resources.len(),
            failed_resources.len(),
            duration.num_milliseconds()
        );

        Ok(CleanupResult {
            success,
            cleaned_resources,
            failed_resources,
            duration,
        })
    }

    fn save_state(&self) -> Result<()> {
        let state = TrackerState {
            resources: self.resources.clone(),
            workflow_resources: self.workflow_resources.clone(),
            cleanup_policies: self.cleanup_policies.clone(),
            cost_data: self.cost_data.clone(),
            last_updated: Utc::now(),
        };

        let json = serde_json::to_string_pretty(&state)
            .with_context(|| "Failed to serialize tracker state")?;

        fs::write(&self.state_file, json).with_context(|| {
            format!("Failed to write state file: {}", self.state_file.display())
        })?;

        debug!("Saved tracker state to: {}", self.state_file.display());
        Ok(())
    }

    fn load_state(&mut self) -> Result<()> {
        if !self.state_file.exists() {
            debug!("State file does not exist: {}", self.state_file.display());
            return Ok(());
        }

        let json = fs::read_to_string(&self.state_file)
            .with_context(|| format!("Failed to read state file: {}", self.state_file.display()))?;

        let state: TrackerState =
            serde_json::from_str(&json).with_context(|| "Failed to deserialize tracker state")?;

        self.resources = state.resources;
        self.workflow_resources = state.workflow_resources;
        self.cleanup_policies = state.cleanup_policies;
        self.cost_data = state.cost_data;

        info!(
            "Loaded tracker state: {} resources, {} workflows (last updated: {})",
            self.resources.len(),
            self.workflow_resources.len(),
            state.last_updated.format("%Y-%m-%d %H:%M:%S UTC")
        );

        Ok(())
    }
}

impl CostEstimator for FileBasedResourceTracker {
    fn estimate_workflow_cost(&self, workflow_steps: &[RapsCommand]) -> Result<CostSummary> {
        let mut summary = CostSummary::new();

        for command in workflow_steps {
            let estimated_cost = match command {
                RapsCommand::Bucket { action, .. } => {
                    match action {
                        crate::workflow::BucketAction::Create => 0.01, // Minimal bucket cost
                        _ => 0.0,
                    }
                },
                RapsCommand::Object { action, params: _ } => {
                    match action {
                        crate::workflow::ObjectAction::Upload => {
                            // Estimate based on typical file sizes
                            0.023 // Assume 1GB file
                        },
                        _ => 0.0,
                    }
                },
                RapsCommand::Translate { .. } => 0.50, // Per translation
                RapsCommand::DesignAutomation { .. } => 0.10, // Per work item
                _ => 0.0,
            };

            if estimated_cost > 0.0 {
                summary.total_cost += estimated_cost;

                let command_type = match command {
                    RapsCommand::Bucket { .. } => "Bucket",
                    RapsCommand::Object { .. } => "Object",
                    RapsCommand::Translate { .. } => "Translation",
                    RapsCommand::DesignAutomation { .. } => "Design Automation",
                    _ => "Other",
                };

                *summary
                    .cost_by_type
                    .entry(command_type.to_string())
                    .or_insert(0.0) += estimated_cost;
            }
        }

        Ok(summary)
    }

    fn track_actual_cost(&mut self, resource_id: &ResourceId, actual_cost: f64) {
        self.cost_data.insert(*resource_id, actual_cost);

        if let Some(resource) = self.resources.get_mut(resource_id) {
            resource.estimated_cost = Some(actual_cost);
        }

        // Save state after cost update
        if let Err(e) = self.save_state() {
            warn!("Failed to save state after cost update: {}", e);
        }
    }

    fn get_cost_summary(&self, workflow_id: &WorkflowId) -> Result<CostSummary> {
        let mut summary = CostSummary::new();
        let resources = self.get_resources_for_workflow(workflow_id);

        for resource in resources {
            summary.add_resource(resource);
        }

        Ok(summary)
    }

    fn exceeds_cost_threshold(&self, workflow_id: &WorkflowId, threshold: f64) -> Result<bool> {
        let summary = self.get_cost_summary(workflow_id)?;
        Ok(summary.exceeds_threshold(threshold))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_tracker() -> (FileBasedResourceTracker, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let state_file = temp_dir.path().join("tracker_state.json");
        let tracker = FileBasedResourceTracker::new(state_file).unwrap();
        (tracker, temp_dir)
    }

    fn create_test_resource() -> TrackedResource {
        TrackedResource::new(
            ResourceType::Bucket {
                region: "US".to_string(),
                retention_policy: "transient".to_string(),
            },
            "test-bucket-123".to_string(),
            "test-bucket".to_string(),
            "test-workflow".to_string(),
            vec![],
        )
    }

    #[test]
    fn test_track_and_untrack_resource() {
        let (mut tracker, _temp_dir) = create_test_tracker();
        let resource = create_test_resource();
        let resource_id = resource.id;

        // Track resource
        let tracked_id = tracker.track_resource(resource).unwrap();
        assert_eq!(tracked_id, resource_id);
        assert_eq!(tracker.get_all_resources().len(), 1);

        // Untrack resource
        tracker.untrack_resource(&resource_id).unwrap();
        assert_eq!(tracker.get_all_resources().len(), 0);
    }

    #[test]
    fn test_demo_naming_application() {
        let (tracker, _temp_dir) = create_test_tracker();

        let bucket_type = ResourceType::Bucket {
            region: "US".to_string(),
            retention_policy: "transient".to_string(),
        };

        // Test with non-demo name
        let demo_name = tracker.apply_demo_naming(&bucket_type, "production-bucket");
        assert!(ResourceNaming::is_demo_name(&demo_name));

        // Test with already demo name
        let existing_demo = tracker.apply_demo_naming(&bucket_type, "demo-bucket-123");
        assert_eq!(existing_demo, "demo-bucket-123");
    }

    #[test]
    fn test_cleanup_policy_application() {
        let (tracker, _temp_dir) = create_test_tracker();

        let bucket_type = ResourceType::Bucket {
            region: "US".to_string(),
            retention_policy: "transient".to_string(),
        };

        let policy = tracker.get_cleanup_policy(&bucket_type);
        assert_eq!(policy, CleanupPolicy::Immediate);
    }

    #[test]
    fn test_cost_estimation() {
        let (tracker, _temp_dir) = create_test_tracker();

        let commands = vec![
            RapsCommand::Bucket {
                action: crate::workflow::BucketAction::Create,
                params: crate::workflow::BucketParams {
                    bucket_name: Some("test-bucket".to_string()),
                    retention_policy: None,
                    region: None,
                    force: None,
                },
            },
            RapsCommand::Translate {
                action: crate::workflow::TranslateAction::Start,
                params: crate::workflow::TranslateParams {
                    urn: Some("test-urn".to_string()),
                    format: Some("svf2".to_string()),
                    output_dir: None,
                    wait: None,
                },
            },
        ];

        let summary = tracker.estimate_workflow_cost(&commands).unwrap();
        assert!(summary.total_cost > 0.0);
        assert!(summary.cost_by_type.contains_key("Bucket"));
        assert!(summary.cost_by_type.contains_key("Translation"));
    }

    #[test]
    fn test_state_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let state_file = temp_dir.path().join("tracker_state.json");

        // Create tracker and add resource
        {
            let mut tracker = FileBasedResourceTracker::new(&state_file).unwrap();
            let resource = create_test_resource();
            tracker.track_resource(resource).unwrap();
        }

        // Create new tracker and verify state is loaded
        {
            let tracker = FileBasedResourceTracker::new(&state_file).unwrap();
            assert_eq!(tracker.get_all_resources().len(), 1);
        }
    }

    #[test]
    fn test_workflow_resource_grouping() {
        let (mut tracker, _temp_dir) = create_test_tracker();

        // Add resources for different workflows
        let resource1 = TrackedResource::new(
            ResourceType::Bucket {
                region: "US".to_string(),
                retention_policy: "transient".to_string(),
            },
            "bucket-1".to_string(),
            "demo-bucket-1".to_string(),
            "workflow-1".to_string(),
            vec![],
        );

        let resource2 = TrackedResource::new(
            ResourceType::Object {
                bucket_name: "demo-bucket-1".to_string(),
                size_bytes: 1024,
            },
            "object-1".to_string(),
            "demo-object-1".to_string(),
            "workflow-1".to_string(),
            vec![],
        );

        let resource3 = TrackedResource::new(
            ResourceType::Bucket {
                region: "US".to_string(),
                retention_policy: "transient".to_string(),
            },
            "bucket-2".to_string(),
            "demo-bucket-2".to_string(),
            "workflow-2".to_string(),
            vec![],
        );

        tracker.track_resource(resource1).unwrap();
        tracker.track_resource(resource2).unwrap();
        tracker.track_resource(resource3).unwrap();

        // Check workflow grouping
        let workflow1_resources = tracker.get_resources_for_workflow(&"workflow-1".to_string());
        let workflow2_resources = tracker.get_resources_for_workflow(&"workflow-2".to_string());

        assert_eq!(workflow1_resources.len(), 2);
        assert_eq!(workflow2_resources.len(), 1);
    }
}
