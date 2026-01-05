// Flowchart Widget for RAPS Demo TUI
//
// Renders workflow steps as a visual flowchart using ratatui widgets

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, StatefulWidget},
};

use crate::workflow::{WorkflowDefinition, RapsCommand};

/// State for the flowchart widget (scroll position and execution state)
#[derive(Default, Clone)]
pub struct FlowchartState {
    pub scroll: usize,
    /// Current executing step index (if any)
    pub executing_step: Option<usize>,
    /// Completed step indices
    pub completed_steps: Vec<usize>,
}

impl FlowchartState {
    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll = self.scroll.saturating_sub(amount);
    }
    
    pub fn scroll_down(&mut self, amount: usize) {
        self.scroll += amount;
    }
    
    pub fn reset(&mut self) {
        self.scroll = 0;
    }
    
    pub fn set_execution_state(&mut self, executing: Option<usize>, completed: &[usize]) {
        self.executing_step = executing;
        self.completed_steps = completed.to_vec();
    }
}

/// A flowchart widget that renders workflow steps
pub struct FlowchartWidget<'a> {
    workflow: Option<&'a WorkflowDefinition>,
    block: Option<Block<'a>>,
}

impl<'a> FlowchartWidget<'a> {
    pub fn new(workflow: Option<&'a WorkflowDefinition>) -> Self {
        Self {
            workflow,
            block: None,
        }
    }
    
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }
    
    /// Format a command for display
    fn format_command(cmd: &RapsCommand) -> String {
        match cmd {
            RapsCommand::Auth { action } => format!("raps auth {:?}", action).to_lowercase(),
            RapsCommand::Bucket { action, params } => {
                let mut s = format!("raps bucket {:?}", action).to_lowercase();
                if let Some(name) = &params.bucket_name {
                    // Truncate long bucket names
                    let short: String = name.chars().take(15).collect();
                    s.push_str(&format!(" --key {}", short));
                    if name.len() > 15 {
                        s.push_str("...");
                    }
                }
                s
            }
            RapsCommand::Object { action, params } => {
                let bucket: String = params.bucket_name.chars().take(12).collect();
                let mut s = format!("raps object {:?} {}", action, bucket).to_lowercase();
                if params.bucket_name.len() > 12 {
                    s.push_str("...");
                }
                s
            }
            RapsCommand::Translate { action, params } => {
                let mut s = format!("raps translate {:?}", action).to_lowercase();
                if let Some(urn) = &params.urn {
                    s.push_str(&format!(" {}", urn));
                }
                if let Some(fmt) = &params.format {
                    s.push_str(&format!(" --format {}", fmt));
                }
                s
            }
            RapsCommand::DataManagement { action, .. } => {
                format!("raps dm {:?}", action).to_lowercase()
            }
            RapsCommand::DesignAutomation { action, .. } => {
                format!("raps da {:?}", action).to_lowercase()
            }
            RapsCommand::Custom { command, args } => {
                let args_str: String = args.iter().take(3).cloned().collect::<Vec<_>>().join(" ");
                format!("{} {}", command, args_str)
            }
        }
    }
    
    /// Build flowchart lines with execution state
    fn build_lines(&self, state: &FlowchartState) -> Vec<Line<'a>> {
        let Some(def) = self.workflow else {
            return vec![Line::from(Span::styled(
                "<- Select a workflow to view its flowchart",
                Style::default().fg(Color::DarkGray),
            ))];
        };
        
        let mut lines: Vec<Line<'a>> = Vec::new();
        
        // Styles
        let border_start = Style::default().fg(Color::Green);
        let border_step = Style::default().fg(Color::Cyan);
        let border_step_active = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
        let border_step_done = Style::default().fg(Color::Green);
        let border_cleanup = Style::default().fg(Color::Magenta);
        let border_end = Style::default().fg(Color::Red);
        let arrow_style = Style::default().fg(Color::DarkGray);
        let title_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
        let cmd_style = Style::default().fg(Color::Gray);
        let label_style = Style::default().fg(Color::White).add_modifier(Modifier::DIM);
        
        // Box dimensions
        let box_width = 38;
        let content_width = box_width - 4;
        let indent = "    ";
        let arrow_indent = "                   ";
        
        // Helper functions as closures
        let h_line = |n: usize, c: char| -> String { std::iter::repeat(c).take(n).collect() };
        let center_text = |text: &str, width: usize| -> String {
            let len = text.chars().count();
            if len >= width {
                text.chars().take(width).collect()
            } else {
                let left_pad = (width - len) / 2;
                let right_pad = width - len - left_pad;
                format!("{}{}{}", " ".repeat(left_pad), text, " ".repeat(right_pad))
            }
        };
        
        // Empty line at top
        lines.push(Line::from(""));
        
        // ═══════════════════════════════════
        // START BLOCK (double-line box)
        // ═══════════════════════════════════
        let top_border = format!("{}+{}+", indent, h_line(box_width - 2, '='));
        lines.push(Line::from(Span::styled(top_border, border_start)));
        
        let start_text = center_text("[START]", box_width - 4);
        lines.push(Line::from(vec![
            Span::styled(format!("{}| ", indent), border_start),
            Span::styled(start_text, title_style),
            Span::styled(" |", border_start),
        ]));
        
        let bottom_border = format!("{}+{}+{}+", indent, h_line((box_width - 4) / 2, '='), h_line((box_width - 4) / 2, '='));
        lines.push(Line::from(Span::styled(bottom_border, border_start)));
        
        // ═══════════════════════════════════
        // STEP BLOCKS
        // ═══════════════════════════════════
        for (i, step) in def.steps.iter().enumerate() {
            // Determine step style based on execution state
            let (step_border_style, status_indicator) = if state.completed_steps.contains(&i) {
                (border_step_done, "[OK]")
            } else if state.executing_step == Some(i) {
                (border_step_active, "[>>]")
            } else {
                (border_step, "    ")
            };
            
            // Connector arrow
            lines.push(Line::from(Span::styled(format!("{}|", arrow_indent), arrow_style)));
            lines.push(Line::from(Span::styled(format!("{}v", arrow_indent), arrow_style)));
            
            // Step box with step number and status
            let step_label = format!("Step {} {}", i + 1, status_indicator);
            let dashes = box_width - 6 - step_label.len();
            let top = format!("{}+-- {} {}", indent, step_label, h_line(dashes.max(1), '-'));
            lines.push(Line::from(Span::styled(top, step_border_style)));
            
            // Step name (centered, bold yellow)
            let name: String = step.name.chars().take(content_width).collect();
            let padded_name = center_text(&name, content_width);
            lines.push(Line::from(vec![
                Span::styled(format!("{}| ", indent), step_border_style),
                Span::styled(padded_name, title_style),
                Span::styled(" |", step_border_style),
            ]));
            
            // Command line (gray, italic-ish)
            let cmd = Self::format_command(&step.command);
            let cmd_truncated: String = cmd.chars().take(content_width).collect();
            let padded_cmd = format!("{:<width$}", cmd_truncated, width = content_width);
            lines.push(Line::from(vec![
                Span::styled(format!("{}| ", indent), step_border_style),
                Span::styled(padded_cmd, cmd_style),
                Span::styled(" |", step_border_style),
            ]));
            
            // Bottom of step box with connector
            let half = (box_width - 5) / 2;
            let bottom = format!("{}+{}+{}+", indent, h_line(half, '-'), h_line(half, '-'));
            lines.push(Line::from(Span::styled(bottom, step_border_style)));
        }
        
        // ═══════════════════════════════════
        // CLEANUP BLOCK (if present, rounded corners)
        // ═══════════════════════════════════
        if !def.cleanup.is_empty() {
            // Connector arrow
            lines.push(Line::from(Span::styled(format!("{}|", arrow_indent), arrow_style)));
            lines.push(Line::from(Span::styled(format!("{}v", arrow_indent), arrow_style)));
            
            let cleanup_width = 28;
            let cleanup_indent = "        ";
            
            // Rounded top
            let top = format!("{}+{}+", cleanup_indent, h_line(cleanup_width - 2, '-'));
            lines.push(Line::from(Span::styled(top, border_cleanup)));
            
            // Cleanup text
            let cleanup_text = format!("Cleanup ({} cmds)", def.cleanup.len());
            let padded = center_text(&cleanup_text, cleanup_width - 4);
            lines.push(Line::from(vec![
                Span::styled(format!("{}| ", cleanup_indent), border_cleanup),
                Span::styled(padded, Style::default().fg(Color::Magenta)),
                Span::styled(" |", border_cleanup),
            ]));
            
            // Rounded bottom with connector
            let half = (cleanup_width - 5) / 2;
            let bottom = format!("{}+{}+{}+", cleanup_indent, h_line(half, '-'), h_line(half, '-'));
            lines.push(Line::from(Span::styled(bottom, border_cleanup)));
        }
        
        // ═══════════════════════════════════
        // END BLOCK (double-line box)
        // ═══════════════════════════════════
        lines.push(Line::from(Span::styled(format!("{}|", arrow_indent), arrow_style)));
        lines.push(Line::from(Span::styled(format!("{}v", arrow_indent), arrow_style)));
        
        let top_border = format!("{}+{}+", indent, h_line(box_width - 2, '='));
        lines.push(Line::from(Span::styled(top_border, border_end)));
        
        let end_text = center_text("[END]", box_width - 4);
        lines.push(Line::from(vec![
            Span::styled(format!("{}| ", indent), border_end),
            Span::styled(end_text, Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" |", border_end),
        ]));
        
        let bottom_border = format!("{}+{}+", indent, h_line(box_width - 2, '='));
        lines.push(Line::from(Span::styled(bottom_border, border_end)));
        
        // Empty line at bottom
        lines.push(Line::from(""));
        
        lines
    }
}

impl<'a> StatefulWidget for FlowchartWidget<'a> {
    type State = FlowchartState;
    
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        // Get the inner area (accounting for block borders)
        let inner_area = match &self.block {
            Some(b) => {
                let block = b.clone();
                let inner = block.inner(area);
                block.render(area, buf);
                inner
            }
            None => area,
        };
        
        // Build all lines with execution state
        let all_lines = self.build_lines(state);
        let total_lines = all_lines.len();
        
        // Clamp scroll to valid range
        let max_scroll = total_lines.saturating_sub(inner_area.height as usize);
        if state.scroll > max_scroll {
            state.scroll = max_scroll;
        }
        
        // Get visible lines based on scroll
        let visible_lines: Vec<Line> = all_lines
            .into_iter()
            .skip(state.scroll)
            .take(inner_area.height as usize)
            .collect();
        
        // Render as paragraph
        let paragraph = Paragraph::new(visible_lines);
        paragraph.render(inner_area, buf);
    }
}

// Also implement Widget for non-stateful usage
impl<'a> Widget for FlowchartWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut state = FlowchartState::default();
        StatefulWidget::render(self, area, buf, &mut state);
    }
}
