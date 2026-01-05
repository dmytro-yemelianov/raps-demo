//! Autodesk Sample Assets Downloader
//!
//! This module provides functionality to download official Autodesk sample files
//! for use with APS (Autodesk Platform Services) demos and testing.
//!
//! # Asset Attribution
//!
//! All sample files are provided by Autodesk, Inc. and are subject to Autodesk's
//! terms of use. These files are publicly available from Autodesk's official
//! documentation and support resources.
//!
//! Source: https://www.autodesk.com
//! Copyright © Autodesk, Inc. All rights reserved.
//!
//! These sample files are intended for educational and demonstration purposes only.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::fs;
use std::io::Write;

/// Asset category for organizing downloads
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetCategory {
    Inventor,
    Revit,
    AutoCAD,
    Fusion,
    Civil3D,
}

impl AssetCategory {
    pub fn folder_name(&self) -> &'static str {
        match self {
            AssetCategory::Inventor => "inventor",
            AssetCategory::Revit => "revit",
            AssetCategory::AutoCAD => "autocad",
            AssetCategory::Fusion => "fusion",
            AssetCategory::Civil3D => "civil3d",
        }
    }
    
    pub fn display_name(&self) -> &'static str {
        match self {
            AssetCategory::Inventor => "Autodesk Inventor",
            AssetCategory::Revit => "Autodesk Revit",
            AssetCategory::AutoCAD => "AutoCAD",
            AssetCategory::Fusion => "Autodesk Fusion",
            AssetCategory::Civil3D => "Civil 3D",
        }
    }
}

/// Represents a downloadable Autodesk sample asset
#[derive(Debug, Clone)]
pub struct AssetDefinition {
    /// Display name for the asset
    pub name: String,
    /// Description of what the asset contains
    pub description: String,
    /// Download URL from Autodesk's CDN
    pub url: String,
    /// Category of the asset
    pub category: AssetCategory,
    /// Whether this is a ZIP file that needs extraction
    pub is_archive: bool,
    /// Estimated size in bytes (for display purposes)
    pub estimated_size_mb: f32,
}

impl AssetDefinition {
    /// Get the filename from the URL
    pub fn filename(&self) -> String {
        self.url
            .split('/')
            .last()
            .unwrap_or("unknown")
            .to_string()
    }
}

/// Registry of all available Autodesk sample assets
pub struct AssetRegistry {
    assets: Vec<AssetDefinition>,
}

impl Default for AssetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetRegistry {
    /// Create a new registry with all known Autodesk sample assets
    pub fn new() -> Self {
        let assets = vec![
            // ============================================================
            // AUTODESK INVENTOR SAMPLES
            // Source: Autodesk Knowledge Network
            // © Autodesk, Inc. All rights reserved.
            // ============================================================
            AssetDefinition {
                name: "Inventor 2022 Samples".to_string(),
                description: "Complete sample project files for Autodesk Inventor 2022, including assemblies, parts, and drawings".to_string(),
                url: "https://damassets.autodesk.net/content/dam/autodesk/external-assets/support-articles/inventor-sample-files/autodesk_inventor_2022_samples.zip".to_string(),
                category: AssetCategory::Inventor,
                is_archive: true,
                estimated_size_mb: 150.0,
            },
            AssetDefinition {
                name: "Inventor Sheet Metal Punch Tool".to_string(),
                description: "Sample iDE files for sheet metal punch tool functionality".to_string(),
                url: "https://damassets.autodesk.net/content/dam/autodesk/external-assets/support-articles/inventor-sample-files/inventor-sheet-metal-punchtool-new-ide-files.zip".to_string(),
                category: AssetCategory::Inventor,
                is_archive: true,
                estimated_size_mb: 5.0,
            },
            AssetDefinition {
                name: "iLogic Vault Sample Rules".to_string(),
                description: "Sample iLogic rules for Vault integration".to_string(),
                url: "https://damassets.autodesk.net/content/dam/autodesk/external-assets/support-articles/inventor-sample-files/iLogic-Vault_SampleRules.zip".to_string(),
                category: AssetCategory::Inventor,
                is_archive: true,
                estimated_size_mb: 1.0,
            },

            // ============================================================
            // AUTODESK REVIT SAMPLES
            // Source: Autodesk Revit Downloads
            // © Autodesk, Inc. All rights reserved.
            // ============================================================
            AssetDefinition {
                name: "Revit MEP Advanced Sample Family".to_string(),
                description: "Advanced MEP (Mechanical, Electrical, Plumbing) family sample".to_string(),
                url: "https://damassets.autodesk.net/content/dam/autodesk/www/revit-downloads/rmeadvancedsamplefamily.rfa".to_string(),
                category: AssetCategory::Revit,
                is_archive: false,
                estimated_size_mb: 0.5,
            },
            AssetDefinition {
                name: "Revit MEP Basic Sample Family".to_string(),
                description: "Basic MEP family sample for beginners".to_string(),
                url: "https://damassets.autodesk.net/content/dam/autodesk/www/revit-downloads/rmebasicsamplefamily.rfa".to_string(),
                category: AssetCategory::Revit,
                is_archive: false,
                estimated_size_mb: 0.3,
            },
            AssetDefinition {
                name: "Revit Structure Advanced Sample Family".to_string(),
                description: "Advanced structural family sample".to_string(),
                url: "https://damassets.autodesk.net/content/dam/autodesk/www/revit-downloads/rstadvancedsamplefamily.rfa".to_string(),
                category: AssetCategory::Revit,
                is_archive: false,
                estimated_size_mb: 0.5,
            },
            AssetDefinition {
                name: "Revit Structure Basic Sample Family".to_string(),
                description: "Basic structural family sample for beginners".to_string(),
                url: "https://damassets.autodesk.net/content/dam/autodesk/www/revit-downloads/rstbasicsamplefamily.rfa".to_string(),
                category: AssetCategory::Revit,
                is_archive: false,
                estimated_size_mb: 0.3,
            },
            AssetDefinition {
                name: "Revit Architecture Advanced Sample Family".to_string(),
                description: "Advanced architectural family sample".to_string(),
                url: "https://damassets.autodesk.net/content/dam/autodesk/www/revit-downloads/racadvancedsamplefamily.rfa".to_string(),
                category: AssetCategory::Revit,
                is_archive: false,
                estimated_size_mb: 0.5,
            },
            AssetDefinition {
                name: "Revit Architecture Basic Sample Family".to_string(),
                description: "Basic architectural family sample for beginners".to_string(),
                url: "https://damassets.autodesk.net/content/dam/autodesk/www/revit-downloads/racbasicsamplefamily.rfa".to_string(),
                category: AssetCategory::Revit,
                is_archive: false,
                estimated_size_mb: 0.3,
            },
        ];

        Self { assets }
    }

    /// Get all registered assets
    pub fn all(&self) -> &[AssetDefinition] {
        &self.assets
    }

    /// Get assets by category
    pub fn by_category(&self, category: AssetCategory) -> Vec<&AssetDefinition> {
        self.assets
            .iter()
            .filter(|a| a.category == category)
            .collect()
    }

    /// Get total estimated download size in MB
    pub fn total_size_mb(&self) -> f32 {
        self.assets.iter().map(|a| a.estimated_size_mb).sum()
    }
}

/// Downloads and manages Autodesk sample assets
pub struct AssetDownloader {
    /// Base directory for storing downloaded assets
    base_dir: PathBuf,
    /// HTTP client for downloads
    client: reqwest::blocking::Client,
    /// Progress callback
    progress_callback: Option<Box<dyn Fn(&str, usize, usize) + Send + Sync>>,
}

impl AssetDownloader {
    /// Create a new asset downloader with the specified base directory
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();
        
        // Create base directory if it doesn't exist
        if !base_dir.exists() {
            fs::create_dir_all(&base_dir)
                .context("Failed to create assets directory")?;
        }

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(300)) // 5 minute timeout for large files
            .user_agent("RAPS-Demo/1.0 (Autodesk Platform Services Demo)")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            base_dir,
            client,
            progress_callback: None,
        })
    }

    /// Set a progress callback for download updates
    pub fn with_progress<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str, usize, usize) + Send + Sync + 'static,
    {
        self.progress_callback = Some(Box::new(callback));
        self
    }

    /// Get the path where an asset would be stored
    pub fn asset_path(&self, asset: &AssetDefinition) -> PathBuf {
        self.base_dir
            .join(asset.category.folder_name())
            .join(asset.filename())
    }

    /// Check if an asset is already downloaded
    pub fn is_downloaded(&self, asset: &AssetDefinition) -> bool {
        self.asset_path(asset).exists()
    }

    /// Download a single asset
    pub fn download(&self, asset: &AssetDefinition) -> Result<PathBuf> {
        let target_dir = self.base_dir.join(asset.category.folder_name());
        if !target_dir.exists() {
            fs::create_dir_all(&target_dir)
                .context("Failed to create category directory")?;
        }

        let target_path = target_dir.join(asset.filename());

        // Skip if already downloaded
        if target_path.exists() {
            return Ok(target_path);
        }

        // Report progress
        if let Some(ref callback) = self.progress_callback {
            callback(&format!("Downloading: {}", asset.name), 0, 100);
        }

        // Download the file
        let response = self.client
            .get(&asset.url)
            .send()
            .context(format!("Failed to download {}", asset.name))?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to download {}: HTTP {}",
                asset.name,
                response.status()
            );
        }

        let total_size = response.content_length().unwrap_or(0) as usize;
        let bytes = response.bytes()
            .context(format!("Failed to read response for {}", asset.name))?;

        // Write to file
        let mut file = fs::File::create(&target_path)
            .context(format!("Failed to create file: {:?}", target_path))?;
        
        file.write_all(&bytes)
            .context(format!("Failed to write file: {:?}", target_path))?;

        // Report completion
        if let Some(ref callback) = self.progress_callback {
            callback(&format!("Downloaded: {}", asset.name), total_size, total_size);
        }

        // Extract if it's an archive
        if asset.is_archive {
            self.extract_archive(&target_path, &target_dir)?;
        }

        Ok(target_path)
    }

    /// Download all assets in a category
    pub fn download_category(&self, category: AssetCategory) -> Result<Vec<PathBuf>> {
        let registry = AssetRegistry::new();
        let assets = registry.by_category(category);
        
        let mut paths = Vec::new();
        for asset in assets {
            let path = self.download(asset)?;
            paths.push(path);
        }
        
        Ok(paths)
    }

    /// Download all registered assets
    pub fn download_all(&self) -> Result<Vec<PathBuf>> {
        let registry = AssetRegistry::new();
        
        let mut paths = Vec::new();
        for asset in registry.all() {
            let path = self.download(asset)?;
            paths.push(path);
        }
        
        Ok(paths)
    }

    /// Extract a ZIP archive
    fn extract_archive(&self, archive_path: &Path, target_dir: &Path) -> Result<()> {
        if let Some(ref callback) = self.progress_callback {
            callback(
                &format!("Extracting: {}", archive_path.file_name().unwrap_or_default().to_string_lossy()),
                0,
                100,
            );
        }

        let file = fs::File::open(archive_path)
            .context("Failed to open archive")?;
        
        let mut archive = zip::ZipArchive::new(file)
            .context("Failed to read ZIP archive")?;

        let extract_dir = target_dir.join(
            archive_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        );

        if !extract_dir.exists() {
            fs::create_dir_all(&extract_dir)?;
        }

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let outpath = extract_dir.join(file.mangled_name());

            if file.name().ends_with('/') {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p)?;
                    }
                }
                let mut outfile = fs::File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }
        }

        if let Some(ref callback) = self.progress_callback {
            callback("Extraction complete", 100, 100);
        }

        Ok(())
    }

    /// Get a summary of what's downloaded and what's missing
    pub fn status(&self) -> AssetStatus {
        let registry = AssetRegistry::new();
        let mut downloaded = Vec::new();
        let mut missing = Vec::new();

        for asset in registry.all() {
            if self.is_downloaded(asset) {
                downloaded.push(asset.clone());
            } else {
                missing.push(asset.clone());
            }
        }

        AssetStatus {
            downloaded,
            missing,
            base_dir: self.base_dir.clone(),
        }
    }
}

/// Status of downloaded assets
#[derive(Debug)]
pub struct AssetStatus {
    pub downloaded: Vec<AssetDefinition>,
    pub missing: Vec<AssetDefinition>,
    pub base_dir: PathBuf,
}

impl AssetStatus {
    /// Check if all assets are downloaded
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty()
    }

    /// Get missing size in MB
    pub fn missing_size_mb(&self) -> f32 {
        self.missing.iter().map(|a| a.estimated_size_mb).sum()
    }

    /// Format a summary for display
    pub fn summary(&self) -> String {
        format!(
            "Assets: {}/{} downloaded ({:.1} MB remaining)",
            self.downloaded.len(),
            self.downloaded.len() + self.missing.len(),
            self.missing_size_mb()
        )
    }
}

/// Print attribution notice for Autodesk assets
pub fn print_attribution() {
    println!();
    println!("═══════════════════════════════════════════════════════════════");
    println!("                    AUTODESK SAMPLE ASSETS");
    println!("═══════════════════════════════════════════════════════════════");
    println!();
    println!("  The sample files downloaded by this tool are provided by:");
    println!();
    println!("     Autodesk, Inc.");
    println!("     https://www.autodesk.com");
    println!();
    println!("  © Autodesk, Inc. All rights reserved.");
    println!();
    println!("  These files are publicly available from Autodesk's official");
    println!("  documentation and support resources. They are intended for");
    println!("  educational and demonstration purposes only.");
    println!();
    println!("  By downloading these files, you agree to Autodesk's terms of");
    println!("  use for sample content.");
    println!();
    println!("═══════════════════════════════════════════════════════════════");
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_registry() {
        let registry = AssetRegistry::new();
        assert!(!registry.all().is_empty());
        
        let inventor = registry.by_category(AssetCategory::Inventor);
        assert!(!inventor.is_empty());
        
        let revit = registry.by_category(AssetCategory::Revit);
        assert!(!revit.is_empty());
    }

    #[test]
    fn test_asset_filename() {
        let asset = AssetDefinition {
            name: "Test".to_string(),
            description: "Test".to_string(),
            url: "https://example.com/path/to/file.zip".to_string(),
            category: AssetCategory::Inventor,
            is_archive: true,
            estimated_size_mb: 1.0,
        };
        assert_eq!(asset.filename(), "file.zip");
    }
}
