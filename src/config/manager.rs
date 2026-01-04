// Configuration Manager implementation for RAPS Demo Workflows
//
// This module provides the main ConfigManager that handles detection and
// integration with existing RAPS CLI configuration.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use tokio::fs as async_fs;

use super::auth::{AuthSetupGuide, AuthValidator, TokenRefresher, SetupInstructions, TroubleshootingGuide};
use super::types::{
    AuthTokens, ConfigPaths, DemoConfig, EnvVars, LogLevel, Profile, RapsConfig, ValidationResult,
};

/// Main configuration manager for RAPS Demo Workflows
#[derive(Debug, Clone)]
pub struct ConfigManager {
    /// Current RAPS configuration
    raps_config: RapsConfig,
    /// Demo-specific configuration
    demo_config: DemoConfig,
    /// Available profiles
    profiles: HashMap<String, Profile>,
    /// Path to configuration directory
    config_dir: PathBuf,
    /// Authentication validator
    auth_validator: AuthValidator,
}

impl ConfigManager {
    /// Create a new configuration manager
    pub async fn new() -> Result<Self> {
        tracing::debug!("Initializing configuration manager");

        let config_dir = Self::determine_config_dir()?;
        tracing::debug!("Using configuration directory: {:?}", config_dir);

        // Ensure configuration directory exists
        if !config_dir.exists() {
            async_fs::create_dir_all(&config_dir)
                .await
                .context("Failed to create configuration directory")?;
            tracing::info!("Created configuration directory: {:?}", config_dir);
        }

        let mut manager = Self {
            raps_config: RapsConfig::default(),
            demo_config: DemoConfig::default(),
            profiles: HashMap::new(),
            config_dir,
            auth_validator: AuthValidator::new("https://developer.api.autodesk.com".to_string()),
        };

        // Load existing configuration
        manager.load_configuration().await?;

        Ok(manager)
    }

    /// Determine the configuration directory to use
    fn determine_config_dir() -> Result<PathBuf> {
        // Check environment variable first
        if let Ok(config_dir) = env::var(EnvVars::CONFIG_DIR) {
            return Ok(PathBuf::from(config_dir));
        }

        // Use default configuration directory
        ConfigPaths::default_config_dir()
    }

    /// Load configuration from files and environment variables
    async fn load_configuration(&mut self) -> Result<()> {
        tracing::debug!("Loading configuration");

        // Load from environment variables first (highest priority)
        self.load_from_environment();

        // Load RAPS configuration file
        self.load_raps_config().await?;

        // Load demo configuration file
        self.load_demo_config().await?;

        // Load profiles
        self.load_profiles().await?;

        // Apply current profile if set
        if let Some(profile_name) = &self.raps_config.current_profile.clone() {
            self.apply_profile(profile_name)?;
        }

        tracing::info!("Configuration loaded successfully");
        Ok(())
    }

    /// Load configuration from environment variables
    fn load_from_environment(&mut self) {
        tracing::debug!("Loading configuration from environment variables");

        if let Ok(client_id) = env::var(EnvVars::CLIENT_ID) {
            self.raps_config.client_id = client_id;
            tracing::debug!("Loaded client ID from environment");
        }

        if let Ok(client_secret) = env::var(EnvVars::CLIENT_SECRET) {
            self.raps_config.client_secret = client_secret;
            tracing::debug!("Loaded client secret from environment");
        }

        if let Ok(callback_url) = env::var(EnvVars::CALLBACK_URL) {
            self.raps_config.callback_url = Some(callback_url);
        }

        if let Ok(environment) = env::var(EnvVars::ENVIRONMENT) {
            self.raps_config.environment = environment;
        }

        if let Ok(base_url) = env::var(EnvVars::BASE_URL) {
            self.raps_config.base_url = base_url;
        }

        if let Ok(access_token) = env::var(EnvVars::ACCESS_TOKEN) {
            // Create a simple token structure from environment
            self.raps_config.auth_tokens = Some(AuthTokens {
                access_token,
                refresh_token: None,
                expires_at: chrono::Utc::now() + chrono::Duration::hours(1), // Assume 1 hour validity
                scopes: vec!["data:read".to_string(), "data:write".to_string()], // Default scopes
            });
            tracing::debug!("Loaded access token from environment");
        }

        if let Ok(profile) = env::var(EnvVars::PROFILE) {
            self.raps_config.current_profile = Some(profile);
        }

        if let Ok(log_level) = env::var(EnvVars::LOG_LEVEL) {
            if let Ok(level) = log_level.to_lowercase().parse::<LogLevel>() {
                self.demo_config.log_level = level;
            }
        }
    }

    /// Load RAPS configuration from file
    async fn load_raps_config(&mut self) -> Result<()> {
        let config_file = self.config_dir.join(ConfigPaths::RAPS_CONFIG_FILE);
        
        if !config_file.exists() {
            tracing::debug!("RAPS configuration file not found, using defaults");
            return Ok(());
        }

        tracing::debug!("Loading RAPS configuration from: {:?}", config_file);
        
        let content = async_fs::read_to_string(&config_file)
            .await
            .context("Failed to read RAPS configuration file")?;

        let file_config: RapsConfig = toml::from_str(&content)
            .context("Failed to parse RAPS configuration file")?;

        // Merge with existing configuration (environment variables take precedence)
        if self.raps_config.client_id.is_empty() {
            self.raps_config.client_id = file_config.client_id;
        }
        if self.raps_config.client_secret.is_empty() {
            self.raps_config.client_secret = file_config.client_secret;
        }
        if self.raps_config.callback_url.is_none() {
            self.raps_config.callback_url = file_config.callback_url;
        }
        if self.raps_config.current_profile.is_none() {
            self.raps_config.current_profile = file_config.current_profile;
        }
        if self.raps_config.auth_tokens.is_none() {
            self.raps_config.auth_tokens = file_config.auth_tokens;
        }

        tracing::debug!("RAPS configuration loaded from file");
        Ok(())
    }

    /// Load demo configuration from file
    async fn load_demo_config(&mut self) -> Result<()> {
        let config_file = self.config_dir.join(ConfigPaths::DEMO_CONFIG_FILE);
        
        if !config_file.exists() {
            tracing::debug!("Demo configuration file not found, using defaults");
            return Ok(());
        }

        tracing::debug!("Loading demo configuration from: {:?}", config_file);
        
        let content = async_fs::read_to_string(&config_file)
            .await
            .context("Failed to read demo configuration file")?;

        self.demo_config = toml::from_str(&content)
            .context("Failed to parse demo configuration file")?;

        tracing::debug!("Demo configuration loaded from file");
        Ok(())
    }

    /// Load profiles from the profiles directory
    async fn load_profiles(&mut self) -> Result<()> {
        let profiles_dir = self.config_dir.join(ConfigPaths::PROFILES_DIR);
        
        if !profiles_dir.exists() {
            tracing::debug!("Profiles directory not found, no profiles to load");
            return Ok(());
        }

        tracing::debug!("Loading profiles from: {:?}", profiles_dir);

        let mut entries = async_fs::read_dir(&profiles_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                if let Err(e) = self.load_profile(&path).await {
                    tracing::warn!("Failed to load profile from {:?}: {}", path, e);
                }
            }
        }

        tracing::debug!("Loaded {} profiles", self.profiles.len());
        Ok(())
    }

    /// Load a single profile from file
    async fn load_profile(&mut self, path: &PathBuf) -> Result<()> {
        let content = async_fs::read_to_string(path).await?;
        let profile: Profile = toml::from_str(&content)?;
        
        tracing::debug!("Loaded profile: {}", profile.name);
        self.profiles.insert(profile.name.clone(), profile);
        
        Ok(())
    }

    /// Apply a profile's configuration
    fn apply_profile(&mut self, profile_name: &str) -> Result<()> {
        let profile = self.profiles.get(profile_name)
            .context(format!("Profile '{}' not found", profile_name))?
            .clone();

        tracing::debug!("Applying profile: {}", profile_name);

        // Apply RAPS configuration from profile (if not overridden by environment)
        if self.raps_config.client_id.is_empty() {
            self.raps_config.client_id = profile.raps_config.client_id;
        }
        if self.raps_config.client_secret.is_empty() {
            self.raps_config.client_secret = profile.raps_config.client_secret;
        }
        if self.raps_config.callback_url.is_none() {
            self.raps_config.callback_url = profile.raps_config.callback_url;
        }
        if self.raps_config.auth_tokens.is_none() {
            self.raps_config.auth_tokens = profile.raps_config.auth_tokens;
        }

        // Apply demo configuration from profile
        self.demo_config = profile.demo_config;

        Ok(())
    }

    /// Get the current RAPS configuration
    pub fn raps_config(&self) -> &RapsConfig {
        &self.raps_config
    }

    /// Get the current demo configuration
    pub fn demo_config(&self) -> &DemoConfig {
        &self.demo_config
    }

    /// Get all available profiles
    pub fn profiles(&self) -> &HashMap<String, Profile> {
        &self.profiles
    }

    /// Get the current profile name
    pub fn current_profile(&self) -> Option<&str> {
        self.raps_config.current_profile.as_deref()
    }

    /// Switch to a different profile
    pub fn switch_profile(&mut self, profile_name: &str) -> Result<()> {
        if !self.profiles.contains_key(profile_name) {
            return Err(anyhow::anyhow!("Profile '{}' not found", profile_name));
        }

        self.raps_config.current_profile = Some(profile_name.to_string());
        self.apply_profile(profile_name)?;

        // Mark profile as used
        if let Some(profile) = self.profiles.get_mut(profile_name) {
            profile.mark_used();
        }

        tracing::info!("Switched to profile: {}", profile_name);
        Ok(())
    }

    /// Validate the current configuration
    pub fn validate(&self) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Check RAPS configuration
        if !self.raps_config.has_credentials() {
            result.add_error("Missing APS credentials (client_id and client_secret)".to_string());
        }

        if !self.raps_config.is_authenticated() {
            result.add_warning("No valid authentication tokens found".to_string());
        }

        // Check demo configuration
        if !self.demo_config.asset_base_path.exists() {
            result.add_warning(format!(
                "Asset base path does not exist: {:?}",
                self.demo_config.asset_base_path
            ));
        }

        if self.demo_config.max_concurrent_workflows == 0 {
            result.add_error("max_concurrent_workflows must be greater than 0".to_string());
        }

        if self.demo_config.cost_warning_threshold < 0.0 {
            result.add_error("cost_warning_threshold must be non-negative".to_string());
        }

        result
    }

    /// Check if the configuration is ready for use
    pub fn is_ready(&self) -> bool {
        self.validate().is_valid && self.raps_config.is_authenticated()
    }

    /// Save the current configuration to files
    pub async fn save(&self) -> Result<()> {
        tracing::debug!("Saving configuration");

        // Save RAPS configuration
        let raps_config_file = self.config_dir.join(ConfigPaths::RAPS_CONFIG_FILE);
        let raps_content = toml::to_string_pretty(&self.raps_config)
            .context("Failed to serialize RAPS configuration")?;
        async_fs::write(&raps_config_file, raps_content)
            .await
            .context("Failed to write RAPS configuration file")?;

        // Save demo configuration
        let demo_config_file = self.config_dir.join(ConfigPaths::DEMO_CONFIG_FILE);
        let demo_content = toml::to_string_pretty(&self.demo_config)
            .context("Failed to serialize demo configuration")?;
        async_fs::write(&demo_config_file, demo_content)
            .await
            .context("Failed to write demo configuration file")?;

        tracing::info!("Configuration saved successfully");
        Ok(())
    }

    /// Create a new profile
    pub async fn create_profile(&mut self, name: String, description: Option<String>) -> Result<()> {
        if self.profiles.contains_key(&name) {
            return Err(anyhow::anyhow!("Profile '{}' already exists", name));
        }

        let profile = Profile::new(name.clone(), description);
        
        // Save profile to file
        let profiles_dir = self.config_dir.join(ConfigPaths::PROFILES_DIR);
        if !profiles_dir.exists() {
            async_fs::create_dir_all(&profiles_dir).await?;
        }

        let profile_file = profiles_dir.join(format!("{}.toml", name));
        let content = toml::to_string_pretty(&profile)?;
        async_fs::write(&profile_file, content).await?;

        self.profiles.insert(name.clone(), profile);
        tracing::info!("Created profile: {}", name);
        
        Ok(())
    }

    /// Validate and refresh authentication if needed
    pub async fn validate_and_refresh_auth(&mut self) -> Result<ValidationResult> {
        tracing::debug!("Validating authentication");

        // First validate current credentials
        let mut validation_result = self.auth_validator.validate_credentials(&self.raps_config).await?;

        // If tokens are expired or expiring soon, try to refresh
        if let Some(tokens) = &self.raps_config.auth_tokens {
            if tokens.is_expired() || tokens.expires_within(300) {
                tracing::info!("Attempting to refresh expired/expiring tokens");
                
                match TokenRefresher::refresh_token(&self.raps_config).await {
                    Ok(Some(new_tokens)) => {
                        self.raps_config.auth_tokens = Some(new_tokens);
                        validation_result = self.auth_validator.validate_credentials(&self.raps_config).await?;
                        tracing::info!("Successfully refreshed authentication tokens");
                    }
                    Ok(None) => {
                        validation_result.add_error("Failed to refresh authentication tokens".to_string());
                    }
                    Err(e) => {
                        validation_result.add_warning(format!("Token refresh failed: {}", e));
                    }
                }
            }
        }

        Ok(validation_result)
    }

    /// Get setup instructions for missing or invalid authentication
    pub fn get_setup_instructions(&self) -> SetupInstructions {
        AuthSetupGuide::generate_setup_instructions(&self.raps_config)
    }

    /// Get troubleshooting guide for authentication issues
    pub fn get_troubleshooting_guide(&self, validation_result: &ValidationResult) -> TroubleshootingGuide {
        AuthSetupGuide::generate_troubleshooting_guide(validation_result)
    }

    /// Check APS connectivity
    pub async fn check_aps_connectivity(&self) -> Result<bool> {
        self.auth_validator.check_connectivity().await
    }

    /// Validate authentication without refreshing
    pub async fn validate_auth_only(&self) -> Result<ValidationResult> {
        self.auth_validator.validate_credentials(&self.raps_config).await
    }

    /// Update authentication tokens (e.g., after manual login)
    pub fn update_auth_tokens(&mut self, tokens: AuthTokens) {
        self.raps_config.auth_tokens = Some(tokens);
        tracing::info!("Updated authentication tokens");
    }

    /// Clear authentication tokens (e.g., for logout)
    pub fn clear_auth_tokens(&mut self) {
        self.raps_config.auth_tokens = None;
        tracing::info!("Cleared authentication tokens");
    }

    /// Check if authentication is valid and not expiring soon
    pub fn is_auth_healthy(&self) -> bool {
        self.raps_config
            .auth_tokens
            .as_ref()
            .map(|tokens| !tokens.is_expired() && !tokens.expires_within(300))
            .unwrap_or(false)
    }

    /// Delete a profile
    pub async fn delete_profile(&mut self, name: &str) -> Result<()> {
        if !self.profiles.contains_key(name) {
            return Err(anyhow::anyhow!("Profile '{}' not found", name));
        }

        // Don't allow deleting the current profile
        if self.current_profile() == Some(name) {
            return Err(anyhow::anyhow!("Cannot delete the current profile"));
        }

        // Remove profile file
        let profiles_dir = self.config_dir.join(ConfigPaths::PROFILES_DIR);
        let profile_file = profiles_dir.join(format!("{}.toml", name));
        if profile_file.exists() {
            async_fs::remove_file(&profile_file).await?;
        }

        self.profiles.remove(name);
        tracing::info!("Deleted profile: {}", name);
        
        Ok(())
    }
}

// Helper trait for parsing log levels from strings
trait LogLevelParse {
    fn parse(s: &str) -> Result<LogLevel, String>;
}

impl LogLevelParse for LogLevel {
    fn parse(s: &str) -> Result<LogLevel, String> {
        match s.to_lowercase().as_str() {
            "error" => Ok(LogLevel::Error),
            "warn" => Ok(LogLevel::Warn),
            "info" => Ok(LogLevel::Info),
            "debug" => Ok(LogLevel::Debug),
            "trace" => Ok(LogLevel::Trace),
            _ => Err(format!("Invalid log level: {}", s)),
        }
    }
}

impl std::str::FromStr for LogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_config_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        env::set_var(EnvVars::CONFIG_DIR, temp_dir.path());

        let manager = ConfigManager::new().await.unwrap();
        assert!(manager.config_dir.exists());
        
        env::remove_var(EnvVars::CONFIG_DIR);
    }

    #[tokio::test]
    async fn test_environment_variable_loading() {
        let temp_dir = TempDir::new().unwrap();
        env::set_var(EnvVars::CONFIG_DIR, temp_dir.path());
        env::set_var(EnvVars::CLIENT_ID, "test_client_id");
        env::set_var(EnvVars::CLIENT_SECRET, "test_client_secret");

        let manager = ConfigManager::new().await.unwrap();
        assert_eq!(manager.raps_config.client_id, "test_client_id");
        assert_eq!(manager.raps_config.client_secret, "test_client_secret");

        env::remove_var(EnvVars::CONFIG_DIR);
        env::remove_var(EnvVars::CLIENT_ID);
        env::remove_var(EnvVars::CLIENT_SECRET);
    }

    #[test]
    fn test_log_level_parsing() {
        assert_eq!("info".parse::<LogLevel>().unwrap(), LogLevel::Info);
        assert_eq!("DEBUG".parse::<LogLevel>().unwrap(), LogLevel::Debug);
        assert!("invalid".parse::<LogLevel>().is_err());
    }

    #[tokio::test]
    async fn test_profile_management() {
        let temp_dir = TempDir::new().unwrap();
        env::set_var(EnvVars::CONFIG_DIR, temp_dir.path());

        let mut manager = ConfigManager::new().await.unwrap();
        
        // Create a profile
        manager.create_profile(
            "test-profile".to_string(),
            Some("Test profile".to_string()),
        ).await.unwrap();

        assert!(manager.profiles.contains_key("test-profile"));

        // Switch to profile
        manager.switch_profile("test-profile").unwrap();
        assert_eq!(manager.current_profile(), Some("test-profile"));

        env::remove_var(EnvVars::CONFIG_DIR);
    }
}