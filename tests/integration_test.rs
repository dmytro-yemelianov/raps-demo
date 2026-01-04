// Integration tests for RAPS Demo Workflows

use anyhow::Result;
use raps_demo_workflows::{ConfigManager, DemoManager, ResourceManager};

#[tokio::test]
async fn test_component_initialization() -> Result<()> {
    // Test that core components can be initialized
    let _config_manager = ConfigManager::new().await?;
    let _demo_manager = DemoManager::new()?;
    let _resource_manager = ResourceManager::new()?;

    Ok(())
}

#[tokio::test]
async fn test_demo_manager_initialization() -> Result<()> {
    // Test DemoManager with custom workflows directory
    let temp_dir = tempfile::TempDir::new()?;
    let mut demo_manager = DemoManager::with_workflows_dir(temp_dir.path())?;
    demo_manager.initialize()?;
    
    // Should have no workflows in empty directory
    assert!(demo_manager.get_workflows().is_empty());
    
    Ok(())
}

#[tokio::test]
async fn test_tui_initialization() -> Result<()> {
    // Test TUI initialization (without actually running it)
    use raps_demo_workflows::TuiApp;

    let _tui_app = TuiApp::new().await?;

    Ok(())
}
