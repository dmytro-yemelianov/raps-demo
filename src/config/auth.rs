// Authentication validation and setup guidance for RAPS Demo Workflows
//
// This module provides authentication validation against APS APIs and
// guided setup flows for missing credentials.

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::process::Command;

use super::types::{AuthTokens, RapsConfig, ValidationResult};

/// Authentication validation service
#[derive(Debug, Clone)]
pub struct AuthValidator {
    /// Base URL for APS APIs
    base_url: String,
}

impl AuthValidator {
    /// Create a new authentication validator
    pub fn new(base_url: String) -> Self {
        Self { base_url }
    }

    /// Validate credentials against APS APIs
    pub async fn validate_credentials(&self, config: &RapsConfig) -> Result<ValidationResult> {
        let mut result = ValidationResult::new();

        // Check if credentials are present
        if !config.has_credentials() {
            result.add_error("Missing APS credentials (client_id and client_secret)".to_string());
            return Ok(result);
        }

        // Check if tokens are present and valid
        if let Some(tokens) = &config.auth_tokens {
            if tokens.is_expired() {
                result.add_warning("Access token has expired".to_string());
            } else if tokens.expires_within(300) {
                // Expires within 5 minutes
                result.add_warning("Access token expires soon".to_string());
            }

            // Validate token by making a test API call
            match self.validate_token(tokens).await {
                Ok(true) => {
                    tracing::info!("Authentication token validated successfully");
                }
                Ok(false) => {
                    result.add_error("Authentication token is invalid".to_string());
                }
                Err(e) => {
                    result.add_warning(format!("Could not validate token: {}", e));
                }
            }
        } else {
            result.add_error("No authentication tokens found".to_string());
        }

        Ok(result)
    }

    /// Validate a token by making a test API call
    async fn validate_token(&self, tokens: &AuthTokens) -> Result<bool> {
        // Use RAPS CLI to validate the token by making a simple API call
        let output = Command::new("raps")
            .args(&["auth", "status"])
            .env("APS_ACCESS_TOKEN", &tokens.access_token)
            .output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    Ok(true)
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::debug!("Token validation failed: {}", stderr);
                    Ok(false)
                }
            }
            Err(e) => {
                tracing::warn!("Failed to run RAPS CLI for token validation: {}", e);
                // If RAPS CLI is not available, we cannot verify the token
                // This is a security-sensitive operation, so we must fail safely
                Ok(false)
            }
        }
    }

    /// Check APS connectivity
    pub async fn check_connectivity(&self) -> Result<bool> {
        // Try to make a simple HTTP request to the APS base URL
        let client = reqwest::Client::new();
        let response = client
            .get(&format!("{}/health", self.base_url))
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await;

        match response {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(e) => {
                tracing::debug!("APS connectivity check failed: {}", e);
                Ok(false)
            }
        }
    }
}

/// Authentication setup guide
pub struct AuthSetupGuide;

impl AuthSetupGuide {
    /// Generate setup instructions for missing credentials
    pub fn generate_setup_instructions(config: &RapsConfig) -> SetupInstructions {
        let mut instructions = SetupInstructions::new();

        if config.client_id.is_empty() {
            instructions.add_step(SetupStep {
                title: "Obtain APS Client ID".to_string(),
                description: "Get your Client ID from the APS Developer Portal".to_string(),
                action: SetupAction::VisitUrl {
                    url: "https://aps.autodesk.com/developer/overview".to_string(),
                    description: "Visit the APS Developer Portal to create an app and get your Client ID".to_string(),
                },
                required: true,
            });
        }

        if config.client_secret.is_empty() {
            instructions.add_step(SetupStep {
                title: "Obtain APS Client Secret".to_string(),
                description: "Get your Client Secret from the APS Developer Portal".to_string(),
                action: SetupAction::VisitUrl {
                    url: "https://aps.autodesk.com/developer/overview".to_string(),
                    description: "Visit the APS Developer Portal to get your Client Secret".to_string(),
                },
                required: true,
            });
        }

        if config.auth_tokens.is_none() {
            instructions.add_step(SetupStep {
                title: "Authenticate with APS".to_string(),
                description: "Run the authentication command to get access tokens".to_string(),
                action: SetupAction::RunCommand {
                    command: "raps auth login".to_string(),
                    description: "This will open a browser window for you to authenticate with Autodesk".to_string(),
                },
                required: true,
            });
        }

        // Add environment variable setup instructions
        instructions.add_step(SetupStep {
            title: "Set Environment Variables (Optional)".to_string(),
            description: "You can set environment variables instead of using config files".to_string(),
            action: SetupAction::SetEnvironmentVariables {
                variables: vec![
                    ("APS_CLIENT_ID".to_string(), "your_client_id".to_string()),
                    ("APS_CLIENT_SECRET".to_string(), "your_client_secret".to_string()),
                ],
            },
            required: false,
        });

        instructions
    }

    /// Generate troubleshooting guide for authentication issues
    pub fn generate_troubleshooting_guide(validation_result: &ValidationResult) -> TroubleshootingGuide {
        let mut guide = TroubleshootingGuide::new();

        for error in &validation_result.errors {
            if error.contains("Missing APS credentials") {
                guide.add_solution(TroubleshootingSolution {
                    problem: "Missing APS credentials".to_string(),
                    solution: "Follow the setup instructions to obtain your Client ID and Client Secret from the APS Developer Portal".to_string(),
                    commands: vec!["raps auth login".to_string()],
                    links: vec!["https://aps.autodesk.com/developer/overview".to_string()],
                });
            }

            if error.contains("No authentication tokens") {
                guide.add_solution(TroubleshootingSolution {
                    problem: "No authentication tokens found".to_string(),
                    solution: "Run the authentication command to get access tokens".to_string(),
                    commands: vec!["raps auth login".to_string()],
                    links: vec!["https://aps.autodesk.com/en/docs/oauth/v2/tutorials/get-3-legged-token/".to_string()],
                });
            }

            if error.contains("Authentication token is invalid") {
                guide.add_solution(TroubleshootingSolution {
                    problem: "Authentication token is invalid".to_string(),
                    solution: "Your token may have expired or been revoked. Re-authenticate to get a new token".to_string(),
                    commands: vec!["raps auth logout".to_string(), "raps auth login".to_string()],
                    links: vec!["https://aps.autodesk.com/en/docs/oauth/v2/tutorials/get-3-legged-token/".to_string()],
                });
            }
        }

        for warning in &validation_result.warnings {
            if warning.contains("Access token has expired") {
                guide.add_solution(TroubleshootingSolution {
                    problem: "Access token has expired".to_string(),
                    solution: "Refresh your access token or re-authenticate".to_string(),
                    commands: vec!["raps auth refresh".to_string()],
                    links: vec!["https://aps.autodesk.com/en/docs/oauth/v2/tutorials/get-3-legged-token/".to_string()],
                });
            }

            if warning.contains("expires soon") {
                guide.add_solution(TroubleshootingSolution {
                    problem: "Access token expires soon".to_string(),
                    solution: "Consider refreshing your token to avoid interruption".to_string(),
                    commands: vec!["raps auth refresh".to_string()],
                    links: vec!["https://aps.autodesk.com/en/docs/oauth/v2/tutorials/get-3-legged-token/".to_string()],
                });
            }
        }

        guide
    }
}

/// Setup instructions for authentication
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupInstructions {
    pub steps: Vec<SetupStep>,
}

impl SetupInstructions {
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub fn add_step(&mut self, step: SetupStep) {
        self.steps.push(step);
    }

    pub fn required_steps(&self) -> Vec<&SetupStep> {
        self.steps.iter().filter(|step| step.required).collect()
    }

    pub fn optional_steps(&self) -> Vec<&SetupStep> {
        self.steps.iter().filter(|step| !step.required).collect()
    }
}

impl Default for SetupInstructions {
    fn default() -> Self {
        Self::new()
    }
}

/// Individual setup step
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupStep {
    pub title: String,
    pub description: String,
    pub action: SetupAction,
    pub required: bool,
}

/// Action to take for a setup step
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SetupAction {
    VisitUrl {
        url: String,
        description: String,
    },
    RunCommand {
        command: String,
        description: String,
    },
    SetEnvironmentVariables {
        variables: Vec<(String, String)>,
    },
    EditConfigFile {
        file_path: String,
        content: String,
    },
}

/// Troubleshooting guide for authentication issues
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TroubleshootingGuide {
    pub solutions: Vec<TroubleshootingSolution>,
}

impl TroubleshootingGuide {
    pub fn new() -> Self {
        Self {
            solutions: Vec::new(),
        }
    }

    pub fn add_solution(&mut self, solution: TroubleshootingSolution) {
        self.solutions.push(solution);
    }

    pub fn is_empty(&self) -> bool {
        self.solutions.is_empty()
    }
}

impl Default for TroubleshootingGuide {
    fn default() -> Self {
        Self::new()
    }
}

/// Individual troubleshooting solution
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TroubleshootingSolution {
    pub problem: String,
    pub solution: String,
    pub commands: Vec<String>,
    pub links: Vec<String>,
}

/// Response structure for token refresh CLI output
#[derive(Debug, Deserialize)]
struct TokenRefreshResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    scope: Option<String>,
}

/// Token refresh service
pub struct TokenRefresher;

impl TokenRefresher {
    /// Attempt to refresh an access token using the refresh token
    pub async fn refresh_token(config: &RapsConfig) -> Result<Option<AuthTokens>> {
        if let Some(tokens) = &config.auth_tokens {
            if let Some(_refresh_token) = &tokens.refresh_token {
                // Use RAPS CLI to refresh the token with JSON output
                let output = Command::new("raps")
                    .args(&["auth", "refresh", "--json"])
                    .output()
                    .context("Failed to run RAPS CLI for token refresh")?;

                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    
                    // Parse the JSON response from RAPS CLI
                    match serde_json::from_str::<TokenRefreshResponse>(&stdout) {
                        Ok(response) => {
                            let expires_at = Utc::now() + Duration::seconds(
                                response.expires_in.unwrap_or(3600)
                            );
                            
                            let scopes = response.scope
                                .map(|s| s.split_whitespace().map(String::from).collect())
                                .unwrap_or_else(|| tokens.scopes.clone());
                            
                            let new_tokens = AuthTokens {
                                access_token: response.access_token,
                                refresh_token: response.refresh_token.or_else(|| tokens.refresh_token.clone()),
                                expires_at,
                                scopes,
                            };
                            
                            tracing::info!("Successfully refreshed access token");
                            Ok(Some(new_tokens))
                        }
                        Err(e) => {
                            tracing::error!("Failed to parse token refresh response: {}", e);
                            tracing::debug!("Raw response: {}", stdout);
                            Ok(None)
                        }
                    }
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::warn!("Token refresh failed: {}", stderr);
                    Ok(None)
                }
            } else {
                tracing::debug!("No refresh token available");
                Ok(None)
            }
        } else {
            tracing::debug!("No tokens available to refresh");
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_validator_creation() {
        let validator = AuthValidator::new("https://developer.api.autodesk.com".to_string());
        assert_eq!(validator.base_url, "https://developer.api.autodesk.com");
    }

    #[tokio::test]
    async fn test_validate_credentials_missing() {
        let validator = AuthValidator::new("https://developer.api.autodesk.com".to_string());
        let config = RapsConfig::default();
        
        let result = validator.validate_credentials(&config).await.unwrap();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_setup_instructions_generation() {
        let config = RapsConfig::default();
        let instructions = AuthSetupGuide::generate_setup_instructions(&config);
        
        assert!(!instructions.steps.is_empty());
        assert!(!instructions.required_steps().is_empty());
    }

    #[test]
    fn test_troubleshooting_guide_generation() {
        let mut validation_result = ValidationResult::new();
        validation_result.add_error("Missing APS credentials".to_string());
        validation_result.add_warning("Access token has expired".to_string());
        
        let guide = AuthSetupGuide::generate_troubleshooting_guide(&validation_result);
        assert!(!guide.is_empty());
        assert_eq!(guide.solutions.len(), 2);
    }

    #[test]
    fn test_setup_instructions_filtering() {
        let mut instructions = SetupInstructions::new();
        instructions.add_step(SetupStep {
            title: "Required Step".to_string(),
            description: "This is required".to_string(),
            action: SetupAction::RunCommand {
                command: "test".to_string(),
                description: "Test command".to_string(),
            },
            required: true,
        });
        instructions.add_step(SetupStep {
            title: "Optional Step".to_string(),
            description: "This is optional".to_string(),
            action: SetupAction::RunCommand {
                command: "test".to_string(),
                description: "Test command".to_string(),
            },
            required: false,
        });

        assert_eq!(instructions.required_steps().len(), 1);
        assert_eq!(instructions.optional_steps().len(), 1);
    }
}