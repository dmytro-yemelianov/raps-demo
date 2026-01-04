// Resource tracking types for RAPS Demo Workflows
//
// This module defines types for tracking APS resources created during demo
// execution, including cleanup policies and cost estimation.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::utils::serde_helpers::duration_serde;
use crate::workflow::{RapsCommand, WorkflowId};

/// Unique identifier for a tracked resource
pub type ResourceId = Uuid;

/// Types of APS resources that can be tracked
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ResourceType {
    /// OSS bucket
    Bucket {
        region: String,
        retention_policy: String,
    },
    /// OSS object
    Object {
        bucket_name: String,
        size_bytes: u64,
    },
    /// Model Derivative translation
    Translation {
        source_urn: String,
        formats: Vec<String>,
    },
    /// Design Automation work item
    DesignAutomationWorkItem {
        activity_id: String,
    },
    /// Reality Capture photoscene
    Photoscene {
        scene_type: String,
    },
    /// Webhook subscription
    Webhook {
        event_type: String,
        callback_url: String,
    },
    /// Data Management folder
    Folder {
        project_id: String,
        parent_folder_id: String,
    },
    /// Data Management item
    Item {
        project_id: String,
        folder_id: String,
    },
}

/// A tracked APS resource
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackedResource {
    /// Unique identifier for tracking
    pub id: ResourceId,
    /// Type and details of the resource
    pub resource_type: ResourceType,
    /// APS identifier for the resource
    pub aps_id: String,
    /// Human-readable name (with demo prefix)
    pub name: String,
    /// When the resource was created
    pub created_at: DateTime<Utc>,
    /// Workflow that created this resource
    pub workflow_id: WorkflowId,
    /// Commands to run for cleanup
    pub cleanup_commands: Vec<RapsCommand>,
    /// Estimated cost for this resource
    pub estimated_cost: Option<f64>,
    /// Tags for additional metadata
    pub tags: HashMap<String, String>,
}

impl TrackedResource {
    /// Create a new tracked resource
    pub fn new(
        resource_type: ResourceType,
        aps_id: String,
        name: String,
        workflow_id: WorkflowId,
        cleanup_commands: Vec<RapsCommand>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            resource_type,
            aps_id,
            name,
            created_at: Utc::now(),
            workflow_id,
            cleanup_commands,
            estimated_cost: None,
            tags: HashMap::new(),
        }
    }

    /// Check if this resource follows demo naming conventions
    pub fn has_demo_naming(&self) -> bool {
        self.name.contains("demo-") || self.name.contains("test-") || self.name.contains("raps-demo-")
    }

    /// Get the age of this resource
    pub fn age(&self) -> Duration {
        Utc::now() - self.created_at
    }

    /// Add a tag to this resource
    pub fn add_tag(&mut self, key: String, value: String) {
        self.tags.insert(key, value);
    }

    /// Get estimated monthly cost for this resource
    pub fn estimated_monthly_cost(&self) -> f64 {
        match &self.resource_type {
            ResourceType::Bucket { .. } => 0.01, // Minimal bucket cost
            ResourceType::Object { size_bytes, .. } => {
                // Rough estimate: $0.023 per GB per month
                (*size_bytes as f64 / 1_073_741_824.0) * 0.023
            }
            ResourceType::Translation { formats, .. } => {
                // Translation costs are one-time, not monthly
                formats.len() as f64 * 0.50
            }
            ResourceType::DesignAutomationWorkItem { .. } => 0.10, // Per work item
            ResourceType::Photoscene { .. } => 1.00, // Per photoscene
            ResourceType::Webhook { .. } => 0.0, // Webhooks are typically free
            ResourceType::Folder { .. } => 0.0, // Folders are free
            ResourceType::Item { .. } => 0.0, // Items are free
        }
    }
}

/// Cleanup policy for resources
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CleanupPolicy {
    /// Clean up immediately after workflow completion
    Immediate,
    /// Clean up after a specified duration
    Delayed { duration: Duration },
    /// Manual cleanup only
    Manual,
    /// Never clean up (for persistent resources)
    Never,
}

impl Default for CleanupPolicy {
    fn default() -> Self {
        CleanupPolicy::Immediate
    }
}

/// Result of a cleanup operation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CleanupResult {
    /// Whether the cleanup was successful
    pub success: bool,
    /// Resources that were successfully cleaned up
    pub cleaned_resources: Vec<ResourceId>,
    /// Resources that failed to clean up
    pub failed_resources: Vec<(ResourceId, String)>,
    /// Total time taken for cleanup
    #[serde(with = "duration_serde")]
    pub duration: Duration,
}

/// Cost summary for a workflow or set of resources
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostSummary {
    /// Total estimated cost
    pub total_cost: f64,
    /// Cost breakdown by resource type
    pub cost_by_type: HashMap<String, f64>,
    /// Cost breakdown by individual resource
    pub cost_by_resource: HashMap<ResourceId, f64>,
    /// Currency (always USD for now)
    pub currency: String,
    /// When this summary was calculated
    pub calculated_at: DateTime<Utc>,
}

impl CostSummary {
    /// Create a new cost summary
    pub fn new() -> Self {
        Self {
            total_cost: 0.0,
            cost_by_type: HashMap::new(),
            cost_by_resource: HashMap::new(),
            currency: "USD".to_string(),
            calculated_at: Utc::now(),
        }
    }

    /// Add a resource to the cost summary
    pub fn add_resource(&mut self, resource: &TrackedResource) {
        let cost = resource.estimated_monthly_cost();
        self.total_cost += cost;

        // Add to type breakdown
        let type_name = match &resource.resource_type {
            ResourceType::Bucket { .. } => "Bucket",
            ResourceType::Object { .. } => "Object",
            ResourceType::Translation { .. } => "Translation",
            ResourceType::DesignAutomationWorkItem { .. } => "Design Automation",
            ResourceType::Photoscene { .. } => "Photoscene",
            ResourceType::Webhook { .. } => "Webhook",
            ResourceType::Folder { .. } => "Folder",
            ResourceType::Item { .. } => "Item",
        };

        *self.cost_by_type.entry(type_name.to_string()).or_insert(0.0) += cost;
        self.cost_by_resource.insert(resource.id, cost);
    }

    /// Check if the total cost exceeds a threshold
    pub fn exceeds_threshold(&self, threshold: f64) -> bool {
        self.total_cost > threshold
    }
}

impl Default for CostSummary {
    fn default() -> Self {
        Self::new()
    }
}

/// Resource naming conventions for demo resources
pub struct ResourceNaming;

impl ResourceNaming {
    /// Generate a demo bucket name
    pub fn demo_bucket_name() -> String {
        let timestamp = Utc::now().timestamp();
        format!("raps-demo-bucket-{}", timestamp)
    }

    /// Generate a demo object key
    pub fn demo_object_key(original_name: &str) -> String {
        let timestamp = Utc::now().timestamp();
        format!("demo-{}-{}", timestamp, original_name)
    }

    /// Generate a demo folder name
    pub fn demo_folder_name(base_name: &str) -> String {
        let timestamp = Utc::now().timestamp();
        format!("RAPS Demo - {} - {}", base_name, timestamp)
    }

    /// Generate a demo photoscene name
    pub fn demo_photoscene_name() -> String {
        let timestamp = Utc::now().timestamp();
        format!("raps-demo-photoscene-{}", timestamp)
    }

    /// Check if a name follows demo conventions
    pub fn is_demo_name(name: &str) -> bool {
        name.contains("demo-") || name.contains("test-") || name.contains("raps-demo-") || name.contains("RAPS Demo")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracked_resource_creation() {
        let resource = TrackedResource::new(
            ResourceType::Bucket {
                region: "US".to_string(),
                retention_policy: "transient".to_string(),
            },
            "test-bucket-123".to_string(),
            "raps-demo-bucket-456".to_string(),
            "test-workflow".to_string(),
            vec![],
        );

        assert!(!resource.id.is_nil());
        assert_eq!(resource.aps_id, "test-bucket-123");
        assert!(resource.has_demo_naming());
    }

    #[test]
    fn test_demo_naming_conventions() {
        assert!(ResourceNaming::is_demo_name("raps-demo-bucket-123"));
        assert!(ResourceNaming::is_demo_name("demo-object-456"));
        assert!(ResourceNaming::is_demo_name("test-resource"));
        assert!(ResourceNaming::is_demo_name("RAPS Demo - Test Folder"));
        assert!(!ResourceNaming::is_demo_name("production-bucket"));
    }

    #[test]
    fn test_cost_estimation() {
        let bucket_resource = TrackedResource::new(
            ResourceType::Bucket {
                region: "US".to_string(),
                retention_policy: "transient".to_string(),
            },
            "bucket-123".to_string(),
            "demo-bucket".to_string(),
            "test-workflow".to_string(),
            vec![],
        );

        let object_resource = TrackedResource::new(
            ResourceType::Object {
                bucket_name: "demo-bucket".to_string(),
                size_bytes: 1_073_741_824, // 1 GB
            },
            "object-456".to_string(),
            "demo-object".to_string(),
            "test-workflow".to_string(),
            vec![],
        );

        assert_eq!(bucket_resource.estimated_monthly_cost(), 0.01);
        assert!((object_resource.estimated_monthly_cost() - 0.023).abs() < 0.001);
    }

    #[test]
    fn test_cost_summary() {
        let mut summary = CostSummary::new();
        
        let resource = TrackedResource::new(
            ResourceType::Bucket {
                region: "US".to_string(),
                retention_policy: "transient".to_string(),
            },
            "bucket-123".to_string(),
            "demo-bucket".to_string(),
            "test-workflow".to_string(),
            vec![],
        );

        summary.add_resource(&resource);
        
        assert_eq!(summary.total_cost, 0.01);
        assert_eq!(summary.cost_by_type.get("Bucket"), Some(&0.01));
        assert!(summary.cost_by_resource.contains_key(&resource.id));
    }

    #[test]
    fn test_cleanup_policy_default() {
        let policy = CleanupPolicy::default();
        assert_eq!(policy, CleanupPolicy::Immediate);
    }
}