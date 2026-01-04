// Configuration types for RAPS Demo Workflows
//
// This module defines configuration structures for integrating with RAPS CLI
// settings and managing demo-specific configuration.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::resource::CleanupPolicy;

/// Log level for the demo system
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Default for LogLevel {
    fn default() -> Self {
        LogLevel::Info
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Error => write!(f, "error"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Trace => write!(f, "trace"),
        }
    }
}

/// Demo-specific configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DemoConfig {
    /// Default cleanup policy for resources
    pub default_cleanup_policy: CleanupPolicy,
    /// Maximum number of concurrent workflows
    pub max_concurrent_workflows: usize,
    /// Log level for the demo system
    pub log_level: LogLevel,
    /// Base path for asset files
    pub asset_base_path: PathBuf,
    /// Base path for temporary directories
    pub temp_dir_base: PathBuf,
    /// Maximum workflow execution timeout in seconds
    pub max_execution_timeout_seconds: u64,
    /// Whether to show cost warnings
    pub show_cost_warnings: bool,
    /// Cost warning threshold in USD
    pub cost_warning_threshold: f64,
}

impl Default for DemoConfig {
    fn default() -> Self {
        Self {
            default_cleanup_policy: CleanupPolicy::default(),
            max_concurrent_workflows: 3,
            log_level: LogLevel::default(),
            asset_base_path: PathBuf::from("Assets"),
            temp_dir_base: std::env::temp_dir(),
            max_execution_timeout_seconds: 1800, // 30 minutes
            show_cost_warnings: true,
            cost_warning_threshold: 1.0, // $1.00
        }
    }
}

/// APS authentication tokens
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthTokens {
    /// Access token for APS API calls
    pub access_token: String,
    /// Refresh token for renewing access
    pub refresh_token: Option<String>,
    /// Token expiration time
    pub expires_at: DateTime<Utc>,
    /// Token scopes
    pub scopes: Vec<String>,
}

impl AuthTokens {
    /// Check if the access token is expired
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }

    /// Check if the token expires within the given duration
    pub fn expires_within(&self, seconds: i64) -> bool {
        Utc::now() + chrono::Duration::seconds(seconds) >= self.expires_at
    }

    /// Check if the token has the required scope
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == scope)
    }
}

/// RAPS CLI configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RapsConfig {
    /// APS Client ID
    pub client_id: String,
    /// APS Client Secret
    pub client_secret: String,
    /// OAuth callback URL
    pub callback_url: Option<String>,
    /// Current active profile
    pub current_profile: Option<String>,
    /// Authentication tokens
    pub auth_tokens: Option<AuthTokens>,
    /// APS environment (production, staging, etc.)
    pub environment: String,
    /// Base URL for APS APIs
    pub base_url: String,
}

impl Default for RapsConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret: String::new(),
            callback_url: None,
            current_profile: None,
            auth_tokens: None,
            environment: "production".to_string(),
            base_url: "https://developer.api.autodesk.com".to_string(),
        }
    }
}

impl RapsConfig {
    /// Check if the configuration has valid credentials
    pub fn has_credentials(&self) -> bool {
        !self.client_id.is_empty() && !self.client_secret.is_empty()
    }

    /// Check if the configuration has valid authentication
    pub fn is_authenticated(&self) -> bool {
        self.auth_tokens
            .as_ref()
            .map(|tokens| !tokens.is_expired())
            .unwrap_or(false)
    }

    /// Get the current access token if valid
    pub fn get_access_token(&self) -> Option<&str> {
        self.auth_tokens
            .as_ref()
            .filter(|tokens| !tokens.is_expired())
            .map(|tokens| tokens.access_token.as_str())
    }
}

/// Profile configuration for different environments or accounts
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// Profile name
    pub name: String,
    /// Profile description
    pub description: Option<String>,
    /// RAPS configuration for this profile
    pub raps_config: RapsConfig,
    /// Demo-specific configuration for this profile
    pub demo_config: DemoConfig,
    /// When this profile was created
    pub created_at: DateTime<Utc>,
    /// When this profile was last used
    pub last_used_at: Option<DateTime<Utc>>,
}

impl Profile {
    /// Create a new profile
    pub fn new(name: String, description: Option<String>) -> Self {
        Self {
            name,
            description,
            raps_config: RapsConfig::default(),
            demo_config: DemoConfig::default(),
            created_at: Utc::now(),
            last_used_at: None,
        }
    }

    /// Mark this profile as used
    pub fn mark_used(&mut self) {
        self.last_used_at = Some(Utc::now());
    }

    /// Check if this profile is ready for use
    pub fn is_ready(&self) -> bool {
        self.raps_config.has_credentials() && self.raps_config.is_authenticated()
    }
}

/// Configuration validation result
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationResult {
    /// Whether the configuration is valid
    pub is_valid: bool,
    /// Validation errors
    pub errors: Vec<String>,
    /// Validation warnings
    pub warnings: Vec<String>,
}

impl ValidationResult {
    /// Create a new validation result
    pub fn new() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Add an error to the validation result
    pub fn add_error(&mut self, error: String) {
        self.is_valid = false;
        self.errors.push(error);
    }

    /// Add a warning to the validation result
    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    /// Check if there are any issues
    pub fn has_issues(&self) -> bool {
        !self.errors.is_empty() || !self.warnings.is_empty()
    }
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Environment variable names used by RAPS CLI
pub struct EnvVars;

impl EnvVars {
    pub const CLIENT_ID: &'static str = "APS_CLIENT_ID";
    pub const CLIENT_SECRET: &'static str = "APS_CLIENT_SECRET";
    pub const CALLBACK_URL: &'static str = "APS_CALLBACK_URL";
    pub const ENVIRONMENT: &'static str = "APS_ENVIRONMENT";
    pub const BASE_URL: &'static str = "APS_BASE_URL";
    pub const ACCESS_TOKEN: &'static str = "APS_ACCESS_TOKEN";
    pub const PROFILE: &'static str = "RAPS_PROFILE";
    pub const CONFIG_DIR: &'static str = "RAPS_CONFIG_DIR";
    pub const LOG_LEVEL: &'static str = "RAPS_LOG_LEVEL";
}

/// Configuration file paths and names
pub struct ConfigPaths;

impl ConfigPaths {
    /// Default RAPS configuration directory name
    pub const CONFIG_DIR_NAME: &'static str = ".raps";
    
    /// RAPS configuration file name
    pub const RAPS_CONFIG_FILE: &'static str = "config.toml";
    
    /// Demo configuration file name
    pub const DEMO_CONFIG_FILE: &'static str = "demo.toml";
    
    /// Profiles directory name
    pub const PROFILES_DIR: &'static str = "profiles";
    
    /// Credentials file name
    pub const CREDENTIALS_FILE: &'static str = "credentials.toml";
    
    /// Get the default configuration directory
    pub fn default_config_dir() -> Result<PathBuf> {
        dirs::home_dir()
            .map(|home| home.join(Self::CONFIG_DIR_NAME))
            .context("Failed to determine home directory")
    }
    
    /// Get the RAPS configuration file path
    pub fn raps_config_file() -> Result<PathBuf> {
        Ok(Self::default_config_dir()?.join(Self::RAPS_CONFIG_FILE))
    }
    
    /// Get the demo configuration file path
    pub fn demo_config_file() -> Result<PathBuf> {
        Ok(Self::default_config_dir()?.join(Self::DEMO_CONFIG_FILE))
    }
    
    /// Get the profiles directory path
    pub fn profiles_dir() -> Result<PathBuf> {
        Ok(Self::default_config_dir()?.join(Self::PROFILES_DIR))
    }
    
    /// Get the credentials file path
    pub fn credentials_file() -> Result<PathBuf> {
        Ok(Self::default_config_dir()?.join(Self::CREDENTIALS_FILE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_log_level_display() {
        assert_eq!(LogLevel::Error.to_string(), "error");
        assert_eq!(LogLevel::Info.to_string(), "info");
        assert_eq!(LogLevel::Debug.to_string(), "debug");
    }

    #[test]
    fn test_demo_config_default() {
        let config = DemoConfig::default();
        assert_eq!(config.max_concurrent_workflows, 3);
        assert_eq!(config.log_level, LogLevel::Info);
        assert!(config.show_cost_warnings);
        assert_eq!(config.cost_warning_threshold, 1.0);
    }

    #[test]
    fn test_auth_tokens_expiration() {
        let expired_tokens = AuthTokens {
            access_token: "token".to_string(),
            refresh_token: None,
            expires_at: Utc::now() - Duration::hours(1),
            scopes: vec!["data:read".to_string()],
        };
        
        let valid_tokens = AuthTokens {
            access_token: "token".to_string(),
            refresh_token: None,
            expires_at: Utc::now() + Duration::hours(1),
            scopes: vec!["data:read".to_string()],
        };

        assert!(expired_tokens.is_expired());
        assert!(!valid_tokens.is_expired());
        assert!(valid_tokens.expires_within(7200)); // 2 hours - should expire within 2 hours since it expires in 1 hour
        assert!(!valid_tokens.expires_within(1800)); // 30 minutes - should not expire within 30 minutes since it expires in 1 hour
    }

    #[test]
    fn test_auth_tokens_scopes() {
        let tokens = AuthTokens {
            access_token: "token".to_string(),
            refresh_token: None,
            expires_at: Utc::now() + Duration::hours(1),
            scopes: vec!["data:read".to_string(), "data:write".to_string()],
        };

        assert!(tokens.has_scope("data:read"));
        assert!(tokens.has_scope("data:write"));
        assert!(!tokens.has_scope("bucket:create"));
    }

    #[test]
    fn test_raps_config_validation() {
        let mut config = RapsConfig::default();
        assert!(!config.has_credentials());
        assert!(!config.is_authenticated());
        assert!(config.get_access_token().is_none());

        config.client_id = "test_id".to_string();
        config.client_secret = "test_secret".to_string();
        assert!(config.has_credentials());

        config.auth_tokens = Some(AuthTokens {
            access_token: "valid_token".to_string(),
            refresh_token: None,
            expires_at: Utc::now() + Duration::hours(1),
            scopes: vec!["data:read".to_string()],
        });
        assert!(config.is_authenticated());
        assert_eq!(config.get_access_token(), Some("valid_token"));
    }

    #[test]
    fn test_profile_creation() {
        let mut profile = Profile::new(
            "test-profile".to_string(),
            Some("Test profile".to_string()),
        );

        assert_eq!(profile.name, "test-profile");
        assert!(!profile.is_ready());
        assert!(profile.last_used_at.is_none());

        profile.mark_used();
        assert!(profile.last_used_at.is_some());
    }

    #[test]
    fn test_validation_result() {
        let mut result = ValidationResult::new();
        assert!(result.is_valid);
        assert!(!result.has_issues());

        result.add_warning("Test warning".to_string());
        assert!(result.is_valid);
        assert!(result.has_issues());

        result.add_error("Test error".to_string());
        assert!(!result.is_valid);
        assert!(result.has_issues());
    }

    #[test]
    fn test_config_paths() {
        // Test that path functions don't panic
        let _config_dir = ConfigPaths::default_config_dir();
        let _raps_config = ConfigPaths::raps_config_file();
        let _demo_config = ConfigPaths::demo_config_file();
        let _profiles_dir = ConfigPaths::profiles_dir();
        let _credentials = ConfigPaths::credentials_file();
    }
}