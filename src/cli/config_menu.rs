//! Interactive configuration menu using ratatui
//!
//! Displays a TUI menu allowing users to review and customize
//! config parameters before running the pipeline.

use std::io::{self, stdout};
use std::path::PathBuf;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

/// Configuration values that can be customized
#[derive(Clone)]
pub struct Config {
    pub input: PathBuf,
    pub target: String,
    pub output: PathBuf,
    pub missing_threshold: f64,
    pub correlation_threshold: f64,
}

/// The current state of the menu
enum MenuState {
    Main,
    EditMissing { input: String },
    EditCorrelation { input: String },
}

/// Result of the config menu interaction
pub enum ConfigResult {
    /// User confirmed, proceed with these settings
    Proceed(Config),
    /// User quit
    Quit,
}

/// Run the interactive configuration menu
pub fn run_config_menu(config: Config) -> Result<ConfigResult> {
    // Setup terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let result = run_menu_loop(&mut terminal, config);

    // Restore terminal
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}

fn run_menu_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, mut config: Config) -> Result<ConfigResult> {
    let mut state = MenuState::Main;

    loop {
        terminal.draw(|frame| {
            draw_ui(frame, &config, &state);
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match &mut state {
                MenuState::Main => match key.code {
                    KeyCode::Enter => return Ok(ConfigResult::Proceed(config)),
                    KeyCode::Char('c') | KeyCode::Char('C') => {
                        state = MenuState::EditMissing {
                            input: format!("{:.2}", config.missing_threshold),
                        };
                    }
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                        return Ok(ConfigResult::Quit);
                    }
                    _ => {}
                },
                MenuState::EditMissing { input } => match key.code {
                    KeyCode::Enter => {
                        if let Ok(val) = input.parse::<f64>() {
                            if (0.0..=1.0).contains(&val) {
                                config.missing_threshold = val;
                            }
                        }
                        state = MenuState::EditCorrelation {
                            input: format!("{:.2}", config.correlation_threshold),
                        };
                    }
                    KeyCode::Esc => {
                        state = MenuState::Main;
                    }
                    KeyCode::Backspace => {
                        input.pop();
                    }
                    KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
                        input.push(c);
                    }
                    _ => {}
                },
                MenuState::EditCorrelation { input } => match key.code {
                    KeyCode::Enter => {
                        if let Ok(val) = input.parse::<f64>() {
                            if (0.0..=1.0).contains(&val) {
                                config.correlation_threshold = val;
                            }
                        }
                        state = MenuState::Main;
                    }
                    KeyCode::Esc => {
                        state = MenuState::Main;
                    }
                    KeyCode::Backspace => {
                        input.pop();
                    }
                    KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
                        input.push(c);
                    }
                    _ => {}
                },
            }
        }
    }
}

fn draw_ui(frame: &mut Frame, config: &Config, state: &MenuState) {
    let area = frame.area();

    // Calculate centered box dimensions
    let menu_width = 60u16;
    let menu_height = 18u16;
    let x = area.width.saturating_sub(menu_width) / 2;
    let y = area.height.saturating_sub(menu_height) / 2;

    let menu_area = Rect::new(x, y, menu_width.min(area.width), menu_height.min(area.height));

    // Clear the area behind the menu
    frame.render_widget(Clear, menu_area);

    // Main container block with gradient-like styling
    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Lo-phi Configuration ")
        .title_style(Style::default().fg(Color::Cyan).bold());

    let inner_area = outer_block.inner(menu_area);
    frame.render_widget(outer_block, menu_area);

    // Build content based on state
    let content = build_content(config, state, inner_area.width as usize);

    let paragraph = Paragraph::new(content).wrap(Wrap { trim: false });

    frame.render_widget(paragraph, inner_area);

    // Draw edit popup if in edit mode
    if let MenuState::EditMissing { input } | MenuState::EditCorrelation { input } = state {
        draw_edit_popup(frame, state, input);
    }
}

fn build_content(config: &Config, state: &MenuState, _width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![];

    // Header section
    lines.push(Line::from(""));

    // File info section
    lines.push(Line::from(vec![
        Span::styled("  Input:  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            config.input.display().to_string(),
            Style::default().fg(Color::White),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Target: ", Style::default().fg(Color::DarkGray)),
        Span::styled(config.target.clone(), Style::default().fg(Color::White)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Output: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            config.output.display().to_string(),
            Style::default().fg(Color::White),
        ),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  ─────────────────────────────────────────────",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    // Threshold section with highlighting based on state
    let missing_style = match state {
        MenuState::EditMissing { .. } => Style::default().fg(Color::Yellow).bold(),
        _ => Style::default().fg(Color::Green),
    };
    let corr_style = match state {
        MenuState::EditCorrelation { .. } => Style::default().fg(Color::Yellow).bold(),
        _ => Style::default().fg(Color::Green),
    };

    lines.push(Line::from(vec![
        Span::styled("  Missing Threshold:     ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:.2}", config.missing_threshold),
            missing_style,
        ),
        Span::styled(
            format!(" ({:.0}%)", config.missing_threshold * 100.0),
            Style::default().fg(Color::DarkGray),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Correlation Threshold: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:.2}", config.correlation_threshold), corr_style),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  ─────────────────────────────────────────────",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    // Controls section
    lines.push(Line::from(vec![
        Span::styled("  [", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", Style::default().fg(Color::Cyan).bold()),
        Span::styled("] Run with these settings", Style::default().fg(Color::White)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  [", Style::default().fg(Color::DarkGray)),
        Span::styled("C", Style::default().fg(Color::Cyan).bold()),
        Span::styled("] Customize parameters", Style::default().fg(Color::White)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  [", Style::default().fg(Color::DarkGray)),
        Span::styled("Q", Style::default().fg(Color::Cyan).bold()),
        Span::styled("] Quit", Style::default().fg(Color::White)),
    ]));

    lines
}

fn draw_edit_popup(frame: &mut Frame, state: &MenuState, input: &str) {
    let area = frame.area();

    let popup_width = 45u16;
    let popup_height = 7u16;
    let x = area.width.saturating_sub(popup_width) / 2;
    let y = area.height.saturating_sub(popup_height) / 2;

    let popup_area = Rect::new(x, y, popup_width.min(area.width), popup_height.min(area.height));

    frame.render_widget(Clear, popup_area);

    let title = match state {
        MenuState::EditMissing { .. } => " Missing Threshold ",
        MenuState::EditCorrelation { .. } => " Correlation Threshold ",
        _ => "",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(title)
        .title_style(Style::default().fg(Color::Yellow).bold());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let content = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Value: ", Style::default().fg(Color::DarkGray)),
            Span::styled(input.to_string(), Style::default().fg(Color::White).bold()),
            Span::styled("▌", Style::default().fg(Color::Yellow)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Enter", Style::default().fg(Color::Cyan)),
            Span::styled(" to confirm, ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled(" to cancel", Style::default().fg(Color::DarkGray)),
        ]),
    ];

    let paragraph = Paragraph::new(content);
    frame.render_widget(paragraph, inner);
}

