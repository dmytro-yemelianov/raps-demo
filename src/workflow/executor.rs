// Workflow step execution engine with error handling for RAPS Demo Workflows
//
// This module provides the core execution engine for running workflow steps,
// handling errors, and providing progress reporting and recovery suggestions.

use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info};
use uuid::Uuid;

use super::client::{CommandProgress, CommandResult, RapsClient, RapsClientConfig};
use super::discovery::WorkflowDefinition;
use super::types::*;

/// Execution engine for running workflows step by step
pub struct WorkflowExecutor {
    /// RAPS CLI client for command execution
    raps_client: Arc<RapsClient>,
    /// Active executions indexed by handle
    active_executions: Arc<RwLock<HashMap<ExecutionHandle, ExecutionState>>>,
    /// Progress sender for reporting execution updates
    progress_sender: Option<mpsc::UnboundedSender<ExecutionUpdate>>,
}

/// Internal state for an active execution
#[derive(Debug, Clone)]
struct ExecutionState {
    /// Workflow definition being executed
    workflow: WorkflowDefinition,
    /// Execution context
    context: ExecutionContext,
    /// Current step index
    current_step_index: usize,
    /// Results from completed steps
    completed_steps: Vec<StepResult>,
    /// Resources created during execution
    created_resources: Vec<ResourceId>,
    /// Start time of execution
    start_time: DateTime<Utc>,
    /// Current status
    status: ExecutionStatus,
    /// Generated placeholders (e.g., {uuid}, {timestamp})
    placeholders: HashMap<String, String>,
}

/// Update message for execution progress
#[derive(Debug, Clone)]
pub enum ExecutionUpdate {
    /// Execution started
    Started {
        handle: ExecutionHandle,
        workflow_id: WorkflowId,
    },
    /// Step started
    StepStarted {
        handle: ExecutionHandle,
        step: ExecutionStep,
    },
    /// Step progress update
    StepProgress {
        handle: ExecutionHandle,
        step_id: StepId,
        progress: CommandProgress,
    },
    /// Step completed
    StepCompleted {
        handle: ExecutionHandle,
        result: StepResult,
    },
    /// Execution paused (interactive mode)
    Paused {
        handle: ExecutionHandle,
        next_step: ExecutionStep,
    },
    /// Execution completed
    Completed {
        handle: ExecutionHandle,
        result: ExecutionResult,
    },
    /// Execution failed
    Failed {
        handle: ExecutionHandle,
        error: ExecutionError,
    },
    /// Execution cancelled
    Cancelled { handle: ExecutionHandle },
}

/// Detailed error information for execution failures
#[derive(Debug, Clone)]
pub struct ExecutionError {
    /// Error message
    pub message: String,
    /// Step that failed (if applicable)
    pub failed_step: Option<StepId>,
    /// RAPS CLI command result (if applicable)
    pub command_result: Option<CommandResult>,
    /// Recovery suggestions
    pub recovery_suggestions: Vec<String>,
    /// Whether the error is recoverable
    pub is_recoverable: bool,
}

impl ExecutionError {
    /// Create a new execution error
    pub fn new(message: String) -> Self {
        Self {
            message,
            failed_step: None,
            command_result: None,
            recovery_suggestions: Vec::new(),
            is_recoverable: false,
        }
    }

    /// Create an error from a failed command result
    pub fn from_command_failure(
        step_id: StepId,
        command_result: CommandResult,
        recovery_suggestions: Vec<String>,
    ) -> Self {
        Self {
            message: command_result
                .error_message()
                .unwrap_or("Command failed".to_string()),
            failed_step: Some(step_id),
            command_result: Some(command_result),
            recovery_suggestions,
            is_recoverable: true,
        }
    }

    /// Add a recovery suggestion
    pub fn with_suggestion(mut self, suggestion: String) -> Self {
        self.recovery_suggestions.push(suggestion);
        self
    }

    /// Mark the error as recoverable
    pub fn recoverable(mut self) -> Self {
        self.is_recoverable = true;
        self
    }
}

impl WorkflowExecutor {
    /// Create a new workflow executor
    pub fn new() -> Self {
        let raps_client = Arc::new(RapsClient::new());

        Self {
            raps_client,
            active_executions: Arc::new(RwLock::new(HashMap::new())),
            progress_sender: None,
        }
    }

    /// Create a new workflow executor with custom RAPS client configuration
    pub fn with_config(config: RapsClientConfig) -> Self {
        let raps_client = Arc::new(RapsClient::with_config(config));

        Self {
            raps_client,
            active_executions: Arc::new(RwLock::new(HashMap::new())),
            progress_sender: None,
        }
    }

    /// Set up progress reporting
    pub fn with_progress_reporting(mut self) -> (Self, mpsc::UnboundedReceiver<ExecutionUpdate>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        self.progress_sender = Some(sender);
        (self, receiver)
    }

    /// Validate prerequisites for a workflow
    pub async fn validate_prerequisites(
        &self,
        workflow: &WorkflowDefinition,
    ) -> Result<Vec<String>> {
        let mut validation_errors = Vec::new();

        // Check RAPS CLI availability
        if let Err(e) = self.raps_client.validate_raps_cli() {
            validation_errors.push(format!("RAPS CLI not available: {}", e));
        }

        // Check authentication status
        if !self.raps_client.check_auth_status()? {
            validation_errors
                .push("APS authentication required. Run 'raps auth login' first.".to_string());
        }

        // Check required assets exist
        for asset_path in &workflow.metadata.required_assets {
            if !asset_path.exists() {
                validation_errors.push(format!(
                    "Required asset not found: {}",
                    asset_path.display()
                ));
            }
        }

        Ok(validation_errors)
    }

    /// Start executing a workflow
    pub async fn execute_workflow(
        &self,
        workflow: WorkflowDefinition,
        options: ExecutionOptions,
    ) -> Result<ExecutionHandle> {
        // Validate prerequisites
        let validation_errors = self.validate_prerequisites(&workflow).await?;
        if !validation_errors.is_empty() {
            return Err(anyhow::anyhow!(
                "Prerequisite validation failed:\n{}",
                validation_errors.join("\n")
            ));
        }

        // Create execution context
        let context = ExecutionContext {
            workflow_id: workflow.metadata.id.clone(),
            options,
            environment: HashMap::new(),
            temp_dir: std::env::temp_dir().join(format!("raps-demo-{}", Uuid::new_v4())),
            start_time: Utc::now(),
        };

        // Create execution handle
        let handle = ExecutionHandle::new(workflow.metadata.id.clone());

        // Create execution state
        let execution_state = ExecutionState {
            workflow: workflow.clone(),
            context,
            current_step_index: 0,
            completed_steps: Vec::new(),
            created_resources: Vec::new(),
            start_time: Utc::now(),
            status: ExecutionStatus::Running,
            placeholders: {
                let mut map = HashMap::new();
                map.insert("uuid".to_string(), Uuid::new_v4().to_string());
                map.insert("timestamp".to_string(), Utc::now().timestamp().to_string());
                map
            },
        };

        // Store execution state
        {
            let mut executions = self.active_executions.write().await;
            executions.insert(handle.clone(), execution_state);
        }

        // Send started update
        if let Some(sender) = &self.progress_sender {
            let _ = sender.send(ExecutionUpdate::Started {
                handle: handle.clone(),
                workflow_id: workflow.metadata.id.clone(),
            });
        }

        // Start execution in background
        let executor = self.clone();
        let execution_handle = handle.clone();
        tokio::spawn(async move {
            if let Err(e) = executor
                .run_workflow_execution(execution_handle.clone())
                .await
            {
                error!("Workflow execution failed: {}", e);
                if let Some(sender) = &executor.progress_sender {
                    let _ = sender.send(ExecutionUpdate::Failed {
                        handle: execution_handle,
                        error: ExecutionError::new(e.to_string()),
                    });
                }
            }
        });

        Ok(handle)
    }

    /// Get execution progress for a workflow
    pub async fn get_execution_progress(
        &self,
        handle: &ExecutionHandle,
    ) -> Result<ExecutionProgress> {
        let executions = self.active_executions.read().await;
        let execution_state = executions
            .get(handle)
            .ok_or_else(|| anyhow::anyhow!("Execution not found"))?;

        let current_step =
            if execution_state.current_step_index < execution_state.workflow.steps.len() {
                Some(
                    execution_state.workflow.steps[execution_state.current_step_index]
                        .id
                        .clone(),
                )
            } else {
                None
            };

        let progress_percent = if execution_state.workflow.steps.is_empty() {
            1.0
        } else {
            execution_state.completed_steps.len() as f32
                / execution_state.workflow.steps.len() as f32
        };

        // Estimate remaining time based on completed steps and their durations
        let estimated_remaining = self.estimate_remaining_time(execution_state);

        Ok(ExecutionProgress {
            workflow_id: execution_state.workflow.metadata.id.clone(),
            status: execution_state.status.clone(),
            current_step,
            completed_steps: execution_state.completed_steps.len(),
            total_steps: execution_state.workflow.steps.len(),
            progress_percent,
            estimated_remaining,
        })
    }

    /// Cancel a workflow execution
    pub async fn cancel_execution(&self, handle: &ExecutionHandle) -> Result<()> {
        let mut executions = self.active_executions.write().await;
        if let Some(execution_state) = executions.get_mut(handle) {
            execution_state.status = ExecutionStatus::Cancelled;

            if let Some(sender) = &self.progress_sender {
                let _ = sender.send(ExecutionUpdate::Cancelled {
                    handle: handle.clone(),
                });
            }
        }
        Ok(())
    }

    /// Resume a paused execution (interactive mode)
    pub async fn resume_execution(&self, handle: &ExecutionHandle) -> Result<()> {
        let mut executions = self.active_executions.write().await;
        if let Some(execution_state) = executions.get_mut(handle) {
            if execution_state.status == ExecutionStatus::Paused {
                execution_state.status = ExecutionStatus::Running;

                // Continue execution in background
                let executor = self.clone();
                let execution_handle = handle.clone();
                tokio::spawn(async move {
                    if let Err(e) = executor
                        .run_workflow_execution(execution_handle.clone())
                        .await
                    {
                        error!("Workflow execution failed after resume: {}", e);
                    }
                });
            }
        }
        Ok(())
    }

    /// Run the workflow execution loop
    async fn run_workflow_execution(&self, handle: ExecutionHandle) -> Result<()> {
        loop {
            let (should_continue, next_step) = {
                let executions = self.active_executions.read().await;
                let execution_state = executions
                    .get(&handle)
                    .ok_or_else(|| anyhow::anyhow!("Execution not found"))?;

                match execution_state.status {
                    ExecutionStatus::Cancelled => return Ok(()),
                    ExecutionStatus::Paused => return Ok(()),
                    ExecutionStatus::Completed | ExecutionStatus::Failed => return Ok(()),
                    ExecutionStatus::Running => {
                        if execution_state.current_step_index
                            >= execution_state.workflow.steps.len()
                        {
                            // Workflow completed
                            (false, None)
                        } else {
                            let step = execution_state.workflow.steps
                                [execution_state.current_step_index]
                                .clone();
                            (true, Some(step))
                        }
                    },
                    ExecutionStatus::Pending => (true, None),
                }
            };

            if !should_continue {
                // Complete the workflow
                self.complete_workflow_execution(&handle).await?;
                return Ok(());
            }

            if let Some(step) = next_step {
                // Check if we should pause in interactive mode
                let should_pause = {
                    let executions = self.active_executions.read().await;
                    let execution_state = executions.get(&handle).unwrap();
                    execution_state.context.options.interactive
                        && execution_state.current_step_index > 0
                };

                if should_pause {
                    // Pause for user confirmation
                    {
                        let mut executions = self.active_executions.write().await;
                        let execution_state = executions.get_mut(&handle).unwrap();
                        execution_state.status = ExecutionStatus::Paused;
                    }

                    if let Some(sender) = &self.progress_sender {
                        let _ = sender.send(ExecutionUpdate::Paused {
                            handle: handle.clone(),
                            next_step: step,
                        });
                    }
                    return Ok(());
                }

                // Execute the step
                self.execute_step(&handle, &step).await?;
            }
        }
    }

    /// Execute a single workflow step
    async fn execute_step(&self, handle: &ExecutionHandle, step: &ExecutionStep) -> Result<()> {
        let mut step = step.clone();

        // Resolve placeholders in command
        {
            let mut executions = self.active_executions.write().await;
            if let Some(state) = executions.get_mut(handle) {
                self.resolve_command_placeholders(&mut step.command, &state.placeholders)?;
                for cleanup in &mut step.cleanup_commands {
                    self.resolve_command_placeholders(cleanup, &state.placeholders)?;
                }
            }
        }

        info!("Executing step: {} - {}", step.id, step.name);

        // Send step started update
        if let Some(sender) = &self.progress_sender {
            let _ = sender.send(ExecutionUpdate::StepStarted {
                handle: handle.clone(),
                step: step.clone(),
            });
        }

        let start_time = Utc::now();

        // Execute the RAPS command
        let command_result = self
            .raps_client
            .execute_command_async(&step.command)
            .await?;

        let end_time = Utc::now();
        let _duration = end_time.signed_duration_since(start_time);

        // Create step result
        let step_result = StepResult {
            step_id: step.id.clone(),
            status: if command_result.success {
                ExecutionStatus::Completed
            } else {
                ExecutionStatus::Failed
            },
            start_time,
            end_time: Some(end_time),
            stdout: command_result.stdout.clone(),
            stderr: command_result.stderr.clone(),
            exit_code: Some(command_result.exit_code),
            created_resources: Vec::new(), // TODO: Parse resources from command output
        };

        // Handle command failure
        if !command_result.success {
            let recovery_suggestions =
                self.generate_recovery_suggestions(&step.command, &command_result);
            let error = ExecutionError::from_command_failure(
                step.id.clone(),
                command_result,
                recovery_suggestions,
            );

            // Update execution state to failed
            {
                let mut executions = self.active_executions.write().await;
                if let Some(execution_state) = executions.get_mut(handle) {
                    execution_state.status = ExecutionStatus::Failed;
                    execution_state.completed_steps.push(step_result.clone());
                }
            }

            if let Some(sender) = &self.progress_sender {
                let _ = sender.send(ExecutionUpdate::Failed {
                    handle: handle.clone(),
                    error,
                });
            }

            return Err(anyhow::anyhow!("Step failed: {}", step.id));
        }

        // Update execution state
        {
            let mut executions = self.active_executions.write().await;
            if let Some(execution_state) = executions.get_mut(handle) {
                // Capture JSON outputs into placeholders
                if let Some(json) = &command_result.json_output {
                    self.capture_json_outputs(json, &step.id, &mut execution_state.placeholders);
                }

                execution_state.completed_steps.push(step_result.clone());
                execution_state.current_step_index += 1;
            }
        }

        // Send step completed update
        if let Some(sender) = &self.progress_sender {
            let _ = sender.send(ExecutionUpdate::StepCompleted {
                handle: handle.clone(),
                result: step_result,
            });
        }

        Ok(())
    }

    /// Complete workflow execution
    async fn complete_workflow_execution(&self, handle: &ExecutionHandle) -> Result<()> {
        let execution_result = {
            let mut executions = self.active_executions.write().await;
            let execution_state = executions
                .get_mut(handle)
                .ok_or_else(|| anyhow::anyhow!("Execution not found"))?;

            execution_state.status = ExecutionStatus::Completed;

            let end_time = Utc::now();
            let duration = end_time.signed_duration_since(execution_state.start_time);

            ExecutionResult {
                workflow_id: execution_state.workflow.metadata.id.clone(),
                success: execution_state
                    .completed_steps
                    .iter()
                    .all(|s| s.status == ExecutionStatus::Completed),
                duration: chrono::Duration::from_std(duration.to_std().unwrap_or_default())
                    .unwrap_or_default(),
                steps_completed: execution_state.completed_steps.len(),
                total_steps: execution_state.workflow.steps.len(),
                resources_created: execution_state.created_resources.clone(),
                cleanup_performed: false, // TODO: Implement cleanup
                step_results: execution_state.completed_steps.clone(),
            }
        };

        if let Some(sender) = &self.progress_sender {
            let _ = sender.send(ExecutionUpdate::Completed {
                handle: handle.clone(),
                result: execution_result,
            });
        }

        Ok(())
    }

    /// Resolve placeholders in a RAPS command
    fn resolve_command_placeholders(
        &self,
        command: &mut RapsCommand,
        placeholders: &HashMap<String, String>,
    ) -> Result<()> {
        let json = serde_json::to_value(&command)?;
        let resolved_json = self.resolve_json_placeholders(json, placeholders);
        *command = serde_json::from_value(resolved_json)?;
        Ok(())
    }

    /// Recursively resolve placeholders in JSON value
    fn resolve_json_placeholders(
        &self,
        value: serde_json::Value,
        placeholders: &HashMap<String, String>,
    ) -> serde_json::Value {
        match value {
            serde_json::Value::String(s) => {
                let mut resolved = s;
                for (key, val) in placeholders {
                    let pattern = format!("{{{}}}", key);
                    resolved = resolved.replace(&pattern, val);
                }
                serde_json::Value::String(resolved)
            },
            serde_json::Value::Array(arr) => serde_json::Value::Array(
                arr.into_iter()
                    .map(|v| self.resolve_json_placeholders(v, placeholders))
                    .collect(),
            ),
            serde_json::Value::Object(obj) => serde_json::Value::Object(
                obj.into_iter()
                    .map(|(k, v)| (k, self.resolve_json_placeholders(v, placeholders)))
                    .collect(),
            ),
            _ => value,
        }
    }

    /// Capture outputs from a JSON value into placeholders
    fn capture_json_outputs(
        &self,
        json: &serde_json::Value,
        step_id: &str,
        placeholders: &mut HashMap<String, String>,
    ) {
        if let serde_json::Value::Object(map) = json {
            for (key, val) in map {
                if let Some(s) = val.as_str() {
                    // Store as global (last one wins)
                    placeholders.insert(key.clone(), s.to_string());
                    // Store as step-specific
                    placeholders.insert(format!("{}.{}", step_id, key), s.to_string());
                } else if let Some(n) = val.as_f64() {
                    placeholders.insert(key.clone(), n.to_string());
                    placeholders.insert(format!("{}.{}", step_id, key), n.to_string());
                } else if let Some(i) = val.as_i64() {
                    placeholders.insert(key.clone(), i.to_string());
                    placeholders.insert(format!("{}.{}", step_id, key), i.to_string());
                }
            }
        }
    }

    /// Generate recovery suggestions for failed commands
    fn generate_recovery_suggestions(
        &self,
        command: &RapsCommand,
        result: &CommandResult,
    ) -> Vec<String> {
        let mut suggestions = Vec::new();

        match command {
            RapsCommand::Auth { .. } => {
                suggestions
                    .push("Check your APS credentials and try 'raps auth login'".to_string());
                suggestions.push("Verify your client ID and client secret are correct".to_string());
            },
            RapsCommand::Bucket { .. } => {
                if result.stderr.contains("already exists") {
                    suggestions
                        .push("Bucket name already exists, try a different name".to_string());
                } else if result.stderr.contains("permission") {
                    suggestions
                        .push("Check that you have OSS permissions in your APS app".to_string());
                }
            },
            RapsCommand::Object { .. } => {
                if result.stderr.contains("not found") {
                    suggestions
                        .push("Verify the bucket exists and the object key is correct".to_string());
                } else if result.stderr.contains("file") {
                    suggestions.push("Check that the file path exists and is readable".to_string());
                }
            },
            RapsCommand::Translate { .. } => {
                if result.stderr.contains("urn") {
                    suggestions.push(
                        "Verify the URN is valid and the file was uploaded successfully"
                            .to_string(),
                    );
                } else if result.stderr.contains("format") {
                    suggestions
                        .push("Check that the requested output format is supported".to_string());
                }
            },
            _ => {
                suggestions.push("Check the RAPS CLI documentation for this command".to_string());
                suggestions.push("Verify your APS permissions and authentication".to_string());
            },
        }

        // Add general suggestions
        if result.stderr.contains("network") || result.stderr.contains("timeout") {
            suggestions.push("Check your internet connection and try again".to_string());
        }

        suggestions
    }

    /// Estimate remaining execution time
    fn estimate_remaining_time(
        &self,
        execution_state: &ExecutionState,
    ) -> Option<chrono::Duration> {
        if execution_state.completed_steps.is_empty() {
            return None;
        }

        // Calculate average step duration
        let total_duration: chrono::Duration = execution_state
            .completed_steps
            .iter()
            .filter_map(|step| {
                step.end_time
                    .map(|end| end.signed_duration_since(step.start_time))
            })
            .sum();

        let avg_duration = total_duration / execution_state.completed_steps.len() as i32;
        let remaining_steps =
            execution_state.workflow.steps.len() - execution_state.completed_steps.len();

        Some(avg_duration * remaining_steps as i32)
    }
}

impl Clone for WorkflowExecutor {
    fn clone(&self) -> Self {
        Self {
            raps_client: Arc::clone(&self.raps_client),
            active_executions: Arc::clone(&self.active_executions),
            progress_sender: self.progress_sender.clone(),
        }
    }
}

impl Default for WorkflowExecutor {
    fn default() -> Self {
        Self::new()
    }
}
