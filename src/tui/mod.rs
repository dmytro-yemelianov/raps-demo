use std::io;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};

use std::sync::Arc;
use tokio::sync::mpsc;

use crate::workflow::{
    ExecutionStatus, ExecutionUpdate, WorkflowDiscovery, WorkflowExecutor, WorkflowMetadata,
    WorkflowDefinition,
};

/// Guard to ensure terminal is restored even on panic
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Attempt to restore terminal state
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
    }
}

pub struct TuiApp {
    /// List of discovered workflows
    workflows: Vec<WorkflowMetadata>,
    /// Cached workflow definitions for quick access
    workflow_definitions: std::collections::HashMap<String, WorkflowDefinition>,
    /// State for the workflow list
    list_state: ListState,
    /// Whether the app should exit
    should_quit: bool,
    /// Console logs/output
    logs: Vec<String>,
    /// Workflow engine executor
    executor: Arc<WorkflowExecutor>,
    /// Receiver for execution updates
    update_receiver: mpsc::UnboundedReceiver<ExecutionUpdate>,
}

impl TuiApp {
    /// Create a new TUI application instance
    pub async fn new() -> Result<Self> {
        tracing::debug!("Initializing TUI application");

        // Ensure workflows directory exists
        let workflows_dir = std::path::Path::new("./workflows");
        if !workflows_dir.exists() {
            std::fs::create_dir_all(workflows_dir)
                .context("Failed to create workflows directory")?;
        }

        // Discover workflows
        let mut discovery = WorkflowDiscovery::new(workflows_dir)
            .context("Failed to initialize workflow discovery")?;
        let workflows = discovery.discover_workflows()?;

        // Cache workflow definitions
        let workflow_definitions = discovery.get_workflows().clone();

        let mut list_state = ListState::default();
        if !workflows.is_empty() {
            list_state.select(Some(0));
        }

        let (executor, update_receiver) = WorkflowExecutor::new().with_progress_reporting();

        Ok(Self {
            workflows,
            workflow_definitions,
            list_state,
            should_quit: false,
            logs: vec!["Welcome to RAPS CLI Demo Workflows!".to_string()],
            executor: Arc::new(executor),
            update_receiver,
        })
    }

    /// Run the TUI application main loop
    pub async fn run(&mut self) -> Result<()> {
        tracing::info!("Starting TUI main loop");

        // Create terminal guard to ensure cleanup on panic/error
        let _guard = TerminalGuard;

        // Set up terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Move receiver out of self to avoid borrow conflicts in select!
        let mut receiver =
            std::mem::replace(&mut self.update_receiver, mpsc::unbounded_channel().1);

        // Main event loop
        loop {
            if self.should_quit {
                break;
            }

            terminal.draw(|f| self.draw(f))?;

            // Poll for events with timeout - simple synchronous approach
            // This avoids race conditions with spawn_blocking
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') => self.should_quit = true,
                        KeyCode::Up | KeyCode::Char('k') => self.previous_workflow(),
                        KeyCode::Down | KeyCode::Char('j') => self.next_workflow(),
                        KeyCode::Enter => self.run_selected_workflow().await?,
                        _ => {}
                    }
                }
            }

            // Check for execution updates (non-blocking)
            while let Ok(update) = receiver.try_recv() {
                self.handle_execution_update(update);
            }
        }

        // Put receiver back
        self.update_receiver = receiver;

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        Ok(())
    }

    /// Handle an update from the execution engine
    fn handle_execution_update(&mut self, update: ExecutionUpdate) {
        match update {
            ExecutionUpdate::Started { workflow_id, .. } => {
                self.logs
                    .push(format!(">>> Started workflow: {}", workflow_id));
            },
            ExecutionUpdate::StepStarted { step, .. } => {
                self.logs.push(format!("  > Step: {}", step.name));
            },
            ExecutionUpdate::StepCompleted { result, .. } => {
                if result.status == ExecutionStatus::Completed {
                    self.logs
                        .push(format!("  [OK] Step '{}' finished", result.step_id));
                } else {
                    self.logs
                        .push(format!("  [FAIL] Step '{}' failed", result.step_id));
                }
            },
            ExecutionUpdate::Completed { result, .. } => {
                let status = if result.success {
                    "COMPLETED"
                } else {
                    "FAILED"
                };
                self.logs.push(format!(
                    "=== Workflow {} {} ({} steps) ===",
                    result.workflow_id, status, result.steps_completed
                ));
            },
            ExecutionUpdate::Failed { error, .. } => {
                self.logs.push(format!("!!! Error: {}", error.message));
                for suggestion in error.recovery_suggestions {
                    self.logs.push(format!("    Suggestion: {}", suggestion));
                }
            },
            _ => {},
        }
    }

    fn draw(&mut self, f: &mut ratatui::Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Min(0),
                    Constraint::Length(10), // Logger
                ]
                .as_ref(),
            )
            .split(f.size());

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Percentage(30), // Sidebar
                    Constraint::Percentage(70), // Detail
                ]
                .as_ref(),
            )
            .split(chunks[0]);

        // Render Sidebar
        let items: Vec<ListItem> = self
            .workflows
            .iter()
            .map(|w| ListItem::new(w.name.clone()))
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Workflows"))
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Yellow),
            )
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, main_chunks[0], &mut self.list_state);

        // Render Details
        let detail_text = if let Some(index) = self.list_state.selected() {
            if index < self.workflows.len() {
                let w = &self.workflows[index];
                format!(
                    "ID: {}\nName: {}\nCategory: {:?}\n\nDescription:\n{}\n\nEstimated Duration: {:?}\n\nPress ENTER to run this workflow",
                    w.id, w.name, w.category, w.description, w.estimated_duration
                )
            } else {
                "Selected workflow not found".to_string()
            }
        } else {
            "Select a workflow from the list".to_string()
        };

        let detail = Paragraph::new(detail_text).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Workflow Details"),
        );
        f.render_widget(detail, main_chunks[1]);

        // Render Logs
        let logs_text: String = self
            .logs
            .iter()
            .rev()
            .take(8)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        let logs = Paragraph::new(logs_text).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Console Output"),
        );
        f.render_widget(logs, chunks[1]);
    }

    fn next_workflow(&mut self) {
        if self.workflows.is_empty() {
            self.list_state.select(None);
            return;
        }

        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.workflows.len() - 1 {
                    0
                } else {
                    i + 1
                }
            },
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn previous_workflow(&mut self) {
        if self.workflows.is_empty() {
            self.list_state.select(None);
            return;
        }

        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.workflows.len() - 1
                } else {
                    i - 1
                }
            },
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    async fn run_selected_workflow(&mut self) -> Result<()> {
        if let Some(index) = self.list_state.selected() {
            let metadata = &self.workflows[index];

            // Use cached workflow definition instead of re-discovering
            if let Some(definition) = self.workflow_definitions.get(&metadata.id) {
                let definition = definition.clone();
                self.logs
                    .push(format!(">>> Executing workflow: {}", metadata.name));

                let options = crate::workflow::ExecutionOptions::default();
                let executor: Arc<WorkflowExecutor> = Arc::clone(&self.executor);

                // execute_workflow spawns in background
                executor.execute_workflow(definition, options).await?;
            } else {
                self.logs.push(format!(
                    "!!! Workflow definition not found: {}",
                    metadata.id
                ));
            }
        }
        Ok(())
    }
}
