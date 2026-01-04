// Core workflow types and data structures for RAPS Demo Workflows
//
// This module defines the fundamental types used throughout the workflow system,
// including metadata, execution context, and command definitions.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

// Use shared serde helpers
use crate::utils::serde_helpers::{duration_serde, optional_duration_serde};

// Re-export ResourceId from resource module to avoid duplication
pub use crate::resource::ResourceId;

/// Unique identifier for a workflow
pub type WorkflowId = String;

/// Unique identifier for a workflow step
pub type StepId = String;

/// Path to an asset file
pub type AssetPath = PathBuf;

/// Workflow category for organization and filtering
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkflowCategory {
    /// Object Storage Service workflows
    #[serde(alias = "oss", alias = "object-storage")]
    ObjectStorage,
    /// Model Derivative workflows
    #[serde(alias = "model-derivative", alias = "md")]
    ModelDerivative,
    /// Data Management workflows
    #[serde(alias = "data-management", alias = "dm")]
    DataManagement,
    /// Design Automation workflows
    #[serde(alias = "design-automation", alias = "da")]
    DesignAutomation,
    /// Autodesk Construction Cloud workflows
    #[serde(alias = "construction-cloud", alias = "acc")]
    ConstructionCloud,
    /// Reality Capture workflows
    #[serde(alias = "reality-capture", alias = "rc")]
    RealityCapture,
    /// Webhook management workflows
    #[serde(alias = "webhooks")]
    Webhooks,
    /// End-to-end workflows combining multiple services
    #[serde(alias = "end-to-end", alias = "e2e")]
    EndToEnd,
}

impl std::fmt::Display for WorkflowCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkflowCategory::ObjectStorage => write!(f, "Object Storage"),
            WorkflowCategory::ModelDerivative => write!(f, "Model Derivative"),
            WorkflowCategory::DataManagement => write!(f, "Data Management"),
            WorkflowCategory::DesignAutomation => write!(f, "Design Automation"),
            WorkflowCategory::ConstructionCloud => write!(f, "Construction Cloud"),
            WorkflowCategory::RealityCapture => write!(f, "Reality Capture"),
            WorkflowCategory::Webhooks => write!(f, "Webhooks"),
            WorkflowCategory::EndToEnd => write!(f, "End-to-End"),
        }
    }
}

/// Prerequisite type for workflow execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrerequisiteType {
    /// Valid APS authentication required
    #[serde(alias = "authentication", alias = "auth")]
    Authentication,
    /// Specific permissions required
    #[serde(alias = "permissions", alias = "perms")]
    Permissions,
    /// External tool or service required
    #[serde(alias = "external-tool", alias = "tool")]
    ExternalTool,
    /// Specific asset files required
    #[serde(alias = "assets", alias = "files")]
    Assets,
}

/// A prerequisite for workflow execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prerequisite {
    /// Type of prerequisite
    #[serde(rename = "type")]
    pub prerequisite_type: PrerequisiteType,
    /// Human-readable description
    pub description: String,
}

/// Cost estimate for a workflow
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostEstimate {
    /// Description of what costs may be incurred
    pub description: String,
    /// Maximum estimated cost in USD
    pub max_cost_usd: f64,
}

/// Default duration for estimated_duration field
fn default_duration() -> Duration {
    Duration::seconds(0)
}

/// Comprehensive metadata for a workflow
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowMetadata {
    /// Unique identifier for the workflow
    pub id: WorkflowId,
    /// Human-readable name
    pub name: String,
    /// Detailed description of what the workflow demonstrates
    pub description: String,
    /// Category for organization
    pub category: WorkflowCategory,
    /// Prerequisites for execution
    #[serde(default)]
    pub prerequisites: Vec<Prerequisite>,
    /// Estimated duration for completion
    #[serde(with = "duration_serde", default = "default_duration")]
    pub estimated_duration: Duration,
    /// Optional cost estimate
    #[serde(default)]
    pub cost_estimate: Option<CostEstimate>,
    /// Required asset files
    #[serde(default)]
    pub required_assets: Vec<AssetPath>,
    /// Path to the workflow definition file
    #[serde(skip)]
    pub script_path: PathBuf,
}

/// Execution status for workflows and steps
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStatus {
    /// Not yet started
    Pending,
    /// Currently running
    Running,
    /// Paused (interactive mode)
    Paused,
    /// Completed successfully
    Completed,
    /// Failed with error
    Failed,
    /// Cancelled by user
    Cancelled,
}

/// Options for workflow execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionOptions {
    /// Run in interactive mode (pause between steps)
    pub interactive: bool,
    /// Enable verbose logging
    pub verbose: bool,
    /// Automatically clean up resources after completion
    pub auto_cleanup: bool,
    /// Maximum time to wait for completion
    #[serde(with = "duration_serde")]
    pub timeout: Duration,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            interactive: true,
            verbose: false,
            auto_cleanup: true,
            timeout: Duration::minutes(30),
        }
    }
}

/// Context for workflow execution
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Workflow being executed
    pub workflow_id: WorkflowId,
    /// Execution options
    pub options: ExecutionOptions,
    /// Environment variables for the execution
    pub environment: HashMap<String, String>,
    /// Temporary directory for this execution
    pub temp_dir: PathBuf,
    /// Start time of execution
    pub start_time: DateTime<Utc>,
}

/// RAPS CLI command types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum RapsCommand {
    /// Authentication commands
    Auth { action: AuthAction },
    /// Bucket operations
    Bucket {
        action: BucketAction,
        #[serde(flatten)]
        params: BucketParams,
    },
    /// Object operations
    Object {
        action: ObjectAction,
        #[serde(flatten)]
        params: ObjectParams,
    },
    /// Translation operations
    Translate {
        action: TranslateAction,
        #[serde(flatten)]
        params: TranslateParams,
    },
    /// Data Management operations
    DataManagement {
        action: DataMgmtAction,
        #[serde(flatten)]
        params: DataMgmtParams,
    },
    /// Design Automation operations
    DesignAutomation {
        action: DesignAutoAction,
        #[serde(flatten)]
        params: DesignAutoParams,
    },
    /// Custom command with arbitrary arguments
    Custom { command: String, args: Vec<String> },
}

/// Authentication actions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthAction {
    Login,
    Logout,
    Status,
    Refresh,
}

/// Bucket actions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BucketAction {
    Create,
    Delete,
    List,
    Details,
}

/// Bucket operation parameters
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BucketParams {
    pub bucket_name: Option<String>,
    pub retention_policy: Option<String>,
    pub region: Option<String>,
    pub force: Option<bool>,
}

/// Object actions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ObjectAction {
    Upload,
    Download,
    Delete,
    List,
    Details,
    SignedUrl,
}

/// Object operation parameters
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectParams {
    pub bucket_name: String,
    pub object_key: Option<String>,
    pub file_path: Option<PathBuf>,
    pub batch: Option<bool>,
    pub expires_in: Option<u64>,
}

/// Translation actions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TranslateAction {
    Start,
    Status,
    Download,
    Manifest,
}

/// Translation operation parameters
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslateParams {
    pub urn: Option<String>,
    pub format: Option<String>,
    pub output_dir: Option<PathBuf>,
    pub wait: Option<bool>,
}

/// Data Management actions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DataMgmtAction {
    HubList,
    ProjectList,
    FolderList,
    FolderCreate,
    ItemVersions,
    ItemBind,
}

/// Data Management operation parameters
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataMgmtParams {
    pub hub_id: Option<String>,
    pub project_id: Option<String>,
    pub folder_id: Option<String>,
    pub item_id: Option<String>,
    pub folder_name: Option<String>,
}

/// Design Automation actions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DesignAutoAction {
    AppBundles,
    Activities,
    WorkItemRun,
    WorkItemGet,
}

/// Design Automation operation parameters
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignAutoParams {
    pub app_bundle_id: Option<String>,
    pub activity_id: Option<String>,
    pub work_item_id: Option<String>,
    pub input_file: Option<PathBuf>,
    pub output_file: Option<PathBuf>,
}

/// Individual step in a workflow
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionStep {
    /// Unique identifier for the step
    pub id: StepId,
    /// Human-readable name
    pub name: String,
    /// Detailed description of what this step does
    pub description: String,
    /// RAPS command to execute
    pub command: RapsCommand,
    /// Expected duration for this step
    #[serde(with = "optional_duration_serde", default)]
    pub expected_duration: Option<Duration>,
    /// Commands to run for cleanup if this step fails
    #[serde(default)]
    pub cleanup_commands: Vec<RapsCommand>,
}

/// Result of executing a workflow step
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepResult {
    /// Step that was executed
    pub step_id: StepId,
    /// Execution status
    pub status: ExecutionStatus,
    /// Start time
    pub start_time: DateTime<Utc>,
    /// End time (if completed)
    pub end_time: Option<DateTime<Utc>>,
    /// Standard output from the command
    pub stdout: String,
    /// Standard error from the command
    pub stderr: String,
    /// Exit code from the command
    pub exit_code: Option<i32>,
    /// Resources created during this step
    pub created_resources: Vec<ResourceId>,
}

/// Complete workflow execution result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Workflow that was executed
    pub workflow_id: WorkflowId,
    /// Overall success status
    pub success: bool,
    /// Total execution duration
    #[serde(with = "duration_serde")]
    pub duration: Duration,
    /// Number of steps completed
    pub steps_completed: usize,
    /// Total number of steps
    pub total_steps: usize,
    /// All resources created during execution
    pub resources_created: Vec<ResourceId>,
    /// Whether cleanup was performed
    pub cleanup_performed: bool,
    /// Results from individual steps
    pub step_results: Vec<StepResult>,
}

/// Progress information for ongoing execution
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionProgress {
    /// Workflow being executed
    pub workflow_id: WorkflowId,
    /// Current status
    pub status: ExecutionStatus,
    /// Current step being executed
    pub current_step: Option<StepId>,
    /// Steps completed so far
    pub completed_steps: usize,
    /// Total number of steps
    pub total_steps: usize,
    /// Progress percentage (0.0 to 1.0)
    pub progress_percent: f32,
    /// Estimated time remaining
    #[serde(with = "optional_duration_serde")]
    pub estimated_remaining: Option<Duration>,
}

/// Handle for tracking ongoing execution
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExecutionHandle {
    /// Unique identifier for this execution
    pub id: Uuid,
    /// Workflow being executed
    pub workflow_id: WorkflowId,
}

impl ExecutionHandle {
    /// Create a new execution handle
    pub fn new(workflow_id: WorkflowId) -> Self {
        Self {
            id: Uuid::new_v4(),
            workflow_id,
        }
    }
}
