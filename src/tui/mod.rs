use std::io;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseEventKind, MouseButton},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Terminal,
};

use std::sync::Arc;
use tokio::sync::mpsc;

mod flowchart;
use flowchart::{FlowchartWidget, FlowchartState};

mod preflight;
use preflight::{PreflightChecker, PreflightStatus, CheckAction};

use crate::workflow::{
    ExecutionStatus, ExecutionUpdate, WorkflowDiscovery, WorkflowExecutor, WorkflowMetadata,
    WorkflowDefinition, RapsCommand,
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

/// Sidebar item type for grouped workflow display
#[derive(Clone, Debug)]
enum SidebarItem {
    /// Category header (expandable/collapsible)
    Category { name: String, count: usize },
    /// Workflow entry with index into workflows vec
    Workflow { index: usize },
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
    /// Current detail tab (0 = Overview, 1 = Steps, 2 = Flowchart, 3 = Assets, 4 = YAML)
    detail_tab: usize,
    /// Scroll offset for steps view
    steps_scroll: usize,
    /// State for flowchart widget
    flowchart_state: FlowchartState,
    /// Cached layout areas for mouse click detection
    sidebar_area: Rect,
    /// Detail panel area
    detail_area: Rect,
    /// Help bar area for click detection
    help_bar_area: Rect,
    /// Current executing workflow ID
    executing_workflow_id: Option<String>,
    /// Current executing step index (0-based)
    executing_step: Option<usize>,
    /// Completed step indices
    completed_steps: Vec<usize>,
    /// Resizable panel percentage for sidebar (30-70%)
    sidebar_percent: u16,
    /// Resizable console height (5-20 lines)
    console_height: u16,
    /// Collapsed category names (for expandable groups)
    collapsed_categories: std::collections::HashSet<String>,
    /// Sidebar display items (for grouped view)
    sidebar_items: Vec<SidebarItem>,
    /// Active popup (URL to display, title)
    popup: Option<PopupState>,
    /// Flag to trigger workflow run from mouse click (handled in async main loop)
    pending_run: bool,
    /// Last click position and time for double-click detection
    last_click: Option<(u16, u16, std::time::Instant)>,
    /// Pre-flight checker for workflow requirements
    preflight_checker: PreflightChecker,
    /// Cached preflight status for selected workflow
    cached_preflight: Option<PreflightStatus>,
    /// Scroll offset for assets view
    assets_scroll: usize,
    /// Selected asset index in assets tab
    selected_asset: usize,
    /// Pending asset download action
    pending_download: Option<usize>,
}

/// State for a popup dialog
#[derive(Clone, Debug)]
struct PopupState {
    title: String,
    message: String,
    url: Option<String>,
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

        let mut app = Self {
            workflows,
            workflow_definitions,
            list_state,
            should_quit: false,
            logs: vec!["Welcome to RAPS CLI Demo Workflows! Press ? for help.".to_string()],
            executor: Arc::new(executor),
            update_receiver,
            detail_tab: 0,
            steps_scroll: 0,
            flowchart_state: FlowchartState::default(),
            sidebar_area: Rect::default(),
            detail_area: Rect::default(),
            help_bar_area: Rect::default(),
            executing_workflow_id: None,
            executing_step: None,
            completed_steps: Vec::new(),
            sidebar_percent: 30,
            console_height: 10,
            collapsed_categories: std::collections::HashSet::new(),
            sidebar_items: Vec::new(),
            popup: None,
            pending_run: false,
            last_click: None,
            preflight_checker: PreflightChecker::new(),
            cached_preflight: None,
            assets_scroll: 0,
            selected_asset: 0,
            pending_download: None,
        };
        
        // Build initial sidebar items
        app.rebuild_sidebar_items();
        
        // Initialize preflight cache for first workflow
        app.update_preflight_cache();
        
        Ok(app)
    }
    
    /// Rebuild the sidebar items based on workflows and collapsed state
    fn rebuild_sidebar_items(&mut self) {
        use std::collections::BTreeMap;
        
        // Group workflows by category
        let mut categories: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, w) in self.workflows.iter().enumerate() {
            let cat_name = format!("{}", w.category);
            categories.entry(cat_name).or_default().push(i);
        }
        
        // Build sidebar items
        self.sidebar_items.clear();
        for (cat_name, indices) in categories {
            // Add category header
            self.sidebar_items.push(SidebarItem::Category { 
                name: cat_name.clone(), 
                count: indices.len() 
            });
            
            // Add workflows if not collapsed
            if !self.collapsed_categories.contains(&cat_name) {
                for idx in indices {
                    self.sidebar_items.push(SidebarItem::Workflow { index: idx });
                }
            }
        }
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
                match event::read()? {
                    Event::Key(key) => {
                        // Only handle key press events, not release or repeat
                        // This is important on Windows where key events include Press/Release/Repeat
                        if key.kind == KeyEventKind::Press {
                            // Handle popup keys first
                            if self.popup.is_some() {
                                match key.code {
                                    KeyCode::Char('o') | KeyCode::Char('O') => {
                                        // Open URL in browser
                                        if let Some(ref popup) = self.popup {
                                            if let Some(ref url) = popup.url {
                                                let _ = open::that(url);
                                            }
                                        }
                                        self.popup = None;
                                    }
                                    _ => {
                                        // Any other key closes the popup
                                        self.popup = None;
                                    }
                                }
                                continue;
                            }
                            
                            match key.code {
                                KeyCode::Char('q') => self.should_quit = true,
                                KeyCode::Up | KeyCode::Char('k') => {
                                    if (self.detail_tab == 1 || self.detail_tab == 4) && self.steps_scroll > 0 {
                                        self.steps_scroll -= 1;
                                    } else if self.detail_tab == 2 {
                                        self.flowchart_state.scroll_up(1);
                                    } else if self.detail_tab == 3 {
                                        // Navigate assets list
                                        if self.selected_asset > 0 {
                                            self.selected_asset -= 1;
                                        }
                                    } else if self.detail_tab == 0 {
                                        self.previous_workflow();
                                        self.update_preflight_cache();
                                    }
                                }
                                KeyCode::Down | KeyCode::Char('j') => {
                                    if self.detail_tab == 1 || self.detail_tab == 4 {
                                        self.steps_scroll += 1;
                                    } else if self.detail_tab == 2 {
                                        self.flowchart_state.scroll_down(1);
                                    } else if self.detail_tab == 3 {
                                        // Navigate assets list
                                        let assets_count = self.preflight_checker.get_all_assets_with_status().len();
                                        if self.selected_asset < assets_count.saturating_sub(1) {
                                            self.selected_asset += 1;
                                        }
                                    } else if self.detail_tab == 0 {
                                        self.next_workflow();
                                        self.update_preflight_cache();
                                    }
                                }
                                KeyCode::Left | KeyCode::Char('h') => {
                                    if self.detail_tab > 0 {
                                        self.detail_tab -= 1;
                                    }
                                }
                                KeyCode::Right | KeyCode::Char('l') => {
                                    if self.detail_tab < 4 {
                                        self.detail_tab += 1;
                                    }
                                }
                                KeyCode::Tab => {
                                    self.detail_tab = (self.detail_tab + 1) % 5;
                                    self.steps_scroll = 0;
                                    self.flowchart_state.reset();
                                }
                                KeyCode::Enter => self.run_selected_workflow().await?,
                                KeyCode::Char('1') => { self.detail_tab = 0; self.steps_scroll = 0; self.flowchart_state.reset(); }
                                KeyCode::Char('2') => { self.detail_tab = 1; self.steps_scroll = 0; }
                                KeyCode::Char('3') => { self.detail_tab = 2; self.flowchart_state.reset(); }
                                KeyCode::Char('4') => { self.detail_tab = 3; self.assets_scroll = 0; }
                                KeyCode::Char('5') => { self.detail_tab = 4; self.steps_scroll = 0; }
                                KeyCode::Char('d') | KeyCode::Char('D') => {
                                    // Download selected asset if in Assets tab
                                    if self.detail_tab == 3 {
                                        self.pending_download = Some(self.selected_asset);
                                    }
                                }
                                KeyCode::PageUp => {
                                    if self.detail_tab == 1 || self.detail_tab == 4 { self.steps_scroll = self.steps_scroll.saturating_sub(5); }
                                    else if self.detail_tab == 2 { self.flowchart_state.scroll_up(5); }
                                    else if self.detail_tab == 3 { self.selected_asset = self.selected_asset.saturating_sub(5); }
                                }
                                KeyCode::PageDown => {
                                    if self.detail_tab == 1 || self.detail_tab == 4 { self.steps_scroll += 5; }
                                    else if self.detail_tab == 2 { self.flowchart_state.scroll_down(5); }
                                    else if self.detail_tab == 3 {
                                        let assets_count = self.preflight_checker.get_all_assets_with_status().len();
                                        self.selected_asset = (self.selected_asset + 5).min(assets_count.saturating_sub(1));
                                    }
                                }
                                KeyCode::Home => {
                                    self.steps_scroll = 0;
                                    self.assets_scroll = 0;
                                    self.selected_asset = 0;
                                    self.flowchart_state.reset();
                                }
                                // Resize panels with [ ] for sidebar, { } for console
                                KeyCode::Char('[') => {
                                    if self.sidebar_percent > 15 {
                                        self.sidebar_percent -= 5;
                                    }
                                }
                                KeyCode::Char(']') => {
                                    if self.sidebar_percent < 60 {
                                        self.sidebar_percent += 5;
                                    }
                                }
                                KeyCode::Char('-') => {
                                    if self.console_height > 5 {
                                        self.console_height -= 2;
                                    }
                                }
                                KeyCode::Char('+') | KeyCode::Char('=') => {
                                    if self.console_height < 25 {
                                        self.console_height += 2;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Event::Mouse(mouse) => {
                        self.handle_mouse_event(mouse);
                        // Handle pending run triggered by mouse click
                        if self.pending_run {
                            self.pending_run = false;
                            self.run_selected_workflow().await?;
                        }
                    }
                    _ => {}
                }
            }
            
            // Handle pending asset download
            if let Some(asset_idx) = self.pending_download.take() {
                self.download_asset(asset_idx);
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
                self.executing_workflow_id = Some(workflow_id.clone());
                self.executing_step = Some(0);
                self.completed_steps.clear();
                self.logs
                    .push(format!(">>> Started workflow: {}", workflow_id));
            },
            ExecutionUpdate::StepStarted { step, .. } => {
                // Find step index by matching step id with workflow definition
                if let Some(ref wf_id) = self.executing_workflow_id {
                    if let Some(def) = self.workflow_definitions.get(wf_id) {
                        if let Some(idx) = def.steps.iter().position(|s| s.id == step.id) {
                            self.executing_step = Some(idx);
                        }
                    }
                }
                self.logs.push(format!("  > Step: {}", step.name));
            },
            ExecutionUpdate::StepCompleted { result, .. } => {
                // Find step index by step_id
                let step_idx = if let Some(ref wf_id) = self.executing_workflow_id {
                    if let Some(def) = self.workflow_definitions.get(wf_id) {
                        def.steps.iter().position(|s| s.id == result.step_id)
                    } else {
                        None
                    }
                } else {
                    None
                };
                
                if let Some(idx) = step_idx {
                    self.completed_steps.push(idx);
                }
                
                if result.status == ExecutionStatus::Completed {
                    self.logs
                        .push(format!("  [OK] Step '{}' finished", result.step_id));
                    // Show stdout if available
                    if !result.stdout.is_empty() {
                        // Try to format as JSON
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&result.stdout) {
                            if let Ok(pretty) = serde_json::to_string_pretty(&json) {
                                for line in pretty.lines().take(10) {
                                    self.logs.push(format!("      {}", line));
                                }
                                if pretty.lines().count() > 10 {
                                    self.logs.push("      ... (truncated)".to_string());
                                }
                            }
                        } else {
                            // Plain text output
                            for line in result.stdout.lines().take(5) {
                                self.logs.push(format!("      {}", line));
                            }
                        }
                    }
                } else {
                    self.logs
                        .push(format!("  [FAIL] Step '{}' failed", result.step_id));
                    if !result.stderr.is_empty() {
                        for line in result.stderr.lines().take(3) {
                            self.logs.push(format!("      ERR: {}", line));
                        }
                    }
                }
            },
            ExecutionUpdate::Completed { result, .. } => {
                let wf_id = result.workflow_id.clone();
                self.executing_workflow_id = None;
                self.executing_step = None;
                let status = if result.success {
                    "COMPLETED"
                } else {
                    "FAILED"
                };
                self.logs.push(format!(
                    "=== Workflow {} {} ({} steps) ===",
                    result.workflow_id, status, result.steps_completed
                ));
                
                // Show popup with viewer URL for translation workflows
                if result.success {
                    // Check if this is a model derivative workflow
                    if wf_id.contains("translate") || wf_id.contains("derivative") || wf_id.contains("svf") {
                        self.popup = Some(PopupState {
                            title: " Workflow Complete ".to_string(),
                            message: format!("Model translation '{}' completed successfully!", wf_id),
                            url: Some("https://aps.autodesk.com/viewer".to_string()),
                        });
                    } else {
                        self.popup = Some(PopupState {
                            title: " Workflow Complete ".to_string(),
                            message: format!("Workflow '{}' completed successfully!", wf_id),
                            url: None,
                        });
                    }
                }
            },
            ExecutionUpdate::Failed { error, .. } => {
                self.executing_workflow_id = None;
                self.executing_step = None;
                self.logs.push(format!("!!! Error: {}", error.message));
                for suggestion in error.recovery_suggestions {
                    self.logs.push(format!("    Suggestion: {}", suggestion));
                }
            },
            _ => {},
        }
    }

    /// Handle mouse events for navigation and interaction
    fn handle_mouse_event(&mut self, mouse: crossterm::event::MouseEvent) {
        let x = mouse.column;
        let y = mouse.row;
        
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // If popup is open, close it on any click
                if self.popup.is_some() {
                    self.popup = None;
                    return;
                }
                
                // Check if click is in sidebar area
                if x >= self.sidebar_area.x 
                    && x < self.sidebar_area.x + self.sidebar_area.width
                    && y >= self.sidebar_area.y + 1  // +1 for border
                    && y < self.sidebar_area.y + self.sidebar_area.height - 1 
                {
                    // Calculate which sidebar item was clicked
                    let clicked_display_index = (y - self.sidebar_area.y - 1) as usize;
                    if clicked_display_index < self.sidebar_items.len() {
                        match &self.sidebar_items[clicked_display_index] {
                            SidebarItem::Category { name, .. } => {
                                // Toggle category expansion
                                let name = name.clone();
                                if self.collapsed_categories.contains(&name) {
                                    self.collapsed_categories.remove(&name);
                                } else {
                                    self.collapsed_categories.insert(name);
                                }
                                self.rebuild_sidebar_items();
                            }
                            SidebarItem::Workflow { index } => {
                                let _workflow_index = *index;
                                // Check if [Run] button was clicked (last 6 chars "[Run]" + border)
                                let run_button_x = self.sidebar_area.x + self.sidebar_area.width - 8;
                                if x >= run_button_x {
                                    // Run button clicked - select and trigger execution
                                    self.list_state.select(Some(clicked_display_index));
                                    self.update_preflight_cache();
                                    self.pending_run = true;
                                } else {
                                    // Check for double-click to run workflow
                                    let now = std::time::Instant::now();
                                    let is_double_click = if let Some((lx, ly, lt)) = self.last_click {
                                        lx == x && ly == y && now.duration_since(lt).as_millis() < 400
                                    } else {
                                        false
                                    };
                                    
                                    // Select workflow (use display index for list widget)
                                    self.list_state.select(Some(clicked_display_index));
                                    self.update_preflight_cache();
                                    
                                    if is_double_click {
                                        // Double-click triggers run
                                        self.pending_run = true;
                                        self.last_click = None;
                                    } else {
                                        // Record click for double-click detection
                                        self.last_click = Some((x, y, now));
                                    }
                                }
                                self.steps_scroll = 0;
                                self.flowchart_state.reset();
                            }
                        }
                    }
                }
                // Check if click is in detail tabs area (top row of detail panel)
                else if x >= self.detail_area.x
                    && x < self.detail_area.x + self.detail_area.width
                    && y >= self.detail_area.y
                    && y <= self.detail_area.y + 2
                {
                    // Simple tab detection based on x position
                    let tab_x = x - self.detail_area.x;
                    if tab_x < 12 {
                        self.detail_tab = 0;  // Overview
                    } else if tab_x < 20 {
                        self.detail_tab = 1;  // Steps
                    } else if tab_x < 32 {
                        self.detail_tab = 2;  // Flowchart
                    } else if tab_x < 40 {
                        self.detail_tab = 3;  // YAML
                    }
                    self.steps_scroll = 0;
                    self.flowchart_state.reset();
                }
                // Check if click is in help bar area
                else if y == self.help_bar_area.y {
                    // Detect which help button was clicked based on x position
                    // Help bar format: " ^/v  Scroll   </>  Tabs   []  Width   -+  Height   Enter  Run   q  Quit"
                    let help_x = x - self.help_bar_area.x;
                    // Run button is around position 48-58, Quit is around 60-68
                    if help_x >= 48 && help_x < 58 {
                        // "Enter Run" clicked - trigger workflow run
                        // We'll set a flag and handle in main loop
                        self.logs.push("Click: Run workflow...".to_string());
                    } else if help_x >= 60 {
                        // "q Quit" clicked
                        self.should_quit = true;
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                // Scroll in the active view
                if x >= self.detail_area.x && x < self.detail_area.x + self.detail_area.width {
                    if self.detail_tab == 1 || self.detail_tab == 3 {
                        self.steps_scroll = self.steps_scroll.saturating_sub(2);
                    } else if self.detail_tab == 2 {
                        self.flowchart_state.scroll_up(2);
                    }
                } else if x >= self.sidebar_area.x && x < self.sidebar_area.x + self.sidebar_area.width {
                    self.previous_workflow();
                }
            }
            MouseEventKind::ScrollDown => {
                // Scroll in the active view
                if x >= self.detail_area.x && x < self.detail_area.x + self.detail_area.width {
                    if self.detail_tab == 1 || self.detail_tab == 3 {
                        self.steps_scroll += 2;
                    } else if self.detail_tab == 2 {
                        self.flowchart_state.scroll_down(2);
                    }
                } else if x >= self.sidebar_area.x && x < self.sidebar_area.x + self.sidebar_area.width {
                    self.next_workflow();
                }
            }
            _ => {}
        }
    }

    fn draw(&mut self, f: &mut ratatui::Frame) {
        let size = f.size();
        
        // Main layout: content + help bar at bottom
        let main_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),      // Main content
                Constraint::Length(1),   // Help bar
            ])
            .split(size);

        // Content layout: main area + console output (resizable)
        let content_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),                            // Main panels
                Constraint::Length(self.console_height),       // Console output (resizable)
            ])
            .split(main_layout[0]);

        // Horizontal split: sidebar + details (resizable)
        let panels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(self.sidebar_percent),      // Sidebar (resizable)
                Constraint::Percentage(100 - self.sidebar_percent), // Details
            ])
            .split(content_layout[0]);

        // Cache layout areas for mouse click detection
        self.sidebar_area = panels[0];
        self.detail_area = panels[1];
        self.help_bar_area = main_layout[1];

        // Render Sidebar with workflow list
        self.render_sidebar(f, panels[0]);

        // Render Details panel with tabs
        self.render_details(f, panels[1]);

        // Render Console Output
        self.render_console(f, content_layout[1]);

        // Render Help Bar
        self.render_help_bar(f, main_layout[1]);
        
        // Render popup if active
        if let Some(ref popup) = self.popup {
            self.render_popup(f, size, popup);
        }
    }
    
    fn render_popup(&self, f: &mut ratatui::Frame, size: Rect, popup: &PopupState) {
        // Create centered popup
        let popup_width = 60.min(size.width.saturating_sub(4));
        let popup_height = 10.min(size.height.saturating_sub(4));
        
        let popup_x = (size.width - popup_width) / 2;
        let popup_y = (size.height - popup_height) / 2;
        
        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);
        
        // Clear the popup area
        use ratatui::widgets::Clear;
        f.render_widget(Clear, popup_area);
        
        // Build popup content
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(&popup.message, Style::default().fg(Color::White))),
            Line::from(""),
        ];
        
        if let Some(ref url) = popup.url {
            lines.push(Line::from(Span::styled(
                format!("URL: {}", url),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::UNDERLINED)
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "(Press 'o' to open in browser, any key to close)",
                Style::default().fg(Color::DarkGray)
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "(Press any key to close)",
                Style::default().fg(Color::DarkGray)
            )));
        }
        
        let popup_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(Span::styled(&popup.title, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
        
        let popup_content = Paragraph::new(lines)
            .block(popup_block)
            .alignment(ratatui::layout::Alignment::Center);
        
        f.render_widget(popup_content, popup_area);
    }

    fn render_sidebar(&mut self, f: &mut ratatui::Frame, area: Rect) {
        // Build list items from sidebar_items (grouped view)
        let mut items: Vec<ListItem> = Vec::new();
        
        for (_display_idx, item) in self.sidebar_items.iter().enumerate() {
            match item {
                SidebarItem::Category { name, count } => {
                    let is_collapsed = self.collapsed_categories.contains(name);
                    let indicator = if is_collapsed { "[+]" } else { "[-]" };
                    let header = format!("{} {} ({})", indicator, name, count);
                    let style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
                    items.push(ListItem::new(header).style(style));
                }
                SidebarItem::Workflow { index } => {
                    if let Some(w) = self.workflows.get(*index) {
                        let category_icon = match w.category {
                            crate::workflow::WorkflowCategory::ObjectStorage => "[OSS]",
                            crate::workflow::WorkflowCategory::ModelDerivative => "[MD]",
                            crate::workflow::WorkflowCategory::DataManagement => "[DM]",
                            crate::workflow::WorkflowCategory::DesignAutomation => "[DA]",
                            crate::workflow::WorkflowCategory::ConstructionCloud => "[ACC]",
                            crate::workflow::WorkflowCategory::RealityCapture => "[RC]",
                            crate::workflow::WorkflowCategory::Webhooks => "[WH]",
                            crate::workflow::WorkflowCategory::EndToEnd => "[E2E]",
                        };
                        // Add [Run] button indicator
                        let text = format!("  {} {} [Run]", category_icon, w.name);
                        items.push(ListItem::new(text));
                    }
                }
            }
        }

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Workflows"))
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Yellow),
            )
            .highlight_symbol("> ");

        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_details(&mut self, f: &mut ratatui::Frame, area: Rect) {
        // Split for tabs header and content
        let detail_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Tabs
                Constraint::Min(0),     // Content
            ])
            .split(area);

        // Render tabs with status indicators
        let preflight = self.cached_preflight.as_ref();
        let auth_ok = preflight.map(|p| p.auth_status().map(|c| c.passed).unwrap_or(true)).unwrap_or(true);
        let assets_ok = preflight.map(|p| p.assets_status().map(|c| c.passed).unwrap_or(true)).unwrap_or(true);
        
        let overview_title = if auth_ok && assets_ok {
            "Overview ✓".to_string()
        } else {
            "Overview ⚠".to_string()
        };
        
        let assets_title = if assets_ok {
            "Assets ✓".to_string()
        } else {
            "Assets ⚠".to_string()
        };
        
        let tab_titles = vec![overview_title, "Steps".to_string(), "Flowchart".to_string(), assets_title, "YAML".to_string()];
        let tabs = Tabs::new(tab_titles)
            .block(Block::default().borders(Borders::ALL).title("Details"))
            .select(self.detail_tab)
            .style(Style::default().fg(Color::White))
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );
        f.render_widget(tabs, detail_layout[0]);

        // Render content based on selected tab
        match self.detail_tab {
            0 => self.render_overview(f, detail_layout[1]),
            1 => self.render_steps(f, detail_layout[1]),
            2 => self.render_flowchart(f, detail_layout[1]),
            3 => self.render_assets(f, detail_layout[1]),
            4 => self.render_yaml(f, detail_layout[1]),
            _ => {}
        }
    }

    fn render_yaml(&self, f: &mut ratatui::Frame, area: Rect) {
        let content = if let Some(selected) = self.list_state.selected() {
            if let Some(SidebarItem::Workflow { index }) = self.sidebar_items.get(selected) {
                let w = &self.workflows[*index];
                if let Some(def) = self.workflow_definitions.get(&w.id) {
                    // Serialize to YAML
                    match serde_yaml::to_string(def) {
                        Ok(yaml) => yaml,
                        Err(e) => format!("Error serializing YAML: {}", e),
                    }
                } else {
                    "Workflow definition not found".to_string()
                }
            } else {
                "← Select a workflow (not a category)".to_string()
            }
        } else {
            "<- Select a workflow from the list".to_string()
        };

        let paragraph = Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL).title("YAML (scroll: ^/v)"))
            .wrap(Wrap { trim: false })
            .scroll((self.steps_scroll as u16, 0));
        f.render_widget(paragraph, area);
    }

    fn render_overview(&self, f: &mut ratatui::Frame, area: Rect) {
        let content = if let Some(selected) = self.list_state.selected() {
            if let Some(SidebarItem::Workflow { index }) = self.sidebar_items.get(selected) {
                let w = &self.workflows[*index];
                let def = self.workflow_definitions.get(&w.id);
                let step_count = def.map(|d| d.steps.len()).unwrap_or(0);
                let prereqs = w.prerequisites.iter()
                    .map(|p| format!("  • {}", p.description))
                    .collect::<Vec<_>>()
                    .join("\n");
                let prereqs_section = if prereqs.is_empty() {
                    "  None".to_string()
                } else {
                    prereqs
                };
                
                // Build preflight status section
                let preflight_section = if let Some(ref preflight) = self.cached_preflight {
                    let mut lines = Vec::new();
                    for check in &preflight.checks {
                        let icon = if check.passed { "✓" } else { "✗" };
                        let color_hint = if check.passed { "" } else { " [!]" };
                        lines.push(format!("  {} {}: {}{}", icon, check.name, check.message, color_hint));
                    }
                    if preflight.all_passed {
                        lines.push("  ══════════════════════════════════".to_string());
                        lines.push("  ✓ Ready to run! Press ENTER to execute".to_string());
                    } else {
                        lines.push("  ══════════════════════════════════".to_string());
                        lines.push("  ⚠ Missing requirements - see Assets tab".to_string());
                    }
                    lines.join("\n")
                } else {
                    "  Checking...".to_string()
                };
                
                // Required assets section
                let assets_section = if w.required_assets.is_empty() {
                    "  None".to_string()
                } else {
                    w.required_assets.iter()
                        .map(|a| format!("  • {}", a.display()))
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                
                format!(
                    "┌─ {} ─┐\n\n\
                     ID: {}\n\
                     Category: {}\n\
                     Steps: {}\n\
                     Duration: ~{} seconds\n\n\
                     ─── Description ───\n\
                     {}\n\n\
                     ─── Prerequisites ───\n\
                     {}\n\n\
                     ─── Required Assets ───\n\
                     {}\n\n\
                     ─── Pre-flight Check ───\n\
                     {}",
                    w.name,
                    w.id,
                    w.category,
                    step_count,
                    w.estimated_duration.num_seconds(),
                    w.description,
                    prereqs_section,
                    assets_section,
                    preflight_section
                )
            } else {
                "← Select a workflow (not a category)".to_string()
            }
        } else {
            "← Select a workflow from the list".to_string()
        };

        let paragraph = Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL))
            .wrap(Wrap { trim: true });
        f.render_widget(paragraph, area);
    }

    fn render_steps(&self, f: &mut ratatui::Frame, area: Rect) {
        let content = if let Some(selected) = self.list_state.selected() {
            if let Some(SidebarItem::Workflow { index }) = self.sidebar_items.get(selected) {
                let w = &self.workflows[*index];
                let is_executing = self.executing_workflow_id.as_ref() == Some(&w.id);
                
                if let Some(def) = self.workflow_definitions.get(&w.id) {
                    let steps: Vec<String> = def.steps.iter()
                        .enumerate()
                        .skip(self.steps_scroll)
                        .map(|(i, step)| {
                            let cmd_str = self.format_command(&step.command);
                            
                            // Determine step status indicator
                            let status = if is_executing {
                                if self.completed_steps.contains(&i) {
                                    "[OK]"
                                } else if self.executing_step == Some(i) {
                                    "[>>]"  // Currently executing
                                } else {
                                    "[  ]"  // Pending
                                }
                            } else {
                                "    "
                            };
                            
                            format!(
                                "+-- Step {} {} ----------------------\n\
                                 | Name: {}\n\
                                 | {}\n\
                                 |\n\
                                 | Command:\n\
                                 |   raps {}\n\
                                 +------------------------------------",
                                i + 1,
                                status,
                                step.name,
                                step.description,
                                cmd_str
                            )
                        })
                        .collect();
                    
                    if steps.is_empty() {
                        "No steps defined".to_string()
                    } else {
                        format!("Total: {} steps (scroll with ↑↓)\n\n{}", 
                            def.steps.len(),
                            steps.join("\n\n"))
                    }
                } else {
                    "Workflow definition not found".to_string()
                }
            } else {
                "← Select a workflow (not a category)".to_string()
            }
        } else {
            "← Select a workflow from the list".to_string()
        };

        let paragraph = Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL).title("Steps"))
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
    }

    fn render_flowchart(&mut self, f: &mut ratatui::Frame, area: Rect) {
        // Get the workflow definition for the selected workflow
        let (workflow_def, is_executing) = if let Some(selected) = self.list_state.selected() {
            if let Some(SidebarItem::Workflow { index }) = self.sidebar_items.get(selected) {
                let w = &self.workflows[*index];
                let is_exec = self.executing_workflow_id.as_ref() == Some(&w.id);
                (self.workflow_definitions.get(&w.id), is_exec)
            } else {
                (None, false)
            }
        } else {
            (None, false)
        };

        // Sync execution state to flowchart state
        if is_executing {
            self.flowchart_state.set_execution_state(self.executing_step, &self.completed_steps);
        } else {
            self.flowchart_state.set_execution_state(None, &[]);
        }

        // Create and render the flowchart widget
        let flowchart = FlowchartWidget::new(workflow_def)
            .block(Block::default()
                .borders(Borders::ALL)
                .title("Flowchart (^/v scroll)"));
        
        f.render_stateful_widget(flowchart, area, &mut self.flowchart_state);
    }

    fn render_assets(&self, f: &mut ratatui::Frame, area: Rect) {
        use crate::assets::AssetCategory as AssetCat;
        
        let assets_with_status = self.preflight_checker.get_all_assets_with_status();
        
        // Build content
        let mut lines: Vec<Line> = Vec::new();
        
        // Header
        lines.push(Line::from(vec![
            Span::styled("═══ ", Style::default().fg(Color::Cyan)),
            Span::styled("AUTODESK SAMPLE ASSETS", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(" ═══", Style::default().fg(Color::Cyan)),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "© Autodesk, Inc. All rights reserved.",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
        
        // Status summary
        let downloaded = assets_with_status.iter().filter(|(_, d)| *d).count();
        let total = assets_with_status.len();
        let status_color = if downloaded == total { Color::Green } else { Color::Yellow };
        lines.push(Line::from(vec![
            Span::raw("Status: "),
            Span::styled(
                format!("{}/{} downloaded", downloaded, total),
                Style::default().fg(status_color),
            ),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Use ↑↓ to select, D to download selected asset",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
        
        // Group by category
        let mut current_category: Option<AssetCat> = None;
        
        for (i, (asset, is_downloaded)) in assets_with_status.iter().enumerate() {
            // Category header
            if current_category != Some(asset.category) {
                current_category = Some(asset.category);
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("┌─ {} ─────────────────────────", asset.category.display_name()),
                    Style::default().fg(Color::Cyan),
                )));
            }
            
            // Asset entry
            let status_icon = if *is_downloaded { "✓" } else { "⬇" };
            let status_color = if *is_downloaded { Color::Green } else { Color::Yellow };
            let is_selected = i == self.selected_asset;
            
            let line_style = if is_selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };
            
            let prefix = if is_selected { "> " } else { "  " };
            
            lines.push(Line::from(vec![
                Span::styled(prefix, line_style),
                Span::styled(status_icon, Style::default().fg(status_color)),
                Span::styled(" ", Style::default()),
                Span::styled(&asset.name, line_style.add_modifier(if is_selected { Modifier::BOLD } else { Modifier::empty() })),
                Span::styled(format!(" ({:.1} MB)", asset.estimated_size_mb), Style::default().fg(Color::DarkGray)),
            ]));
            
            if is_selected {
                lines.push(Line::from(vec![
                    Span::styled("    ", Style::default()),
                    Span::styled(&asset.description, Style::default().fg(Color::Gray)),
                ]));
                if !*is_downloaded {
                    lines.push(Line::from(vec![
                        Span::styled("    ", Style::default()),
                        Span::styled("[Press D to download]", Style::default().fg(Color::Yellow)),
                    ]));
                }
            }
        }
        
        // Footer with workflow requirements if any
        if let Some(selected) = self.list_state.selected() {
            if let Some(SidebarItem::Workflow { index }) = self.sidebar_items.get(selected) {
                let w = &self.workflows[*index];
                if !w.required_assets.is_empty() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        format!("─── Required for '{}' ───", w.name),
                        Style::default().fg(Color::Magenta),
                    )));
                    for asset_path in &w.required_assets {
                        let exists = asset_path.exists();
                        let icon = if exists { "✓" } else { "✗" };
                        let color = if exists { Color::Green } else { Color::Red };
                        lines.push(Line::from(vec![
                            Span::styled(format!("  {} ", icon), Style::default().fg(color)),
                            Span::styled(asset_path.display().to_string(), Style::default()),
                        ]));
                    }
                }
            }
        }
        
        let paragraph = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Assets (D=download)"))
            .scroll((self.assets_scroll as u16, 0));
        f.render_widget(paragraph, area);
    }

    fn render_console(&self, f: &mut ratatui::Frame, area: Rect) {
        let logs_text: String = self
            .logs
            .iter()
            .rev()
            .take(8)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        
        let logs = Paragraph::new(logs_text)
            .block(Block::default().borders(Borders::ALL).title("Console Output"));
        f.render_widget(logs, area);
    }

    fn render_help_bar(&self, f: &mut ratatui::Frame, area: Rect) {
        let help_items = vec![
            ("^/v", "Scroll"),
            ("</>", "Tabs"),
            ("[]", "Width"),
            ("-+", "Height"),
            ("Enter", "Run"),
            ("q", "Quit"),
        ];
        
        let help_spans: Vec<Span> = help_items
            .iter()
            .flat_map(|(key, desc)| {
                vec![
                    Span::styled(
                        format!(" {} ", key),
                        Style::default().fg(Color::Black).bg(Color::Cyan),
                    ),
                    Span::styled(
                        format!(" {} ", desc),
                        Style::default().fg(Color::White),
                    ),
                    Span::raw(" "),
                ]
            })
            .collect();
        
        let help_line = Line::from(help_spans);
        let help = Paragraph::new(help_line)
            .style(Style::default().bg(Color::DarkGray));
        f.render_widget(help, area);
    }

    fn format_command(&self, cmd: &RapsCommand) -> String {
        match cmd {
            RapsCommand::Auth { action } => format!("auth {:?}", action).to_lowercase(),
            RapsCommand::Bucket { action, params } => {
                let mut s = format!("bucket {:?}", action).to_lowercase();
                if let Some(name) = &params.bucket_name {
                    s.push_str(&format!(" --key {}", name));
                }
                s
            }
            RapsCommand::Object { action, params } => {
                let mut s = format!("object {:?} {}", action, params.bucket_name).to_lowercase();
                if let Some(key) = &params.object_key {
                    s.push_str(&format!(" {}", key));
                }
                s
            }
            RapsCommand::Translate { action, params } => {
                let mut s = format!("translate {:?}", action).to_lowercase();
                if let Some(urn) = &params.urn {
                    s.push_str(&format!(" {}", urn));
                }
                if let Some(fmt) = &params.format {
                    s.push_str(&format!(" --format {}", fmt));
                }
                s
            }
            RapsCommand::DataManagement { action, .. } => {
                format!("dm {:?}", action).to_lowercase()
            }
            RapsCommand::DesignAutomation { action, .. } => {
                format!("da {:?}", action).to_lowercase()
            }
            RapsCommand::Custom { command, args } => {
                format!("{} {}", command, args.join(" "))
            }
        }
    }

    fn next_workflow(&mut self) {
        if self.sidebar_items.is_empty() {
            self.list_state.select(None);
            return;
        }

        let current = self.list_state.selected().unwrap_or(0);
        // Find next workflow (skip categories)
        let mut next = current + 1;
        loop {
            if next >= self.sidebar_items.len() {
                next = 0;
            }
            if matches!(self.sidebar_items[next], SidebarItem::Workflow { .. }) {
                break;
            }
            next += 1;
            if next == current {
                break; // Wrapped around, no workflows
            }
        }
        self.list_state.select(Some(next));
    }

    fn previous_workflow(&mut self) {
        if self.sidebar_items.is_empty() {
            self.list_state.select(None);
            return;
        }

        let current = self.list_state.selected().unwrap_or(0);
        // Find previous workflow (skip categories)
        let mut prev = if current == 0 { self.sidebar_items.len() - 1 } else { current - 1 };
        loop {
            if matches!(self.sidebar_items[prev], SidebarItem::Workflow { .. }) {
                break;
            }
            if prev == 0 {
                prev = self.sidebar_items.len() - 1;
            } else {
                prev -= 1;
            }
            if prev == current {
                break; // Wrapped around, no workflows
            }
        }
        self.list_state.select(Some(prev));
    }

    fn get_selected_workflow(&self) -> Option<&crate::workflow::WorkflowMetadata> {
        if let Some(selected) = self.list_state.selected() {
            if let Some(SidebarItem::Workflow { index }) = self.sidebar_items.get(selected) {
                return self.workflows.get(*index);
            }
        }
        None
    }
    
    /// Update the cached preflight status for the selected workflow
    fn update_preflight_cache(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            if let Some(SidebarItem::Workflow { index }) = self.sidebar_items.get(selected) {
                let workflow = &self.workflows[*index];
                self.cached_preflight = Some(self.preflight_checker.check(workflow));
            } else {
                self.cached_preflight = None;
            }
        } else {
            self.cached_preflight = None;
        }
    }
    
    /// Download an asset by index
    fn download_asset(&mut self, asset_index: usize) {
        let assets = self.preflight_checker.get_all_assets_with_status();
        if let Some((asset, is_downloaded)) = assets.get(asset_index) {
            if *is_downloaded {
                self.logs.push(format!("Asset already downloaded: {}", asset.name));
                return;
            }
            
            self.logs.push(format!("Downloading: {}...", asset.name));
            
            // Clone what we need before the match
            let asset_clone = asset.clone();
            
            match self.preflight_checker.download_asset(&asset_clone) {
                Ok(path) => {
                    self.logs.push(format!("  ✓ Downloaded to: {}", path.display()));
                    // Refresh preflight cache
                    self.update_preflight_cache();
                }
                Err(e) => {
                    self.logs.push(format!("  ✗ Download failed: {}", e));
                }
            }
        }
    }

    async fn run_selected_workflow(&mut self) -> Result<()> {
        // Get the actual workflow index from sidebar_items
        if let Some(selected) = self.list_state.selected() {
            if let Some(SidebarItem::Workflow { index: workflow_index }) = self.sidebar_items.get(selected) {
                let metadata = &self.workflows[*workflow_index];
                
                // Check preflight status before running
                let preflight = self.preflight_checker.check(metadata);
                
                if !preflight.all_passed {
                    // Show popup with missing requirements
                    let blockers = preflight.blocking_checks.join(", ");
                    
                    // Check if assets can be downloaded
                    let has_downloadable = preflight.checks.iter().any(|c| {
                        matches!(&c.action, Some(CheckAction::DownloadAssets(_)))
                    });
                    
                    if has_downloadable {
                        self.popup = Some(PopupState {
                            title: " Missing Requirements ".to_string(),
                            message: format!(
                                "Cannot run '{}'\n\nMissing: {}\n\nGo to Assets tab (press 4) to download required files.",
                                metadata.name, blockers
                            ),
                            url: None,
                        });
                    } else {
                        self.popup = Some(PopupState {
                            title: " Missing Requirements ".to_string(),
                            message: format!(
                                "Cannot run '{}'\n\nMissing: {}\n\nPlease resolve these requirements first.",
                                metadata.name, blockers
                            ),
                            url: None,
                        });
                    }
                    return Ok(());
                }

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
        }
        Ok(())
    }
}
