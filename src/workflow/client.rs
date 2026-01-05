// RAPS CLI command execution engine for RAPS Demo Workflows
//
// This module provides a client interface for executing RAPS CLI commands as subprocesses,
// parsing their output, and tracking progress during workflow execution.

use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tokio::process::Command as AsyncCommand;
use tokio::time::timeout;
use tracing::{debug, info, warn};

use super::types::*;

/// Configuration for RAPS CLI execution
#[derive(Debug, Clone)]
pub struct RapsClientConfig {
    /// Path to the RAPS CLI binary
    pub raps_binary_path: String,
    /// Default timeout for command execution
    pub default_timeout: Duration,
    /// Whether to capture and parse JSON output
    pub parse_json_output: bool,
    /// Environment variables to pass to RAPS CLI
    pub environment: HashMap<String, String>,
}

impl Default for RapsClientConfig {
    fn default() -> Self {
        Self {
            raps_binary_path: "raps".to_string(),
            default_timeout: Duration::from_secs(300), // 5 minutes
            parse_json_output: true,
            environment: HashMap::new(),
        }
    }
}

/// Result of executing a RAPS CLI command
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// Exit code from the command
    pub exit_code: i32,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Execution duration
    pub duration: Duration,
    /// Parsed JSON output (if available and parsing enabled)
    pub json_output: Option<Value>,
    /// Whether the command was successful (exit code 0)
    pub success: bool,
}

impl CommandResult {
    /// Create a new command result
    pub fn new(exit_code: i32, stdout: String, stderr: String, duration: Duration) -> Self {
        let success = exit_code == 0;
        let json_output = if success && !stdout.trim().is_empty() {
            serde_json::from_str(&stdout).ok()
        } else {
            None
        };

        Self {
            exit_code,
            stdout,
            stderr,
            duration,
            json_output,
            success,
        }
    }

    /// Get a human-readable error message if the command failed
    pub fn error_message(&self) -> Option<String> {
        if self.success {
            return None;
        }

        let mut message = format!("RAPS CLI command failed with exit code {}", self.exit_code);
        
        if !self.stderr.is_empty() {
            message.push_str(&format!("\nError output: {}", self.stderr));
        }
        
        if !self.stdout.is_empty() {
            message.push_str(&format!("\nStandard output: {}", self.stdout));
        }

        Some(message)
    }
}

/// Progress information for long-running commands
#[derive(Debug, Clone)]
pub struct CommandProgress {
    /// Current step or operation being performed
    pub current_operation: String,
    /// Progress percentage (0.0 to 1.0)
    pub progress_percent: f32,
    /// Estimated time remaining
    pub estimated_remaining: Option<Duration>,
    /// Additional status information
    pub status_info: HashMap<String, String>,
}

/// Client for executing RAPS CLI commands
pub struct RapsClient {
    /// Configuration for the client
    config: RapsClientConfig,
    /// Progress callback for long-running operations
    progress_callback: Option<Box<dyn Fn(CommandProgress) + Send + Sync>>,
}

impl RapsClient {
    /// Create a new RAPS client with default configuration
    pub fn new() -> Self {
        Self {
            config: RapsClientConfig::default(),
            progress_callback: None,
        }
    }

    /// Create a new RAPS client with custom configuration
    pub fn with_config(config: RapsClientConfig) -> Self {
        Self {
            config,
            progress_callback: None,
        }
    }

    /// Set a progress callback for long-running operations
    pub fn with_progress_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(CommandProgress) + Send + Sync + 'static,
    {
        self.progress_callback = Some(Box::new(callback));
        self
    }

    /// Execute a RAPS command synchronously
    pub fn execute_command(&self, command: &RapsCommand) -> Result<CommandResult> {
        let args = self.build_command_args(command)?;
        let start_time = Instant::now();

        info!("Executing RAPS command: {} {}", self.config.raps_binary_path, args.join(" "));

        let mut cmd = Command::new(&self.config.raps_binary_path);
        cmd.args(&args)
           .stdout(Stdio::piped())
           .stderr(Stdio::piped());

        // Add environment variables
        for (key, value) in &self.config.environment {
            cmd.env(key, value);
        }

        let output = cmd.output()
            .with_context(|| format!("Failed to execute RAPS CLI: {}", self.config.raps_binary_path))?;

        let duration = start_time.elapsed();
        let result = CommandResult::new(
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
            duration,
        );

        if result.success {
            debug!("RAPS command completed successfully in {:?}", duration);
        } else {
            warn!("RAPS command failed: {}", result.error_message().unwrap_or_default());
        }

        Ok(result)
    }

    /// Execute a RAPS command asynchronously with timeout
    pub async fn execute_command_async(&self, command: &RapsCommand) -> Result<CommandResult> {
        let args = self.build_command_args(command)?;
        let start_time = Instant::now();

        info!("Executing RAPS command async: {} {}", self.config.raps_binary_path, args.join(" "));

        let mut cmd = AsyncCommand::new(&self.config.raps_binary_path);
        cmd.args(&args)
           .stdout(Stdio::piped())
           .stderr(Stdio::piped());

        // Add environment variables
        for (key, value) in &self.config.environment {
            cmd.env(key, value);
        }

        let output = timeout(self.config.default_timeout, cmd.output())
            .await
            .with_context(|| format!("RAPS command timed out after {:?}", self.config.default_timeout))?
            .with_context(|| format!("Failed to execute RAPS CLI: {}", self.config.raps_binary_path))?;

        let duration = start_time.elapsed();
        let result = CommandResult::new(
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
            duration,
        );

        if result.success {
            debug!("RAPS command completed successfully in {:?}", duration);
        } else {
            warn!("RAPS command failed: {}", result.error_message().unwrap_or_default());
        }

        Ok(result)
    }

    /// Execute a command with progress monitoring for long-running operations
    pub async fn execute_with_progress(&self, command: &RapsCommand) -> Result<CommandResult> {
        // For commands that support progress monitoring, we can parse their output
        match command {
            RapsCommand::Translate { action: TranslateAction::Start, .. } => {
                self.execute_translation_with_progress(command).await
            }
            _ => {
                // For other commands, just execute normally
                self.execute_command_async(command).await
            }
        }
    }

    /// Build command line arguments from a RapsCommand
    fn build_command_args(&self, command: &RapsCommand) -> Result<Vec<String>> {
        let mut args = Vec::new();

        match command {
            RapsCommand::Auth { action } => {
                args.push("auth".to_string());
                match action {
                    AuthAction::Login => args.push("login".to_string()),
                    AuthAction::Logout => args.push("logout".to_string()),
                    AuthAction::Status => args.push("status".to_string()),
                    AuthAction::Refresh => args.push("refresh".to_string()),
                }
            }

            RapsCommand::Bucket { action, params } => {
                args.push("bucket".to_string());
                match action {
                    BucketAction::Create => {
                        args.push("create".to_string());
                        if let Some(name) = &params.bucket_name {
                            args.extend(["--key".to_string(), name.clone()]);
                        }
                        if let Some(policy) = &params.retention_policy {
                            args.extend(["--policy".to_string(), policy.clone()]);
                        }
                        if let Some(region) = &params.region {
                            args.extend(["--region".to_string(), region.clone()]);
                        }
                    }
                    BucketAction::Delete => {
                        args.push("delete".to_string());
                        if let Some(name) = &params.bucket_name {
                            args.extend(["--key".to_string(), name.clone()]);
                        }
                        if params.force.unwrap_or(false) {
                            args.push("--yes".to_string());
                        }
                    }
                    BucketAction::List => {
                        args.push("list".to_string());
                    }
                    BucketAction::Details => {
                        args.push("details".to_string());
                        if let Some(name) = &params.bucket_name {
                            args.extend(["--key".to_string(), name.clone()]);
                        }
                    }
                }
            }

            RapsCommand::Object { action, params } => {
                args.push("object".to_string());
                match action {
                    ObjectAction::Upload => {
                        args.push("upload".to_string());
                        args.push(params.bucket_name.clone());
                        if let Some(file_path) = &params.file_path {
                            args.push(file_path.to_string_lossy().to_string());
                        }
                        if let Some(object_key) = &params.object_key {
                            args.extend(["--key".to_string(), object_key.clone()]);
                        }
                        if params.batch.unwrap_or(false) {
                            args.push("--batch".to_string());
                        }
                    }
                    ObjectAction::Download => {
                        args.push("download".to_string());
                        args.push(params.bucket_name.clone());
                        if let Some(object_key) = &params.object_key {
                            args.push(object_key.clone());
                        }
                        if let Some(file_path) = &params.file_path {
                            args.extend(["--output".to_string(), file_path.to_string_lossy().to_string()]);
                        }
                    }
                    ObjectAction::Delete => {
                        args.push("delete".to_string());
                        args.push(params.bucket_name.clone());
                        if let Some(object_key) = &params.object_key {
                            args.push(object_key.clone());
                        }
                    }
                    ObjectAction::List => {
                        args.push("list".to_string());
                        args.push(params.bucket_name.clone());
                    }
                    ObjectAction::Details => {
                        args.push("details".to_string());
                        args.push(params.bucket_name.clone());
                        if let Some(object_key) = &params.object_key {
                            args.push(object_key.clone());
                        }
                    }
                    ObjectAction::SignedUrl => {
                        args.push("signed-url".to_string());
                        args.push(params.bucket_name.clone());
                        if let Some(object_key) = &params.object_key {
                            args.push(object_key.clone());
                        }
                        if let Some(expires_in) = params.expires_in {
                            args.extend(["--expires-in".to_string(), expires_in.to_string()]);
                        }
                    }
                }
            }

            RapsCommand::Translate { action, params } => {
                args.push("translate".to_string());
                match action {
                    TranslateAction::Start => {
                        args.push("start".to_string());
                        if let Some(urn) = &params.urn {
                            args.push(urn.clone());
                        }
                        if let Some(format) = &params.format {
                            args.extend(["--format".to_string(), format.clone()]);
                        }
                        if params.wait.unwrap_or(false) {
                            args.push("--wait".to_string());
                        }
                    }
                    TranslateAction::Status => {
                        args.push("status".to_string());
                        if let Some(urn) = &params.urn {
                            args.push(urn.clone());
                        }
                    }
                    TranslateAction::Download => {
                        args.push("download".to_string());
                        if let Some(urn) = &params.urn {
                            args.push(urn.clone());
                        }
                        if let Some(output_dir) = &params.output_dir {
                            args.extend(["--output".to_string(), output_dir.to_string_lossy().to_string()]);
                        }
                    }
                    TranslateAction::Manifest => {
                        args.push("manifest".to_string());
                        if let Some(urn) = &params.urn {
                            args.push(urn.clone());
                        }
                    }
                }
            }

            RapsCommand::DataManagement { action, params } => {
                match action {
                    DataMgmtAction::HubList => {
                        args.extend(["hub".to_string(), "list".to_string()]);
                    }
                    DataMgmtAction::ProjectList => {
                        args.extend(["project".to_string(), "list".to_string()]);
                        if let Some(hub_id) = &params.hub_id {
                            args.push(hub_id.clone());
                        }
                    }
                    DataMgmtAction::FolderList => {
                        args.extend(["folder".to_string(), "list".to_string()]);
                        if let Some(project_id) = &params.project_id {
                            args.push(project_id.clone());
                        }
                        if let Some(folder_id) = &params.folder_id {
                            args.push(folder_id.clone());
                        }
                    }
                    DataMgmtAction::FolderCreate => {
                        args.extend(["folder".to_string(), "create".to_string()]);
                        if let Some(project_id) = &params.project_id {
                            args.push(project_id.clone());
                        }
                        if let Some(folder_name) = &params.folder_name {
                            args.push(folder_name.clone());
                        }
                    }
                    DataMgmtAction::ItemVersions => {
                        args.extend(["item".to_string(), "versions".to_string()]);
                        if let Some(project_id) = &params.project_id {
                            args.push(project_id.clone());
                        }
                        if let Some(item_id) = &params.item_id {
                            args.push(item_id.clone());
                        }
                    }
                    DataMgmtAction::ItemBind => {
                        args.extend(["item".to_string(), "bind".to_string()]);
                        if let Some(project_id) = &params.project_id {
                            args.push(project_id.clone());
                        }
                        if let Some(item_id) = &params.item_id {
                            args.push(item_id.clone());
                        }
                    }
                }
            }

            RapsCommand::DesignAutomation { action, params } => {
                args.push("da".to_string());
                match action {
                    DesignAutoAction::AppBundles => {
                        args.push("appbundles".to_string());
                        if let Some(app_bundle_id) = &params.app_bundle_id {
                            args.push(app_bundle_id.clone());
                        }
                    }
                    DesignAutoAction::Activities => {
                        args.push("activities".to_string());
                        if let Some(activity_id) = &params.activity_id {
                            args.push(activity_id.clone());
                        }
                    }
                    DesignAutoAction::WorkItemRun => {
                        args.extend(["workitem".to_string(), "run".to_string()]);
                        if let Some(activity_id) = &params.activity_id {
                            args.push(activity_id.clone());
                        }
                        if let Some(input_file) = &params.input_file {
                            args.extend(["--input".to_string(), input_file.to_string_lossy().to_string()]);
                        }
                        if let Some(output_file) = &params.output_file {
                            args.extend(["--output".to_string(), output_file.to_string_lossy().to_string()]);
                        }
                    }
                    DesignAutoAction::WorkItemGet => {
                        args.extend(["workitem".to_string(), "get".to_string()]);
                        if let Some(work_item_id) = &params.work_item_id {
                            args.push(work_item_id.clone());
                        }
                    }
                }
            }

            RapsCommand::Custom { command, args: custom_args } => {
                args.push(command.clone());
                args.extend(custom_args.clone());
            }
        }

        // Add non-interactive flag to prevent prompts when running as subprocess
        args.push("--non-interactive".to_string());

        // Add JSON output flag if enabled (using --output json format)
        if self.config.parse_json_output {
            args.extend(["--output".to_string(), "json".to_string()]);
        }

        Ok(args)
    }

    /// Execute translation command with progress monitoring
    async fn execute_translation_with_progress(&self, command: &RapsCommand) -> Result<CommandResult> {
        // Start the translation
        let result = self.execute_command_async(command).await?;
        
        // If the command included --wait, progress monitoring was handled by RAPS CLI
        // Otherwise, we could implement polling for status updates
        if let Some(callback) = &self.progress_callback {
            let progress = CommandProgress {
                current_operation: "Translation completed".to_string(),
                progress_percent: 1.0,
                estimated_remaining: None,
                status_info: HashMap::new(),
            };
            callback(progress);
        }

        Ok(result)
    }

    /// Validate that RAPS CLI is available and working
    pub fn validate_raps_cli(&self) -> Result<()> {
        let version_command = RapsCommand::Custom {
            command: "--version".to_string(),
            args: vec![],
        };

        let result = self.execute_command(&version_command)?;
        
        if !result.success {
            return Err(anyhow::anyhow!(
                "RAPS CLI validation failed: {}",
                result.error_message().unwrap_or("Unknown error".to_string())
            ));
        }

        info!("RAPS CLI validation successful: {}", result.stdout.trim());
        Ok(())
    }

    /// Check authentication status
    pub fn check_auth_status(&self) -> Result<bool> {
        let auth_command = RapsCommand::Auth {
            action: AuthAction::Status,
        };

        let result = self.execute_command(&auth_command)?;
        Ok(result.success)
    }

    /// Get the current configuration
    pub fn config(&self) -> &RapsClientConfig {
        &self.config
    }
}

impl Default for RapsClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_command_result_creation() {
        let result = CommandResult::new(0, "success".to_string(), "".to_string(), Duration::from_secs(1));
        assert!(result.success);
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "success");
        assert!(result.error_message().is_none());
    }

    #[test]
    fn test_command_result_error() {
        let result = CommandResult::new(1, "".to_string(), "error occurred".to_string(), Duration::from_secs(1));
        assert!(!result.success);
        assert_eq!(result.exit_code, 1);
        assert!(result.error_message().is_some());
        assert!(result.error_message().unwrap().contains("error occurred"));
    }

    #[test]
    fn test_build_auth_command_args() {
        let client = RapsClient::new();
        let command = RapsCommand::Auth {
            action: AuthAction::Status,
        };

        let args = client.build_command_args(&command).unwrap();
        assert_eq!(args, vec!["auth", "status", "--non-interactive", "--output", "json"]);
    }

    #[test]
    fn test_build_bucket_create_command_args() {
        let client = RapsClient::new();
        let command = RapsCommand::Bucket {
            action: BucketAction::Create,
            params: BucketParams {
                bucket_name: Some("test-bucket".to_string()),
                retention_policy: Some("transient".to_string()),
                region: Some("US".to_string()),
                force: None,
            },
        };

        let args = client.build_command_args(&command).unwrap();
        assert_eq!(args, vec![
            "bucket", "create",
            "--key", "test-bucket",
            "--policy", "transient",
            "--region", "US",
            "--non-interactive",
            "--output", "json"
        ]);
    }

    #[test]
    fn test_build_object_upload_command_args() {
        let client = RapsClient::new();
        let command = RapsCommand::Object {
            action: ObjectAction::Upload,
            params: ObjectParams {
                bucket_name: "test-bucket".to_string(),
                object_key: Some("test-file.dwg".to_string()),
                file_path: Some(PathBuf::from("/path/to/file.dwg")),
                batch: Some(false),
                expires_in: None,
            },
        };

        let args = client.build_command_args(&command).unwrap();
        assert_eq!(args, vec![
            "object", "upload", "test-bucket", "/path/to/file.dwg",
            "--key", "test-file.dwg",
            "--non-interactive",
            "--output", "json"
        ]);
    }

    #[test]
    fn test_build_translate_command_args() {
        let client = RapsClient::new();
        let command = RapsCommand::Translate {
            action: TranslateAction::Start,
            params: TranslateParams {
                urn: Some("test-urn".to_string()),
                format: Some("svf2".to_string()),
                output_dir: None,
                wait: Some(true),
            },
        };

        let args = client.build_command_args(&command).unwrap();
        assert_eq!(args, vec![
            "translate", "start", "test-urn",
            "--format", "svf2",
            "--wait",
            "--non-interactive",
            "--output", "json"
        ]);
    }

    #[test]
    fn test_build_custom_command_args() {
        let client = RapsClient::new();
        let command = RapsCommand::Custom {
            command: "custom-command".to_string(),
            args: vec!["arg1".to_string(), "arg2".to_string()],
        };

        let args = client.build_command_args(&command).unwrap();
        assert_eq!(args, vec!["custom-command", "arg1", "arg2", "--non-interactive", "--output", "json"]);
    }

    #[test]
    fn test_raps_client_config_default() {
        let config = RapsClientConfig::default();
        assert_eq!(config.raps_binary_path, "raps");
        assert_eq!(config.default_timeout, Duration::from_secs(300));
        assert!(config.parse_json_output);
        assert!(config.environment.is_empty());
    }
}