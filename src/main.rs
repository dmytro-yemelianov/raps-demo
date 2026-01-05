// RAPS Demo Workflows - Interactive demonstration system for Autodesk Platform Services
//
// This application provides a Terminal User Interface (TUI) for discovering and executing
// demo workflows that showcase APS capabilities through the RAPS CLI.

use anyhow::Result;
use clap::Parser;

mod assets;
mod config;
mod demo;
mod resource;
mod tui;
mod utils;
mod workflow;

use crate::tui::TuiApp;
use crate::workflow::{ExecutionOptions, WorkflowDiscovery, WorkflowExecutor};

/// RAPS Demo Workflows - Interactive APS demonstration system
#[derive(Parser)]
#[command(name = "raps-demo")]
#[command(
    about = "Interactive demo system for RAPS CLI showcasing Autodesk Platform Services workflows"
)]
#[command(version)]
struct Args {
    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Configuration file path
    #[arg(short, long)]
    config: Option<String>,

    /// Run in non-interactive mode (skip TUI)
    #[arg(long)]
    no_tui: bool,

    /// List available workflows (requires --no-tui)
    #[arg(long)]
    list: bool,

    /// Workflow to execute directly (bypasses TUI)
    #[arg(long)]
    workflow: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    init_logging(args.verbose)?;

    tracing::info!("Starting RAPS Demo Workflows system");

    if args.no_tui {
        // Run in non-interactive mode
        tracing::info!("Running in non-interactive mode");
        run_cli_mode(args.workflow, args.list).await?;
    } else {
        // Launch TUI application
        tracing::info!("Launching TUI application");
        let mut app = TuiApp::new().await?;
        app.run().await?;
    }

    tracing::info!("RAPS Demo Workflows system shutdown complete");
    Ok(())
}

/// Run in non-interactive CLI mode
async fn run_cli_mode(workflow_id: Option<String>, list_only: bool) -> Result<()> {
    let workflows_dir = std::path::Path::new("./workflows");
    
    // Ensure workflows directory exists
    if !workflows_dir.exists() {
        std::fs::create_dir_all(workflows_dir)?;
    }
    
    let mut discovery = WorkflowDiscovery::new(workflows_dir)?;
    let workflows = discovery.discover_workflows()?;

    // If --list flag is set, or no workflow specified, list workflows
    if list_only || workflow_id.is_none() {
        // List available workflows
        println!("Available workflows:\n");
        
        if workflows.is_empty() {
            println!("  No workflows found in ./workflows/");
            println!("\n  Create workflow YAML files in the workflows/ directory to get started.");
        } else {
            for workflow in &workflows {
                println!("  {} - {}", workflow.id, workflow.name);
                println!("    Category: {}", workflow.category);
                println!("    {}\n", workflow.description);
            }
            
            println!("Run a workflow with: raps-demo --no-tui --workflow <workflow-id>");
        }
        return Ok(());
    }
    
    if let Some(workflow_id) = workflow_id {
        // Execute specific workflow
        tracing::info!("Executing workflow: {}", workflow_id);
        
        if let Some(definition) = discovery.get_workflow(&workflow_id) {
            let definition = definition.clone();
            let (executor, mut receiver) = WorkflowExecutor::new().with_progress_reporting();
            
            println!("Starting workflow: {} - {}", definition.metadata.name, definition.metadata.description);
            
            let options = ExecutionOptions {
                interactive: false,
                verbose: true,
                auto_cleanup: true,
                ..Default::default()
            };
            
            let _handle = executor.execute_workflow(definition, options).await?;
            
            // Wait for execution updates
            while let Some(update) = receiver.recv().await {
                match update {
                    workflow::ExecutionUpdate::StepStarted { step, .. } => {
                        println!("  → Step: {}", step.name);
                    }
                    workflow::ExecutionUpdate::StepCompleted { result, .. } => {
                        let status = if result.status == workflow::ExecutionStatus::Completed {
                            "✓"
                        } else {
                            "✗"
                        };
                        println!("  {} Completed: {}", status, result.step_id);
                    }
                    workflow::ExecutionUpdate::Completed { result, .. } => {
                        if result.success {
                            println!("\n✓ Workflow completed successfully ({} steps)", result.steps_completed);
                        } else {
                            println!("\n✗ Workflow failed after {} steps", result.steps_completed);
                        }
                        break;
                    }
                    workflow::ExecutionUpdate::Failed { error, .. } => {
                        println!("\n✗ Workflow failed: {}", error.message);
                        for suggestion in &error.recovery_suggestions {
                            println!("  Suggestion: {}", suggestion);
                        }
                        break;
                    }
                    _ => {}
                }
            }
        } else {
            eprintln!("Error: Workflow '{}' not found", workflow_id);
            eprintln!("\nAvailable workflows:");
            for workflow in &workflows {
                println!("  - {} ({})", workflow.id, workflow.name);
            }
            std::process::exit(1);
        }
    }
    
    Ok(())
}

/// Initialize logging based on verbosity level
fn init_logging(verbose: bool) -> Result<()> {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    let log_level = if verbose { "debug" } else { "info" };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("raps_demo_workflows={}", log_level).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    Ok(())
}
