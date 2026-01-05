//! Autodesk Sample Assets Downloader CLI
//!
//! Downloads official Autodesk sample files for use with APS demos.
//!
//! # Usage
//!
//! ```bash
//! # Download all assets
//! cargo run --bin download-assets
//!
//! # Download specific category
//! cargo run --bin download-assets -- --category inventor
//! cargo run --bin download-assets -- --category revit
//!
//! # Check status only
//! cargo run --bin download-assets -- --status
//!
//! # Specify output directory
//! cargo run --bin download-assets -- --output ./my-assets
//! ```
//!
//! # Attribution
//!
//! All sample files are provided by Autodesk, Inc.
//! © Autodesk, Inc. All rights reserved.

use anyhow::Result;
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

// Import from the library
use raps_demo_workflows::assets::{
    AssetCategory, AssetDownloader, AssetRegistry, print_attribution,
};

#[derive(Debug, Clone, ValueEnum)]
enum CategoryArg {
    Inventor,
    Revit,
    All,
}

impl From<CategoryArg> for Option<AssetCategory> {
    fn from(arg: CategoryArg) -> Self {
        match arg {
            CategoryArg::Inventor => Some(AssetCategory::Inventor),
            CategoryArg::Revit => Some(AssetCategory::Revit),
            CategoryArg::All => None,
        }
    }
}

/// Download Autodesk sample assets for APS demos
#[derive(Parser, Debug)]
#[command(name = "download-assets")]
#[command(author = "RAPS Demo")]
#[command(version = "1.0")]
#[command(about = "Downloads official Autodesk sample files for APS demos")]
#[command(long_about = r#"
Downloads official Autodesk sample files for use with Autodesk Platform Services demos.

All sample files are provided by Autodesk, Inc. and are subject to Autodesk's 
terms of use. These files are publicly available from Autodesk's official 
documentation and support resources.

© Autodesk, Inc. All rights reserved.
"#)]
struct Args {
    /// Output directory for downloaded assets
    #[arg(short, long, default_value = "./sample-models/autodesk")]
    output: PathBuf,

    /// Asset category to download
    #[arg(short, long, value_enum, default_value = "all")]
    category: CategoryArg,

    /// Only show status, don't download
    #[arg(short, long)]
    status: bool,

    /// Show detailed list of assets
    #[arg(short, long)]
    list: bool,

    /// Skip attribution notice
    #[arg(long)]
    no_attribution: bool,

    /// Force re-download even if files exist
    #[arg(short, long)]
    force: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Print attribution unless skipped
    if !args.no_attribution {
        print_attribution();
    }

    // Create downloader
    let downloader = AssetDownloader::new(&args.output)?
        .with_progress(|msg, current, total| {
            if total > 0 {
                let percent = (current as f64 / total as f64 * 100.0) as u32;
                println!("  [{}%] {}", percent, msg);
            } else {
                println!("  {}", msg);
            }
        });

    // List assets if requested
    if args.list {
        print_asset_list();
        return Ok(());
    }

    // Show status
    let status = downloader.status();
    println!("📁 Asset Directory: {}", args.output.display());
    println!("📊 {}", status.summary());
    println!();

    if args.status {
        print_detailed_status(&status);
        return Ok(());
    }

    // Check what needs downloading
    if status.is_complete() && !args.force {
        println!("✅ All assets are already downloaded!");
        println!();
        println!("Use --force to re-download existing files.");
        return Ok(());
    }

    // Confirm download
    let category_filter: Option<AssetCategory> = args.category.into();
    
    let to_download: Vec<_> = if let Some(cat) = category_filter {
        status.missing.iter()
            .filter(|a| a.category == cat)
            .collect()
    } else {
        status.missing.iter().collect()
    };

    if to_download.is_empty() {
        println!("✅ No assets to download for the selected category.");
        return Ok(());
    }

    let total_size: f32 = to_download.iter().map(|a| a.estimated_size_mb).sum();
    
    println!("📥 Will download {} assets (~{:.1} MB)", to_download.len(), total_size);
    println!();

    for asset in &to_download {
        println!("  • {} ({:.1} MB)", asset.name, asset.estimated_size_mb);
    }
    println!();

    // Prompt for confirmation
    println!("Press Enter to continue or Ctrl+C to cancel...");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    // Download assets
    println!();
    println!("🚀 Starting downloads...");
    println!();

    let registry = AssetRegistry::new();
    let mut success_count = 0;
    let mut error_count = 0;

    let assets_to_process: Vec<_> = if let Some(cat) = category_filter {
        registry.by_category(cat).into_iter().cloned().collect()
    } else {
        registry.all().to_vec()
    };

    for asset in assets_to_process {
        if downloader.is_downloaded(&asset) && !args.force {
            println!("⏭️  Skipping (exists): {}", asset.name);
            success_count += 1;
            continue;
        }

        print!("📥 {}", asset.name);
        std::io::Write::flush(&mut std::io::stdout())?;

        match downloader.download(&asset) {
            Ok(path) => {
                println!(" ✅");
                println!("   → {}", path.display());
                success_count += 1;
            }
            Err(e) => {
                println!(" ❌");
                println!("   Error: {}", e);
                error_count += 1;
            }
        }
    }

    println!();
    println!("═══════════════════════════════════════════════════════════════");
    println!("                        DOWNLOAD COMPLETE");
    println!("═══════════════════════════════════════════════════════════════");
    println!();
    println!("  ✅ Successful: {}", success_count);
    if error_count > 0 {
        println!("  ❌ Failed: {}", error_count);
    }
    println!("  📁 Location: {}", args.output.display());
    println!();

    Ok(())
}

fn print_asset_list() {
    let registry = AssetRegistry::new();
    
    println!("═══════════════════════════════════════════════════════════════");
    println!("                   AVAILABLE AUTODESK ASSETS");
    println!("═══════════════════════════════════════════════════════════════");
    println!();

    for category in [AssetCategory::Inventor, AssetCategory::Revit] {
        println!("┌─ {} ─────────────────────────────────────", category.display_name());
        println!("│");
        
        for asset in registry.by_category(category) {
            println!("│  📦 {}", asset.name);
            println!("│     {}", asset.description);
            println!("│     Size: ~{:.1} MB | Archive: {}", 
                asset.estimated_size_mb,
                if asset.is_archive { "Yes (ZIP)" } else { "No" }
            );
            println!("│");
        }
        
        println!("└──────────────────────────────────────────────────────────");
        println!();
    }

    println!("Total: {} assets (~{:.1} MB)", 
        registry.all().len(),
        registry.total_size_mb()
    );
    println!();
}

fn print_detailed_status(status: &raps_demo_workflows::assets::AssetStatus) {
    println!("═══════════════════════════════════════════════════════════════");
    println!("                       ASSET STATUS");
    println!("═══════════════════════════════════════════════════════════════");
    println!();

    if !status.downloaded.is_empty() {
        println!("✅ Downloaded ({}):", status.downloaded.len());
        for asset in &status.downloaded {
            println!("   • {} ({:.1} MB)", asset.name, asset.estimated_size_mb);
        }
        println!();
    }

    if !status.missing.is_empty() {
        println!("❌ Missing ({}):", status.missing.len());
        for asset in &status.missing {
            println!("   • {} ({:.1} MB)", asset.name, asset.estimated_size_mb);
        }
        println!();
        println!("📥 Total to download: {:.1} MB", status.missing_size_mb());
    }

    println!();
}
