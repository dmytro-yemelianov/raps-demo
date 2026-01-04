// Workflow discovery and metadata parsing for RAPS Demo Workflows
//
// This module handles discovering workflow definition files, parsing their metadata,
// and resolving dependencies between workflows.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use super::types::*;

/// Workflow definition as stored in YAML files
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    /// Metadata about the workflow
    pub metadata: WorkflowMetadata,
    /// Execution steps
    pub steps: Vec<ExecutionStep>,
    /// Cleanup commands to run after workflow completion
    #[serde(default)]
    pub cleanup: Vec<RapsCommand>,
    /// Dependencies on other workflows (optional)
    #[serde(default)]
    pub dependencies: Option<Vec<WorkflowId>>,
}

/// Result of workflow validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationResult {
    /// Whether the workflow is valid
    pub is_valid: bool,
    /// Validation errors found
    pub errors: Vec<String>,
    /// Validation warnings
    pub warnings: Vec<String>,
}

impl ValidationResult {
    /// Create a successful validation result
    pub fn success() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Create a failed validation result with errors
    pub fn with_errors(errors: Vec<String>) -> Self {
        Self {
            is_valid: false,
            errors,
            warnings: Vec::new(),
        }
    }

    /// Add a warning to the validation result
    pub fn with_warning(mut self, warning: String) -> Self {
        self.warnings.push(warning);
        self
    }
}

/// Workflow discovery and management
pub struct WorkflowDiscovery {
    /// Base directory for workflow definitions
    workflows_dir: PathBuf,
    /// Discovered workflows indexed by ID
    workflows: HashMap<WorkflowId, WorkflowDefinition>,
    /// Dependency graph for workflow resolution
    pub dependency_graph: HashMap<WorkflowId, Vec<WorkflowId>>,
}

impl WorkflowDiscovery {
    /// Create a new workflow discovery instance
    pub fn new<P: AsRef<Path>>(workflows_dir: P) -> Result<Self> {
        let workflows_dir = workflows_dir.as_ref().to_path_buf();

        if !workflows_dir.exists() {
            return Err(anyhow::anyhow!(
                "Workflows directory does not exist: {}",
                workflows_dir.display()
            ));
        }

        let mut discovery = Self {
            workflows_dir,
            workflows: HashMap::new(),
            dependency_graph: HashMap::new(),
        };

        discovery.discover_workflows()?;

        Ok(discovery)
    }

    /// Discover all workflow definition files
    pub fn discover_workflows(&mut self) -> Result<Vec<WorkflowMetadata>> {
        tracing::info!("Discovering workflows in {}", self.workflows_dir.display());

        self.workflows.clear();
        let mut discovered_metadata = Vec::new();

        // Walk through the workflows directory looking for YAML files
        for entry in WalkDir::new(&self.workflows_dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Only process YAML files
            if path.is_file()
                && (path.extension().map_or(false, |ext| ext == "yaml")
                    || path.extension().map_or(false, |ext| ext == "yml"))
            {
                match self.load_workflow_definition(path) {
                    Ok(mut definition) => {
                        // Set the script path in metadata
                        definition.metadata.script_path = path.to_path_buf();

                        let workflow_id = definition.metadata.id.clone();
                        tracing::debug!("Discovered workflow: {}", workflow_id);

                        discovered_metadata.push(definition.metadata.clone());
                        self.workflows.insert(workflow_id, definition);
                    },
                    Err(e) => {
                        tracing::error!("Failed to load workflow from {}: {:?}", path.display(), e);
                        eprintln!("ERROR loading workflow {}: {:?}", path.display(), e);
                    },
                }
            }
        }

        // Build dependency graph after all workflows are loaded
        self.build_dependency_graph()?;

        tracing::info!("Discovered {} workflows", discovered_metadata.len());
        Ok(discovered_metadata)
    }

    /// Load and parse a workflow definition from a YAML file
    fn load_workflow_definition<P: AsRef<Path>>(&self, path: P) -> Result<WorkflowDefinition> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read workflow file: {}", path.display()))?;

        let definition: WorkflowDefinition = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse workflow YAML: {}", path.display()))?;

        Ok(definition)
    }

    /// Validate a workflow definition
    pub fn validate_workflow(&self, workflow_id: &WorkflowId) -> Result<ValidationResult> {
        let workflow = self
            .workflows
            .get(workflow_id)
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;

        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Validate metadata
        if workflow.metadata.id.is_empty() {
            errors.push("Workflow ID cannot be empty".to_string());
        }

        if workflow.metadata.name.is_empty() {
            errors.push("Workflow name cannot be empty".to_string());
        }

        if workflow.metadata.description.is_empty() {
            warnings.push("Workflow description is empty".to_string());
        }

        // Validate steps
        if workflow.steps.is_empty() {
            errors.push("Workflow must have at least one step".to_string());
        }

        let mut step_ids = HashSet::new();
        for step in &workflow.steps {
            if step.id.is_empty() {
                errors.push("Step ID cannot be empty".to_string());
            } else if !step_ids.insert(step.id.clone()) {
                errors.push(format!("Duplicate step ID: {}", step.id));
            }

            if step.name.is_empty() {
                errors.push(format!("Step '{}' name cannot be empty", step.id));
            }

            // Validate command structure
            if let Err(e) = self.validate_command(&step.command) {
                errors.push(format!("Invalid command in step '{}': {}", step.id, e));
            }
        }

        // Validate required assets exist
        for asset_path in &workflow.metadata.required_assets {
            if !asset_path.exists() {
                warnings.push(format!(
                    "Required asset not found: {}",
                    asset_path.display()
                ));
            }
        }

        // Validate dependencies
        if let Some(deps) = &workflow.dependencies {
            for dep_id in deps {
                if !self.workflows.contains_key(dep_id) {
                    errors.push(format!("Dependency workflow not found: {}", dep_id));
                }
            }
        }

        let result = if errors.is_empty() {
            ValidationResult::success()
        } else {
            ValidationResult::with_errors(errors)
        };

        Ok(result.with_warning(warnings.join("; ")))
    }

    /// Validate a RAPS command structure
    fn validate_command(&self, command: &RapsCommand) -> Result<()> {
        match command {
            RapsCommand::Bucket { params, .. } => {
                if params.bucket_name.is_none() {
                    return Err(anyhow::anyhow!("Bucket command requires bucket_name"));
                }
            },
            RapsCommand::Object { params, .. } => {
                if params.bucket_name.is_empty() {
                    return Err(anyhow::anyhow!("Object command requires bucket_name"));
                }
            },
            RapsCommand::Custom { command, .. } => {
                if command.is_empty() {
                    return Err(anyhow::anyhow!("Custom command cannot be empty"));
                }
            },
            _ => {}, // Other commands are valid by structure
        }
        Ok(())
    }

    /// Build dependency graph for workflow resolution
    fn build_dependency_graph(&mut self) -> Result<()> {
        self.dependency_graph.clear();

        for (workflow_id, definition) in &self.workflows {
            let dependencies = definition.dependencies.clone().unwrap_or_default();
            self.dependency_graph
                .insert(workflow_id.clone(), dependencies);
        }

        // Validate no circular dependencies
        for workflow_id in self.workflows.keys() {
            if self.has_circular_dependency(workflow_id, &mut HashSet::new())? {
                return Err(anyhow::anyhow!(
                    "Circular dependency detected involving workflow: {}",
                    workflow_id
                ));
            }
        }

        Ok(())
    }

    /// Check for circular dependencies using DFS
    fn has_circular_dependency(
        &self,
        workflow_id: &WorkflowId,
        visited: &mut HashSet<WorkflowId>,
    ) -> Result<bool> {
        if visited.contains(workflow_id) {
            return Ok(true); // Circular dependency found
        }

        visited.insert(workflow_id.clone());

        if let Some(dependencies) = self.dependency_graph.get(workflow_id) {
            for dep_id in dependencies {
                if self.has_circular_dependency(dep_id, visited)? {
                    return Ok(true);
                }
            }
        }

        visited.remove(workflow_id);
        Ok(false)
    }

    /// Get workflow dependencies in execution order
    pub fn get_workflow_dependencies(&self, workflow_id: &WorkflowId) -> Result<Vec<WorkflowId>> {
        let mut resolved = Vec::new();
        let mut visited = HashSet::new();

        self.resolve_dependencies_recursive(workflow_id, &mut resolved, &mut visited)?;

        Ok(resolved)
    }

    /// Recursively resolve dependencies
    fn resolve_dependencies_recursive(
        &self,
        workflow_id: &WorkflowId,
        resolved: &mut Vec<WorkflowId>,
        visited: &mut HashSet<WorkflowId>,
    ) -> Result<()> {
        if visited.contains(workflow_id) {
            return Ok(()); // Already processed
        }

        visited.insert(workflow_id.clone());

        // First resolve all dependencies
        if let Some(dependencies) = self.dependency_graph.get(workflow_id) {
            for dep_id in dependencies {
                self.resolve_dependencies_recursive(dep_id, resolved, visited)?;
            }
        }

        // Then add this workflow
        resolved.push(workflow_id.clone());
        Ok(())
    }

    /// Get all discovered workflows
    pub fn get_workflows(&self) -> &HashMap<WorkflowId, WorkflowDefinition> {
        &self.workflows
    }

    /// Get a specific workflow definition
    pub fn get_workflow(&self, workflow_id: &WorkflowId) -> Option<&WorkflowDefinition> {
        self.workflows.get(workflow_id)
    }

    /// Get workflows by category
    pub fn get_workflows_by_category(
        &self,
        category: &WorkflowCategory,
    ) -> Vec<&WorkflowDefinition> {
        self.workflows
            .values()
            .filter(|w| w.metadata.category == *category)
            .collect()
    }

    /// Refresh workflow discovery (re-scan directory)
    pub fn refresh(&mut self) -> Result<Vec<WorkflowMetadata>> {
        self.discover_workflows()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_workflow_yaml() -> String {
        r#"
metadata:
  id: "test-workflow"
  name: "Test Workflow"
  description: "A test workflow for unit testing"
  category: "ObjectStorage"
  prerequisites:
    - type: "Authentication"
      description: "Valid APS credentials"
  estimated_duration: 300
  cost_estimate:
    description: "Minimal costs"
    max_cost_usd: 0.10
  required_assets: []

steps:
  - id: "step1"
    name: "Create Bucket"
    description: "Create a test bucket"
    command:
      type: "bucket"
      action: "create"
      bucket_name: "test-bucket"
      retention_policy: "transient"
    expected_duration: 30
    cleanup_commands: []

cleanup:
  - type: "bucket"
    action: "delete"
    bucket_name: "test-bucket"
    force: true
"#
        .to_string()
    }

    #[test]
    fn test_workflow_definition_parsing() {
        let yaml_content = create_test_workflow_yaml();
        let definition: WorkflowDefinition = serde_yaml::from_str(&yaml_content).unwrap();

        assert_eq!(definition.metadata.id, "test-workflow");
        assert_eq!(definition.metadata.name, "Test Workflow");
        assert_eq!(
            definition.metadata.category,
            WorkflowCategory::ObjectStorage
        );
        assert_eq!(definition.steps.len(), 1);
        assert_eq!(definition.cleanup.len(), 1);
    }

    #[test]
    fn test_workflow_discovery() {
        let temp_dir = TempDir::new().unwrap();
        let workflow_file = temp_dir.path().join("test-workflow.yaml");

        fs::write(&workflow_file, create_test_workflow_yaml()).unwrap();

        let mut discovery = WorkflowDiscovery::new(temp_dir.path()).unwrap();
        let metadata_list = discovery.discover_workflows().unwrap();
        let workflows = discovery.get_workflows();

        assert_eq!(workflows.len(), 1);
        assert_eq!(metadata_list.len(), 1);
        assert!(workflows.contains_key("test-workflow"));
    }

    #[test]
    fn test_workflow_validation() {
        let temp_dir = TempDir::new().unwrap();
        let workflow_file = temp_dir.path().join("test-workflow.yaml");

        fs::write(&workflow_file, create_test_workflow_yaml()).unwrap();

        let mut discovery = WorkflowDiscovery::new(temp_dir.path()).unwrap();
        discovery.discover_workflows().unwrap();
        let result = discovery
            .validate_workflow(&"test-workflow".to_string())
            .unwrap();

        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_invalid_workflow_validation() {
        let invalid_yaml = r#"
metadata:
  id: "invalid-workflow"
  name: ""
  description: ""
  category: "ObjectStorage"
  prerequisites: []
  estimated_duration: 300
  required_assets: []

steps: []
cleanup: []
"#;

        let temp_dir = TempDir::new().unwrap();
        let workflow_file = temp_dir.path().join("invalid-workflow.yaml");

        fs::write(&workflow_file, invalid_yaml).unwrap();

        let mut discovery = WorkflowDiscovery::new(temp_dir.path()).unwrap();
        discovery.discover_workflows().unwrap();
        let result = discovery
            .validate_workflow(&"invalid-workflow".to_string())
            .unwrap();

        // Should fail because workflow name is empty and has no steps
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_dependency_resolution() {
        let temp_dir = TempDir::new().unwrap();

        // Create workflow A that depends on B
        let workflow_a = r#"
metadata:
  id: "workflow-a"
  name: "Workflow A"
  description: "Depends on B"
  category: "ObjectStorage"
  prerequisites: []
  estimated_duration: 300
  required_assets: []

dependencies: ["workflow-b"]

steps:
  - id: "step1"
    name: "Step 1"
    description: "Test step"
    command:
      type: "auth"
      action: "status"
    expected_duration: 30
    cleanup_commands: []

cleanup: []
"#;

        // Create workflow B (no dependencies)
        let workflow_b = r#"
metadata:
  id: "workflow-b"
  name: "Workflow B"
  description: "No dependencies"
  category: "ObjectStorage"
  prerequisites: []
  estimated_duration: 300
  required_assets: []

steps:
  - id: "step1"
    name: "Step 1"
    description: "Test step"
    command:
      type: "auth"
      action: "status"
    expected_duration: 30
    cleanup_commands: []

cleanup: []
"#;

        fs::write(temp_dir.path().join("workflow-a.yaml"), workflow_a).unwrap();
        fs::write(temp_dir.path().join("workflow-b.yaml"), workflow_b).unwrap();

        let mut discovery = WorkflowDiscovery::new(temp_dir.path()).unwrap();
        let deps = discovery
            .get_workflow_dependencies(&"workflow-a".to_string())
            .unwrap();

        // Should resolve B first, then A
        assert_eq!(
            deps,
            vec!["workflow-b".to_string(), "workflow-a".to_string()]
        );
    }
}
