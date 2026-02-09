//! Pre-flight check system for workflow execution
//!
//! Validates that all prerequisites are met before running a workflow:
//! - Authentication status
//! - Required asset files
//! - Other prerequisites (permissions, external tools)

use std::path::{Path, PathBuf};
use std::cell::RefCell;
use crate::assets::{AssetCategory, AssetDefinition, AssetDownloader, AssetRegistry, AssetStatus};
use crate::workflow::{WorkflowMetadata, PrerequisiteType};

/// Result of a single pre-flight check
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub name: String,
    pub passed: bool,
    pub message: String,
    pub action: Option<CheckAction>,
}

/// Suggested action to resolve a failed check
#[derive(Debug, Clone)]
pub enum CheckAction {
    /// Need to authenticate
    Login,
    /// Need to download specific assets
    DownloadAssets(Vec<AssetDefinition>),
    /// Need to run a command
    RunCommand(String),
    /// Generic instruction
    Instruction(String),
}

/// Overall pre-flight check status
#[derive(Debug, Clone)]
pub struct PreflightStatus {
    pub checks: Vec<CheckResult>,
    pub all_passed: bool,
    pub blocking_checks: Vec<String>,
}

impl PreflightStatus {
    /// Get a short summary string for display
    pub fn summary(&self) -> String {
        if self.all_passed {
            "✓ Ready to run".to_string()
        } else {
            let blockers = self.blocking_checks.join(", ");
            format!("✗ Missing: {}", blockers)
        }
    }
    
    /// Get checks by category for display
    pub fn auth_status(&self) -> Option<&CheckResult> {
        self.checks.iter().find(|c| c.name == "Authentication")
    }
    
    pub fn assets_status(&self) -> Option<&CheckResult> {
        self.checks.iter().find(|c| c.name == "Assets")
    }
}

/// Pre-flight checker for workflow execution
pub struct PreflightChecker {
    /// Base directory for assets
    assets_dir: PathBuf,
    /// Asset registry for looking up available assets
    registry: AssetRegistry,
    /// Cached downloader to avoid recreating HTTP client on every call
    cached_downloader: RefCell<Option<AssetDownloader>>,
    /// Cached asset status (asset definitions with download status)
    cached_assets_status: RefCell<Option<Vec<(AssetDefinition, bool)>>>,
}

impl PreflightChecker {
    /// Create a new pre-flight checker
    pub fn new() -> Self {
        Self {
            assets_dir: PathBuf::from("./sample-models/autodesk"),
            registry: AssetRegistry::new(),
            cached_downloader: RefCell::new(None),
            cached_assets_status: RefCell::new(None),
        }
    }
    
    /// Set the assets directory
    pub fn with_assets_dir<P: AsRef<Path>>(mut self, dir: P) -> Self {
        self.assets_dir = dir.as_ref().to_path_buf();
        // Reset caches when directory changes
        *self.cached_downloader.borrow_mut() = None;
        *self.cached_assets_status.borrow_mut() = None;
        self
    }
    
    /// Run all pre-flight checks for a workflow
    pub fn check(&self, workflow: &WorkflowMetadata) -> PreflightStatus {
        let mut checks = Vec::new();
        let mut all_passed = true;
        let mut blocking = Vec::new();
        
        // Check authentication
        let auth_check = self.check_authentication(workflow);
        if !auth_check.passed {
            all_passed = false;
            blocking.push("Authentication".to_string());
        }
        checks.push(auth_check);
        
        // Check required assets
        let assets_check = self.check_assets(workflow);
        if !assets_check.passed {
            all_passed = false;
            blocking.push("Assets".to_string());
        }
        checks.push(assets_check);
        
        // Check other prerequisites
        for prereq in &workflow.prerequisites {
            match prereq.prerequisite_type {
                PrerequisiteType::Authentication | PrerequisiteType::Assets => {
                    // Already handled above
                    continue;
                }
                PrerequisiteType::Permissions => {
                    checks.push(CheckResult {
                        name: "Permissions".to_string(),
                        passed: true, // Assume OK, will fail at runtime if not
                        message: prereq.description.clone(),
                        action: None,
                    });
                }
                PrerequisiteType::ExternalTool => {
                    checks.push(CheckResult {
                        name: "External Tool".to_string(),
                        passed: true, // Can't easily check this
                        message: prereq.description.clone(),
                        action: Some(CheckAction::Instruction(prereq.description.clone())),
                    });
                }
            }
        }
        
        PreflightStatus {
            checks,
            all_passed,
            blocking_checks: blocking,
        }
    }
    
    /// Check if authentication prerequisite is met
    fn check_authentication(&self, workflow: &WorkflowMetadata) -> CheckResult {
        // Check if workflow requires authentication
        let requires_auth = workflow.prerequisites.iter().any(|p| {
            matches!(p.prerequisite_type, PrerequisiteType::Authentication)
        });
        
        if !requires_auth {
            return CheckResult {
                name: "Authentication".to_string(),
                passed: true,
                message: "Not required for this workflow".to_string(),
                action: None,
            };
        }
        
        // Check for auth credentials in environment or config
        // This is a simplified check - in reality would verify token validity
        let has_credentials = std::env::var("APS_CLIENT_ID").is_ok() 
            || std::env::var("APS_ACCESS_TOKEN").is_ok()
            || Self::check_raps_auth_file();
        
        if has_credentials {
            CheckResult {
                name: "Authentication".to_string(),
                passed: true,
                message: "APS credentials found".to_string(),
                action: None,
            }
        } else {
            CheckResult {
                name: "Authentication".to_string(),
                passed: false,
                message: "APS authentication required".to_string(),
                action: Some(CheckAction::Login),
            }
        }
    }
    
    /// Check if raps auth file exists
    fn check_raps_auth_file() -> bool {
        // Check common locations for raps config
        if let Some(home) = dirs::home_dir() {
            let raps_config = home.join(".raps").join("credentials.toml");
            if raps_config.exists() {
                return true;
            }
        }
        false
    }
    
    /// Check if required assets are available
    fn check_assets(&self, workflow: &WorkflowMetadata) -> CheckResult {
        if workflow.required_assets.is_empty() {
            return CheckResult {
                name: "Assets".to_string(),
                passed: true,
                message: "No assets required".to_string(),
                action: None,
            };
        }
        
        let mut missing_files: Vec<&PathBuf> = Vec::new();
        let mut missing_assets: Vec<AssetDefinition> = Vec::new();
        
        for asset_path in &workflow.required_assets {
            // Check if the file exists
            if !asset_path.exists() {
                missing_files.push(asset_path);
                
                // Try to find matching asset in registry
                if let Some(asset_def) = self.find_matching_asset(asset_path) {
                    if !missing_assets.iter().any(|a| a.name == asset_def.name) {
                        missing_assets.push(asset_def);
                    }
                }
            }
        }
        
        if missing_files.is_empty() {
            CheckResult {
                name: "Assets".to_string(),
                passed: true,
                message: format!("{} asset(s) available", workflow.required_assets.len()),
                action: None,
            }
        } else {
            let msg = if missing_assets.is_empty() {
                format!("{} file(s) missing", missing_files.len())
            } else {
                format!("{} asset(s) need download", missing_assets.len())
            };
            
            CheckResult {
                name: "Assets".to_string(),
                passed: false,
                message: msg,
                action: if missing_assets.is_empty() {
                    Some(CheckAction::Instruction(
                        format!("Missing files: {:?}", missing_files.iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>())
                    ))
                } else {
                    Some(CheckAction::DownloadAssets(missing_assets))
                },
            }
        }
    }
    
    /// Find a matching asset definition for a required asset path
    fn find_matching_asset(&self, asset_path: &Path) -> Option<AssetDefinition> {
        let path_str = asset_path.to_string_lossy().to_lowercase();
        
        // Detect category from path
        let category = if path_str.contains("inventor") {
            Some(AssetCategory::Inventor)
        } else if path_str.contains("revit") {
            Some(AssetCategory::Revit)
        } else if path_str.contains("autocad") {
            Some(AssetCategory::AutoCAD)
        } else if path_str.contains("fusion") {
            Some(AssetCategory::Fusion)
        } else if path_str.contains("civil") {
            Some(AssetCategory::Civil3D)
        } else {
            None
        };
        
        // Find the appropriate asset based on the path content
        if let Some(cat) = category {
            let assets: Vec<&AssetDefinition> = self.registry.by_category(cat);
            
            // Try to match by specific patterns
            for asset in &assets {
                let filename = asset.filename().to_lowercase();
                
                // Check for common patterns
                if path_str.contains("stapler") && filename.contains("inventor") && filename.contains("sample") {
                    return Some((*asset).clone());
                }
                if path_str.contains("basic") && path_str.contains("revit") {
                    if asset.name.to_lowercase().contains("basic") {
                        return Some((*asset).clone());
                    }
                }
                if path_str.contains("advanced") && path_str.contains("revit") {
                    if asset.name.to_lowercase().contains("advanced") {
                        return Some((*asset).clone());
                    }
                }
            }
            
            // If no specific match, return the main sample for this category
            if let Some(main_asset) = assets.first() {
                return Some((*main_asset).clone());
            }
        }
        
        None
    }
    
    /// Get the asset downloader for downloading missing assets
    /// Uses cached downloader to avoid recreating HTTP client on every call
    fn ensure_downloader(&self) -> anyhow::Result<()> {
        let mut cached = self.cached_downloader.borrow_mut();
        if cached.is_none() {
            *cached = Some(AssetDownloader::new(&self.assets_dir)?);
        }
        Ok(())
    }
    
    /// Get the asset downloader (must call ensure_downloader first)
    pub fn get_downloader(&self) -> anyhow::Result<std::cell::Ref<'_, AssetDownloader>> {
        self.ensure_downloader()?;
        Ok(std::cell::Ref::map(self.cached_downloader.borrow(), |opt| {
            opt.as_ref().expect("downloader should be initialized")
        }))
    }
    
    /// Get the current asset status
    pub fn get_asset_status(&self) -> anyhow::Result<AssetStatus> {
        let downloader = self.get_downloader()?;
        Ok(downloader.status())
    }
    
    /// Get all assets with their download status
    /// Uses cached data for fast repeated access during UI rendering
    pub fn get_all_assets_with_status(&self) -> Vec<(AssetDefinition, bool)> {
        // Check if we have cached status
        {
            let cached = self.cached_assets_status.borrow();
            if let Some(ref status) = *cached {
                return status.clone();
            }
        }
        
        // Build and cache the status
        let status: Vec<(AssetDefinition, bool)> = match self.get_downloader() {
            Ok(downloader) => {
                self.registry.all().iter()
                    .map(|a: &AssetDefinition| (a.clone(), downloader.is_downloaded(a)))
                    .collect::<Vec<_>>()
            }
            Err(_) => {
                self.registry.all().iter()
                    .map(|a: &AssetDefinition| (a.clone(), false))
                    .collect::<Vec<_>>()
            }
        };
        
        *self.cached_assets_status.borrow_mut() = Some(status.clone());
        status
    }
    
    /// Invalidate the cached asset status (call after downloading assets)
    pub fn invalidate_asset_cache(&self) {
        *self.cached_assets_status.borrow_mut() = None;
    }
    
    /// Download a specific asset
    pub fn download_asset(&self, asset: &AssetDefinition) -> anyhow::Result<PathBuf> {
        let downloader = self.get_downloader()?;
        let result = downloader.download(asset);
        // Invalidate cache after download
        self.invalidate_asset_cache();
        result
    }
    
    /// Download all missing assets for a workflow
    pub fn download_workflow_assets(&self, workflow: &WorkflowMetadata) -> anyhow::Result<Vec<PathBuf>> {
        let check = self.check_assets(workflow);
        
        if let Some(CheckAction::DownloadAssets(assets)) = check.action {
            let downloader = self.get_downloader()?;
            let mut paths = Vec::new();
            for asset in assets {
                let path = downloader.download(&asset)?;
                paths.push(path);
            }
            // Invalidate cache after downloads
            self.invalidate_asset_cache();
            Ok(paths)
        } else {
            Ok(Vec::new())
        }
    }
}

impl Default for PreflightChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_preflight_checker_creation() {
        let checker = PreflightChecker::new();
        assert!(checker.assets_dir.ends_with("autodesk"));
    }
}
