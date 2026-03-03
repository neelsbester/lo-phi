//! In-TUI progress overlay for the pipeline execution phase.
//!
//! Renders pipeline stage progress inside the same persistent 66-wide shell used by
//! the wizard and dashboard so the user never sees a raw terminal during execution.
//!
//! # Layout
//!
//! ```text
//!               Lo-phi ASCII logo
//!     ┌────────── Running Pipeline ──────────┐
//!     │                                      │
//!     │  ✓ Loading dataset          2.3s     │
//!     │  ✓ Missing value analysis   0.1s     │
//!     │  ◐ Gini/IV analysis...               │
//!     │    142/500 features                  │
//!     │  · Correlation analysis              │
//!     │  · Saving results                    │
//!     │  · Generating reports                │
//!     │                                      │
//!     │  Elapsed: 12.4s                      │
//!     └──────────────────────────────────────┘
//!       [Q] quit (aborts pipeline)
//! ```

use std::io::Stdout;
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Rect},
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph},
    Terminal,
};

use super::shared::{draw_too_small_overlay, render_logo, themed, MIN_COLS, MIN_ROWS};
use super::theme;
use crate::pipeline::progress::{PipelineStage, ProgressEvent, ProgressReceiver, SummaryData};

/// Spinner frames (braille dot sequence)
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// A single stage slot displayed in the overlay.
struct StageRow {
    label: &'static str,
    stage: PipelineStage,
    status: StageStatus,
    elapsed_secs: f64,
}

#[derive(Clone, PartialEq)]
enum StageStatus {
    Pending,
    Running,
    Done,
}

/// State for the progress overlay.
pub struct ProgressOverlay {
    rows: Vec<StageRow>,
    current_idx: usize,
    start_time: Instant,
    stage_start: Instant,
    spinner_frame: usize,
    detail: Option<String>,
    pub complete: bool,
    /// Frozen total elapsed seconds, set when pipeline completes.
    final_elapsed_secs: Option<f64>,
    summary_lines: Vec<String>,
    /// Structured reduction summary from the pipeline.
    summary_data: Option<SummaryData>,
    /// Set to true when the user presses Q during the pipeline run.
    pub abort_requested: bool,
}

impl ProgressOverlay {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            rows: vec![
                StageRow {
                    label: "Loading dataset",
                    stage: PipelineStage::Loading,
                    status: StageStatus::Pending,
                    elapsed_secs: 0.0,
                },
                StageRow {
                    label: "Validating target",
                    stage: PipelineStage::Validating,
                    status: StageStatus::Pending,
                    elapsed_secs: 0.0,
                },
                StageRow {
                    label: "Missing value analysis",
                    stage: PipelineStage::MissingAnalysis,
                    status: StageStatus::Pending,
                    elapsed_secs: 0.0,
                },
                StageRow {
                    label: "Gini/IV analysis",
                    stage: PipelineStage::GiniAnalysis,
                    status: StageStatus::Pending,
                    elapsed_secs: 0.0,
                },
                StageRow {
                    label: "Correlation analysis",
                    stage: PipelineStage::CorrelationAnalysis,
                    status: StageStatus::Pending,
                    elapsed_secs: 0.0,
                },
                StageRow {
                    label: "Saving results",
                    stage: PipelineStage::Saving,
                    status: StageStatus::Pending,
                    elapsed_secs: 0.0,
                },
                StageRow {
                    label: "Generating reports",
                    stage: PipelineStage::Reports,
                    status: StageStatus::Pending,
                    elapsed_secs: 0.0,
                },
            ],
            current_idx: 0,
            start_time: now,
            stage_start: now,
            spinner_frame: 0,
            detail: None,
            complete: false,
            final_elapsed_secs: None,
            summary_lines: Vec::new(),
            summary_data: None,
            abort_requested: false,
        }
    }

    /// Process incoming progress events.
    pub fn handle_event(&mut self, event: ProgressEvent) {
        if event.is_complete {
            // Stage finished — prefer the pipeline-measured elapsed time over our
            // local wall-clock to avoid race conditions when start+complete events
            // are drained in the same render cycle.
            if event.stage == PipelineStage::Complete {
                self.mark_complete(event.message, event.detail, event.summary);
            } else if let Some(idx) = self.stage_index(&event.stage) {
                self.rows[idx].status = StageStatus::Done;
                self.rows[idx].elapsed_secs = event
                    .elapsed_secs
                    .unwrap_or_else(|| self.stage_start.elapsed().as_secs_f64());
                self.detail = None;
            }
        } else {
            // Start or mid-stage update
            if let Some(idx) = self.stage_index(&event.stage) {
                if self.rows[idx].status != StageStatus::Running {
                    // Transition to running
                    self.rows[idx].status = StageStatus::Running;
                    self.current_idx = idx;
                    self.stage_start = Instant::now();
                    self.detail = event.detail;
                } else {
                    // Update detail only
                    if event.detail.is_some() {
                        self.detail = event.detail;
                    }
                }
            } else if event.stage == PipelineStage::Complete {
                self.mark_complete(event.message, event.detail, event.summary);
            }
        }
    }

    fn mark_complete(
        &mut self,
        message: String,
        detail: Option<String>,
        summary: Option<SummaryData>,
    ) {
        // Freeze the total elapsed time so it stops ticking while
        // the user reads the summary and presses Enter.
        self.final_elapsed_secs = Some(self.start_time.elapsed().as_secs_f64());

        // Mark any still-running row as done
        for row in &mut self.rows {
            if row.status == StageStatus::Running {
                row.status = StageStatus::Done;
                row.elapsed_secs = self.stage_start.elapsed().as_secs_f64();
            }
        }
        self.complete = true;
        self.summary_data = summary;
        self.summary_lines.push(message);
        if let Some(d) = detail {
            self.summary_lines.push(d);
        }
        self.detail = None;
    }

    fn stage_index(&self, stage: &PipelineStage) -> Option<usize> {
        self.rows.iter().position(|r| &r.stage == stage)
    }

    pub fn tick_spinner(&mut self) {
        self.spinner_frame = (self.spinner_frame + 1) % SPINNER_FRAMES.len();
    }

    /// Render the overlay into a frame.
    pub fn render(&self, f: &mut Frame, area: Rect) {
        let elapsed_total = self
            .final_elapsed_secs
            .unwrap_or_else(|| self.start_time.elapsed().as_secs_f64());

        // Build the content lines
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));

        for row in &self.rows {
            let (glyph, glyph_style, label_style) = match &row.status {
                StageStatus::Pending => (
                    "  ·",
                    themed(Style::default().fg(theme::MUTED)),
                    themed(Style::default().fg(theme::MUTED)),
                ),
                StageStatus::Running => {
                    let frame = SPINNER_FRAMES[self.spinner_frame];
                    (
                        // We build the glyph string for the spinner below
                        // Using a placeholder — see below
                        frame,
                        themed(Style::default().fg(theme::WARNING)),
                        themed(Style::default().fg(theme::WARNING).bold()),
                    )
                }
                StageStatus::Done => (
                    "  ✓",
                    themed(Style::default().fg(theme::SUCCESS)),
                    themed(Style::default().fg(theme::TEXT)),
                ),
            };

            match &row.status {
                StageStatus::Pending => {
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {} ", glyph.trim()), glyph_style),
                        Span::styled(row.label, label_style),
                    ]));
                }
                StageStatus::Running => {
                    let stage_elapsed = self.stage_start.elapsed().as_secs_f64();
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {} ", glyph), glyph_style),
                        Span::styled(format!("{}...", row.label), label_style),
                        Span::styled(
                            format!("  {:.1}s", stage_elapsed),
                            themed(Style::default().fg(theme::MUTED)),
                        ),
                    ]));
                    if let Some(detail) = &self.detail {
                        lines.push(Line::from(vec![
                            Span::raw("      "),
                            Span::styled(
                                detail.as_str(),
                                themed(Style::default().fg(theme::MUTED)),
                            ),
                        ]));
                    }
                }
                StageStatus::Done => {
                    lines.push(Line::from(vec![
                        Span::styled("  ✓ ", themed(Style::default().fg(theme::SUCCESS))),
                        Span::styled(row.label, label_style),
                        Span::styled(
                            format!("  {:.1}s", row.elapsed_secs),
                            themed(Style::default().fg(theme::MUTED)),
                        ),
                    ]));
                }
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  Elapsed: ", themed(Style::default().fg(theme::SUBTEXT))),
            Span::styled(
                format!("{:.1}s", elapsed_total),
                themed(Style::default().fg(theme::TEXT).bold()),
            ),
        ]));

        if self.complete {
            if let Some(ref sd) = self.summary_data {
                let total_dropped = sd.dropped_missing + sd.dropped_gini + sd.dropped_correlation;
                let pct = if sd.initial_features > 0 {
                    (total_dropped as f64 / sd.initial_features as f64) * 100.0
                } else {
                    0.0
                };

                let pct_color = if pct > 30.0 {
                    theme::SUCCESS
                } else if pct > 10.0 {
                    theme::WARNING
                } else {
                    theme::PRIMARY
                };

                // Feature count line: "100 features -> 50 remaining  (50.0% reduction)"
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {} features", sd.initial_features),
                        themed(Style::default().fg(theme::TEXT).bold()),
                    ),
                    Span::styled(" -> ", themed(Style::default().fg(theme::MUTED))),
                    Span::styled(
                        format!("{} remaining", sd.final_features),
                        themed(Style::default().fg(theme::TEXT).bold()),
                    ),
                    Span::styled(
                        format!("  ({:.1}% reduction)", pct),
                        themed(Style::default().fg(pct_color)),
                    ),
                ]));

                // Per-stage drop counts
                let drop_span = |count: usize, label: &str| -> Vec<Span> {
                    let color = if count > 0 {
                        theme::ERROR
                    } else {
                        theme::MUTED
                    };
                    vec![Span::styled(
                        format!("{} {}", count, label),
                        themed(Style::default().fg(color)),
                    )]
                };

                let mut drop_line: Vec<Span> = vec![Span::styled("  ", Style::default())];
                drop_line.extend(drop_span(sd.dropped_missing, "missing"));
                drop_line.push(Span::styled(
                    " · ",
                    themed(Style::default().fg(theme::MUTED)),
                ));
                drop_line.extend(drop_span(sd.dropped_gini, "gini"));
                drop_line.push(Span::styled(
                    " · ",
                    themed(Style::default().fg(theme::MUTED)),
                ));
                drop_line.extend(drop_span(sd.dropped_correlation, "correlation"));
                lines.push(Line::from(drop_line));

                // Output path
                if let Some(output_line) = self.summary_lines.get(1) {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![Span::styled(
                        format!("  {}", output_line),
                        themed(Style::default().fg(theme::SUBTEXT)),
                    )]));
                }
            } else {
                // Fallback: no structured summary (e.g. disconnected channel)
                lines.push(Line::from(""));
                for summary_line in &self.summary_lines {
                    lines.push(Line::from(vec![Span::styled(
                        format!("  {}", summary_line),
                        themed(Style::default().fg(theme::SUCCESS).bold()),
                    )]));
                }
            }
            lines.push(Line::from(""));
            lines.push(Line::from(vec![Span::styled(
                "  Press Esc to exit...",
                themed(Style::default().fg(theme::KEYS)),
            )]));
        }

        let title = if self.complete {
            " Pipeline Complete "
        } else {
            " Running Pipeline "
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(themed(Style::default().fg(theme::PRIMARY)))
            .title(title)
            .title_style(themed(Style::default().fg(theme::PRIMARY).bold()))
            .title_alignment(Alignment::Center);

        let inner = block.inner(area);
        f.render_widget(Clear, area);
        f.render_widget(block, area);
        f.render_widget(Paragraph::new(lines), inner);
    }
}

/// Run the progress overlay event loop.
///
/// Keeps the TUI alive while the pipeline runs in a background thread.
/// Returns when the pipeline completes and the user presses Enter (or Q).
pub fn run_progress_overlay(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    rx: ProgressReceiver,
) -> Result<()> {
    let mut overlay = ProgressOverlay::new();
    let mut last_tick = Instant::now();

    loop {
        // Drain all pending events from the pipeline thread (non-blocking)
        loop {
            match rx.try_recv() {
                Ok(ev) => overlay.handle_event(ev),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // Pipeline thread finished (or panicked); if we haven't received
                    // a Complete event yet, mark it now so the user can exit.
                    if !overlay.complete {
                        overlay.complete = true;
                        overlay.summary_lines.push("Pipeline finished.".to_string());
                    }
                    break;
                }
            }
        }

        // Tick spinner ~10 fps
        if last_tick.elapsed() >= Duration::from_millis(100) {
            overlay.tick_spinner();
            last_tick = Instant::now();
        }

        // Render
        let (cols, rows) = crossterm::terminal::size().unwrap_or((0, 0));
        let too_small = cols < MIN_COLS || rows < MIN_ROWS;

        terminal.draw(|f| {
            let area = f.area();

            if too_small {
                draw_too_small_overlay(f);
                return;
            }

            let logo_height = 9u16;
            let hint_height = 1u16;
            let box_width = 66u16;
            let box_height = 22u16.min(area.height.saturating_sub(logo_height + hint_height + 2));

            let total_height = logo_height + box_height + hint_height;
            let x = area.width.saturating_sub(box_width) / 2;
            let y = area.height.saturating_sub(total_height) / 2;

            // Logo
            let logo_area = Rect::new(x, y, box_width.min(area.width), logo_height);
            render_logo(f, logo_area);

            // Progress box
            let box_area = Rect::new(
                x,
                y + logo_height,
                box_width.min(area.width),
                box_height.max(10),
            );
            overlay.render(f, box_area);

            // Help bar
            let hint_y = y + logo_height + box_height;
            if hint_y < area.height {
                let hint_area = Rect::new(x, hint_y, box_width.min(area.width), 1);
                let hint = if overlay.complete {
                    Line::from(vec![
                        Span::styled(" Esc ", themed(Style::default().fg(theme::KEYS))),
                        Span::styled("exit", themed(Style::default().fg(theme::MUTED))),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled(" Q ", themed(Style::default().fg(theme::KEYS))),
                        Span::styled("abort pipeline", themed(Style::default().fg(theme::MUTED))),
                    ])
                };
                f.render_widget(Paragraph::new(hint).alignment(Alignment::Center), hint_area);
            }
        })?;

        // Poll for key events (short timeout to keep spinner live)
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if overlay.complete {
                    if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char(' '))
                    {
                        return Ok(());
                    }
                } else if matches!(
                    key.code,
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc
                ) {
                    overlay.abort_requested = true;
                    // We still need to drain the channel until it's disconnected
                    // so the pipeline thread doesn't hang on a full channel.
                    // Just return — the caller will check abort_requested.
                    return Ok(());
                }
            }
        }
    }
}
