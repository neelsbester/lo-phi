//! Interactive configuration menu using ratatui
//!
//! Displays a TUI menu allowing users to review and customize
//! config parameters before running the pipeline.

use std::collections::HashSet;
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
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::pipeline::TargetMapping;

/// Configuration values that can be customized
#[derive(Clone)]
pub struct Config {
    pub input: PathBuf,
    pub target: Option<String>,
    pub output: PathBuf,
    pub missing_threshold: f64,
    pub gini_threshold: f64,
    pub correlation_threshold: f64,
    pub columns_to_drop: Vec<String>,
    /// Optional mapping for non-binary target columns
    pub target_mapping: Option<TargetMapping>,
    /// Optional column containing sample weights
    pub weight_column: Option<String>,
}

/// The current state of the menu
enum MenuState {
    Main,
    SelectTarget {
        search: String,
        columns: Vec<String>,
        filtered: Vec<usize>,
        selected: usize,
    },
    SelectColumnsToDrop {
        search: String,
        columns: Vec<String>,
        filtered: Vec<usize>,
        selected: usize,
        checked: HashSet<usize>,
    },
    /// Select which value represents EVENT (1)
    SelectEventValue {
        unique_values: Vec<String>,
        selected: usize,
    },
    /// Select which value represents NON-EVENT (0)
    SelectNonEventValue {
        unique_values: Vec<String>,
        selected: usize,
        event_value: String,
    },
    EditMissing {
        input: String,
    },
    EditGini {
        input: String,
    },
    EditCorrelation {
        input: String,
    },
}

/// Result of the config menu interaction
pub enum ConfigResult {
    /// User confirmed, proceed with these settings
    Proceed(Config),
    /// User quit
    Quit,
}

/// Run the interactive configuration menu
pub fn run_config_menu(config: Config, columns: Vec<String>) -> Result<ConfigResult> {
    // Setup terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let result = run_menu_loop(&mut terminal, config, columns);

    // Restore terminal
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}

/// Result of target mapping selection
pub enum TargetMappingResult {
    /// User selected event and non-event values
    Selected(TargetMapping),
    /// User cancelled selection
    Cancelled,
}

/// Run target mapping selector as a standalone TUI
///
/// This is called from main.rs after loading data and analyzing the target column
/// when the target column is not already binary 0/1.
pub fn run_target_mapping_selector(unique_values: Vec<String>) -> Result<TargetMappingResult> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let result = run_mapping_loop(&mut terminal, unique_values);

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}

fn run_mapping_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    unique_values: Vec<String>,
) -> Result<TargetMappingResult> {
    let mut state = MappingState::SelectEvent {
        unique_values: unique_values.clone(),
        selected: 0,
    };

    loop {
        terminal.draw(|frame| {
            draw_mapping_ui(frame, &state);
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match &mut state {
                MappingState::SelectEvent {
                    unique_values,
                    selected,
                } => match key.code {
                    KeyCode::Enter => {
                        if !unique_values.is_empty() {
                            let event_value = unique_values[*selected].clone();
                            let remaining: Vec<String> = unique_values
                                .iter()
                                .filter(|v| *v != &event_value)
                                .cloned()
                                .collect();
                            state = MappingState::SelectNonEvent {
                                unique_values: remaining,
                                selected: 0,
                                event_value,
                            };
                        }
                    }
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                        return Ok(TargetMappingResult::Cancelled);
                    }
                    KeyCode::Up => {
                        if *selected > 0 {
                            *selected -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if *selected + 1 < unique_values.len() {
                            *selected += 1;
                        }
                    }
                    _ => {}
                },
                MappingState::SelectNonEvent {
                    unique_values,
                    selected,
                    event_value,
                } => match key.code {
                    KeyCode::Enter => {
                        if !unique_values.is_empty() {
                            let non_event_value = unique_values[*selected].clone();
                            return Ok(TargetMappingResult::Selected(TargetMapping::new(
                                event_value.clone(),
                                non_event_value,
                            )));
                        }
                    }
                    KeyCode::Esc => {
                        // Go back to event selection
                        let mut all_values = unique_values.clone();
                        all_values.push(event_value.clone());
                        all_values.sort();
                        state = MappingState::SelectEvent {
                            unique_values: all_values,
                            selected: 0,
                        };
                    }
                    KeyCode::Up => {
                        if *selected > 0 {
                            *selected -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if *selected + 1 < unique_values.len() {
                            *selected += 1;
                        }
                    }
                    _ => {}
                },
            }
        }
    }
}

/// Internal state for the standalone mapping selector
enum MappingState {
    SelectEvent {
        unique_values: Vec<String>,
        selected: usize,
    },
    SelectNonEvent {
        unique_values: Vec<String>,
        selected: usize,
        event_value: String,
    },
}

fn draw_mapping_ui(frame: &mut Frame, state: &MappingState) {
    let area = frame.area();

    // Draw a centered info box first
    let info_width = 55u16;
    let info_height = 5u16;
    let info_x = area.width.saturating_sub(info_width) / 2;
    let info_y = area.height.saturating_sub(25) / 2;

    let info_area = Rect::new(info_x, info_y, info_width.min(area.width), info_height);

    frame.render_widget(Clear, info_area);

    let info_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Target Mapping Required ")
        .title_style(Style::default().fg(Color::Cyan).bold());

    let info_content = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Target column is not binary (0/1).",
            Style::default().fg(Color::White),
        )]),
        Line::from(vec![Span::styled(
            "  Please select event and non-event values.",
            Style::default().fg(Color::DarkGray),
        )]),
    ])
    .block(info_block);

    frame.render_widget(info_content, info_area);

    // Draw the selector below the info box
    match state {
        MappingState::SelectEvent {
            unique_values,
            selected,
        } => {
            draw_standalone_event_selector(
                frame,
                unique_values,
                *selected,
                info_y + info_height + 1,
            );
        }
        MappingState::SelectNonEvent {
            unique_values,
            selected,
            event_value,
        } => {
            draw_standalone_non_event_selector(
                frame,
                unique_values,
                *selected,
                event_value,
                info_y + info_height + 1,
            );
        }
    }
}

fn draw_standalone_event_selector(
    frame: &mut Frame,
    unique_values: &[String],
    selected: usize,
    y_offset: u16,
) {
    let area = frame.area();

    let popup_width = 50u16;
    let popup_height = 16u16;
    let x = area.width.saturating_sub(popup_width) / 2;

    let popup_area = Rect::new(
        x,
        y_offset,
        popup_width.min(area.width),
        popup_height.min(area.height.saturating_sub(y_offset)),
    );

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .title(" Select EVENT Value (1) ")
        .title_style(Style::default().fg(Color::Green).bold());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(inner);

    let desc = Paragraph::new(vec![Line::from(vec![
        Span::styled(
            "  Select the value that represents ",
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled("EVENT (1)", Style::default().fg(Color::Green).bold()),
    ])]);
    frame.render_widget(desc, chunks[0]);

    let max_visible = (chunks[1].height as usize).saturating_sub(0);
    let start_idx = if selected >= max_visible {
        selected - max_visible + 1
    } else {
        0
    };

    let items: Vec<ListItem> = unique_values
        .iter()
        .enumerate()
        .skip(start_idx)
        .take(max_visible)
        .map(|(i, value)| {
            let style = if i == selected {
                Style::default().fg(Color::Black).bg(Color::Green).bold()
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("  {}", value)).style(style)
        })
        .collect();

    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(selected.saturating_sub(start_idx)));
    frame.render_stateful_widget(list, chunks[1], &mut list_state);

    let help_text = Line::from(vec![
        Span::styled("  Enter", Style::default().fg(Color::Cyan)),
        Span::styled(" select  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc/Q", Style::default().fg(Color::Cyan)),
        Span::styled(" cancel", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(help_text), chunks[2]);
}

fn draw_standalone_non_event_selector(
    frame: &mut Frame,
    unique_values: &[String],
    selected: usize,
    event_value: &str,
    y_offset: u16,
) {
    let area = frame.area();

    let popup_width = 50u16;
    let popup_height = 16u16;
    let x = area.width.saturating_sub(popup_width) / 2;

    let popup_area = Rect::new(
        x,
        y_offset,
        popup_width.min(area.width),
        popup_height.min(area.height.saturating_sub(y_offset)),
    );

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Select NON-EVENT Value (0) ")
        .title_style(Style::default().fg(Color::Yellow).bold());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(inner);

    let desc = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("  Event (1): ", Style::default().fg(Color::DarkGray)),
            Span::styled(event_value, Style::default().fg(Color::Green).bold()),
        ]),
        Line::from(vec![
            Span::styled(
                "  Select the value for ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("NON-EVENT (0)", Style::default().fg(Color::Yellow).bold()),
        ]),
    ]);
    frame.render_widget(desc, chunks[0]);

    let max_visible = (chunks[1].height as usize).saturating_sub(0);
    let start_idx = if selected >= max_visible {
        selected - max_visible + 1
    } else {
        0
    };

    let items: Vec<ListItem> = unique_values
        .iter()
        .enumerate()
        .skip(start_idx)
        .take(max_visible)
        .map(|(i, value)| {
            let style = if i == selected {
                Style::default().fg(Color::Black).bg(Color::Yellow).bold()
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("  {}", value)).style(style)
        })
        .collect();

    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(selected.saturating_sub(start_idx)));
    frame.render_stateful_widget(list, chunks[1], &mut list_state);

    let help_text = Line::from(vec![
        Span::styled("  Enter", Style::default().fg(Color::Cyan)),
        Span::styled(" select  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::styled(" back", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(help_text), chunks[2]);
}

fn run_menu_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut config: Config,
    columns: Vec<String>,
) -> Result<ConfigResult> {
    let mut state = MenuState::Main;

    loop {
        terminal.draw(|frame| {
            draw_ui(frame, &config, &state, &columns);
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match &mut state {
                MenuState::Main => match key.code {
                    KeyCode::Enter => {
                        // Only proceed if target is selected
                        if config.target.is_some() {
                            return Ok(ConfigResult::Proceed(config));
                        }
                    }
                    KeyCode::Char('t') | KeyCode::Char('T') => {
                        let filtered: Vec<usize> = (0..columns.len()).collect();
                        state = MenuState::SelectTarget {
                            search: String::new(),
                            columns: columns.clone(),
                            filtered,
                            selected: 0,
                        };
                    }
                    KeyCode::Char('d') | KeyCode::Char('D') => {
                        let filtered: Vec<usize> = (0..columns.len()).collect();
                        // Pre-check columns that are already marked for dropping
                        let checked: HashSet<usize> = columns
                            .iter()
                            .enumerate()
                            .filter(|(_, col)| config.columns_to_drop.contains(col))
                            .map(|(i, _)| i)
                            .collect();
                        state = MenuState::SelectColumnsToDrop {
                            search: String::new(),
                            columns: columns.clone(),
                            filtered,
                            selected: 0,
                            checked,
                        };
                    }
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
                MenuState::SelectTarget {
                    search,
                    columns,
                    filtered,
                    selected,
                } => match key.code {
                    KeyCode::Enter => {
                        if !filtered.is_empty() {
                            let idx = filtered[*selected];
                            config.target = Some(columns[idx].clone());
                        }
                        state = MenuState::Main;
                    }
                    KeyCode::Esc => {
                        state = MenuState::Main;
                    }
                    KeyCode::Up => {
                        if *selected > 0 {
                            *selected -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if *selected + 1 < filtered.len() {
                            *selected += 1;
                        }
                    }
                    KeyCode::Backspace => {
                        search.pop();
                        update_filtered(search, columns, filtered);
                        *selected = 0;
                    }
                    KeyCode::Char(c) => {
                        search.push(c);
                        update_filtered(search, columns, filtered);
                        *selected = 0;
                    }
                    _ => {}
                },
                MenuState::SelectColumnsToDrop {
                    search,
                    columns,
                    filtered,
                    selected,
                    checked,
                } => match key.code {
                    KeyCode::Enter => {
                        // Confirm selection - convert checked indices to column names
                        config.columns_to_drop =
                            checked.iter().map(|&idx| columns[idx].clone()).collect();
                        state = MenuState::Main;
                    }
                    KeyCode::Esc => {
                        // Cancel - discard changes
                        state = MenuState::Main;
                    }
                    KeyCode::Char(' ') => {
                        // Toggle selection of current item
                        if !filtered.is_empty() {
                            let idx = filtered[*selected];
                            if checked.contains(&idx) {
                                checked.remove(&idx);
                            } else {
                                checked.insert(idx);
                            }
                        }
                    }
                    KeyCode::Up => {
                        if *selected > 0 {
                            *selected -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if *selected + 1 < filtered.len() {
                            *selected += 1;
                        }
                    }
                    KeyCode::Backspace => {
                        search.pop();
                        update_filtered(search, columns, filtered);
                        *selected = 0;
                    }
                    KeyCode::Char(c) => {
                        search.push(c);
                        update_filtered(search, columns, filtered);
                        *selected = 0;
                    }
                    _ => {}
                },
                MenuState::SelectEventValue {
                    unique_values,
                    selected,
                } => match key.code {
                    KeyCode::Enter => {
                        if !unique_values.is_empty() {
                            let event_value = unique_values[*selected].clone();
                            // Move to non-event selection, excluding the chosen event value
                            let remaining: Vec<String> = unique_values
                                .iter()
                                .filter(|v| *v != &event_value)
                                .cloned()
                                .collect();
                            state = MenuState::SelectNonEventValue {
                                unique_values: remaining,
                                selected: 0,
                                event_value,
                            };
                        }
                    }
                    KeyCode::Esc => {
                        // Cancel - clear target mapping and go back to main
                        config.target_mapping = None;
                        state = MenuState::Main;
                    }
                    KeyCode::Up => {
                        if *selected > 0 {
                            *selected -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if *selected + 1 < unique_values.len() {
                            *selected += 1;
                        }
                    }
                    _ => {}
                },
                MenuState::SelectNonEventValue {
                    unique_values,
                    selected,
                    event_value,
                } => match key.code {
                    KeyCode::Enter => {
                        if !unique_values.is_empty() {
                            let non_event_value = unique_values[*selected].clone();
                            // Create the target mapping
                            config.target_mapping =
                                Some(TargetMapping::new(event_value.clone(), non_event_value));
                            state = MenuState::Main;
                        }
                    }
                    KeyCode::Esc => {
                        // Go back to event selection
                        let mut all_values = unique_values.clone();
                        all_values.push(event_value.clone());
                        all_values.sort();
                        state = MenuState::SelectEventValue {
                            unique_values: all_values,
                            selected: 0,
                        };
                    }
                    KeyCode::Up => {
                        if *selected > 0 {
                            *selected -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if *selected + 1 < unique_values.len() {
                            *selected += 1;
                        }
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
                        state = MenuState::EditGini {
                            input: format!("{:.2}", config.gini_threshold),
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
                MenuState::EditGini { input } => match key.code {
                    KeyCode::Enter => {
                        if let Ok(val) = input.parse::<f64>() {
                            if (0.0..=1.0).contains(&val) {
                                config.gini_threshold = val;
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

/// Update filtered indices based on search query (case-insensitive fuzzy match)
fn update_filtered(search: &str, columns: &[String], filtered: &mut Vec<usize>) {
    let search_lower = search.to_lowercase();
    filtered.clear();
    for (i, col) in columns.iter().enumerate() {
        if col.to_lowercase().contains(&search_lower) {
            filtered.push(i);
        }
    }
}

/// Truncate a path string from the start to fit within max_len characters
/// Returns "...rest/of/path" style truncation
fn truncate_path_start(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }
    if max_len <= 3 {
        return "...".to_string();
    }
    let truncate_to = max_len - 3; // Account for "..."
    let start_idx = path.len() - truncate_to;
    format!("...{}", &path[start_idx..])
}

fn draw_ui(frame: &mut Frame, config: &Config, state: &MenuState, _columns: &[String]) {
    let area = frame.area();

    // ASCII logo for Lo-phi
    let logo_lines = vec![
        Line::from(Span::styled(
            "██╗      ██████╗       ██████╗ ██╗  ██╗██╗",
            Style::default().fg(Color::Cyan).bold(),
        )),
        Line::from(Span::styled(
            "██║     ██╔═══██╗      ██╔══██╗██║  ██║██║",
            Style::default().fg(Color::Cyan).bold(),
        )),
        Line::from(Span::styled(
            "██║     ██║   ██║█████╗██████╔╝███████║██║",
            Style::default().fg(Color::Cyan).bold(),
        )),
        Line::from(Span::styled(
            "██║     ██║   ██║╚════╝██╔═══╝ ██╔══██║██║",
            Style::default().fg(Color::Cyan).bold(),
        )),
        Line::from(Span::styled(
            "███████╗╚██████╔╝      ██║     ██║  ██║██║",
            Style::default().fg(Color::Cyan).bold(),
        )),
        Line::from(Span::styled(
            "╚══════╝ ╚═════╝       ╚═╝     ╚═╝  ╚═╝╚═╝",
            Style::default().fg(Color::Cyan).bold(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("φ ", Style::default().fg(Color::Magenta).bold()),
            Span::styled(
                "Feature Reduction as simple as phi",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ];
    let logo_height = 9u16; // 6 logo lines + 1 empty + 1 subtitle + 1 spacing

    // Calculate centered box dimensions - wider box (66 chars, ~10% wider than 60)
    let menu_width = 66u16;
    // Dynamic height: use minimum needed (22) or available space, whichever is smaller
    let ideal_height = 22u16;
    let menu_height = ideal_height.min(area.height.saturating_sub(logo_height + 2)); // Leave room for logo

    // Total height needed: logo + menu
    let total_height = logo_height + menu_height;
    let x = area.width.saturating_sub(menu_width) / 2;
    let y = area.height.saturating_sub(total_height) / 2;

    // Draw logo above the menu (centered)
    let logo_width = 43u16; // Width of the ASCII art
    let logo_x = area.width.saturating_sub(logo_width) / 2;
    let logo_area = Rect::new(logo_x, y, logo_width.min(area.width), logo_height);
    let logo_paragraph = Paragraph::new(logo_lines).alignment(Alignment::Center);
    frame.render_widget(logo_paragraph, logo_area);

    // Menu area positioned below the logo
    let menu_y = y + logo_height;
    let menu_area = Rect::new(x, menu_y, menu_width.min(area.width), menu_height.max(10)); // Min 10 rows

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

    // Build content based on state with adaptive sizing
    let content = build_content(
        config,
        state,
        inner_area.width as usize,
        inner_area.height as usize,
    );

    let paragraph = Paragraph::new(content).wrap(Wrap { trim: false });

    frame.render_widget(paragraph, inner_area);

    // Draw popup based on current state
    match state {
        MenuState::SelectTarget {
            search,
            columns,
            filtered,
            selected,
        } => {
            draw_target_selector(frame, search, columns, filtered, *selected);
        }
        MenuState::SelectColumnsToDrop {
            search,
            columns,
            filtered,
            selected,
            checked,
        } => {
            draw_columns_to_drop_selector(frame, search, columns, filtered, *selected, checked);
        }
        MenuState::SelectEventValue {
            unique_values,
            selected,
        } => {
            draw_event_value_selector(frame, unique_values, *selected);
        }
        MenuState::SelectNonEventValue {
            unique_values,
            selected,
            event_value,
        } => {
            draw_non_event_value_selector(frame, unique_values, *selected, event_value);
        }
        MenuState::EditMissing { input }
        | MenuState::EditGini { input }
        | MenuState::EditCorrelation { input } => {
            draw_edit_popup(frame, state, input);
        }
        MenuState::Main => {}
    }
}

fn build_content(
    config: &Config,
    state: &MenuState,
    width: usize,
    height: usize,
) -> Vec<Line<'static>> {
    let mut lines = vec![];

    // Calculate max path length (width minus label and padding)
    // Label is "  Input:  " = 10 chars, plus 2 for border = 12, plus some margin
    let max_path_len = width.saturating_sub(14);

    // Header section - skip if very tight on space
    if height >= 18 {
        lines.push(Line::from(""));
    }

    // File info section with truncated paths
    let input_path = truncate_path_start(&config.input.display().to_string(), max_path_len);
    lines.push(Line::from(vec![
        Span::styled("  Input:  ", Style::default().fg(Color::DarkGray)),
        Span::styled(input_path, Style::default().fg(Color::White)),
    ]));

    // Target with highlighting if not selected
    let target_display = config
        .target
        .clone()
        .unwrap_or_else(|| "⚠ Not selected".to_string());
    let target_style = if config.target.is_some() {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::Yellow).bold()
    };
    lines.push(Line::from(vec![
        Span::styled("  Target: ", Style::default().fg(Color::DarkGray)),
        Span::styled(target_display, target_style),
    ]));

    // Show target mapping if configured
    if let Some(mapping) = &config.target_mapping {
        lines.push(Line::from(vec![
            Span::styled("          ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(
                    "→ {} = 1, {} = 0",
                    mapping.event_value, mapping.non_event_value
                ),
                Style::default().fg(Color::DarkGray).italic(),
            ),
        ]));
    }

    let output_path = truncate_path_start(&config.output.display().to_string(), max_path_len);
    lines.push(Line::from(vec![
        Span::styled("  Output: ", Style::default().fg(Color::DarkGray)),
        Span::styled(output_path, Style::default().fg(Color::White)),
    ]));

    // Show columns to drop count
    let drop_display = if config.columns_to_drop.is_empty() {
        "None".to_string()
    } else {
        format!("{} column(s) selected", config.columns_to_drop.len())
    };
    let drop_style = if config.columns_to_drop.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Red)
    };
    lines.push(Line::from(vec![
        Span::styled("  Drop:   ", Style::default().fg(Color::DarkGray)),
        Span::styled(drop_display, drop_style),
    ]));

    // Separator - use shorter one for compact mode
    if height >= 16 {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  ───────────────────────────────────────────────────",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
    } else {
        lines.push(Line::from(Span::styled(
            "  ─────────────────────────────────",
            Style::default().fg(Color::DarkGray),
        )));
    }

    // Threshold section with highlighting based on state
    let missing_style = match state {
        MenuState::EditMissing { .. } => Style::default().fg(Color::Yellow).bold(),
        _ => Style::default().fg(Color::Green),
    };
    let gini_style = match state {
        MenuState::EditGini { .. } => Style::default().fg(Color::Yellow).bold(),
        _ => Style::default().fg(Color::Green),
    };
    let corr_style = match state {
        MenuState::EditCorrelation { .. } => Style::default().fg(Color::Yellow).bold(),
        _ => Style::default().fg(Color::Green),
    };

    lines.push(Line::from(vec![
        Span::styled(
            "  Missing Threshold:     ",
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(format!("{:.2}", config.missing_threshold), missing_style),
        Span::styled(
            format!(" ({:.0}%)", config.missing_threshold * 100.0),
            Style::default().fg(Color::DarkGray),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled(
            "  Gini Threshold:        ",
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(format!("{:.2}", config.gini_threshold), gini_style),
    ]));
    lines.push(Line::from(vec![
        Span::styled(
            "  Correlation Threshold: ",
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(format!("{:.2}", config.correlation_threshold), corr_style),
    ]));

    // Second separator - adaptive
    if height >= 16 {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  ───────────────────────────────────────────────────",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
    } else {
        lines.push(Line::from(Span::styled(
            "  ─────────────────────────────────",
            Style::default().fg(Color::DarkGray),
        )));
    }

    // Controls section
    let enter_style = if config.target.is_some() {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };
    lines.push(Line::from(vec![
        Span::styled("  [", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", enter_style),
        Span::styled(
            "] Run with these settings",
            if config.target.is_some() {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  [", Style::default().fg(Color::DarkGray)),
        Span::styled("T", Style::default().fg(Color::Cyan).bold()),
        Span::styled("] Select target column", Style::default().fg(Color::White)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  [", Style::default().fg(Color::DarkGray)),
        Span::styled("D", Style::default().fg(Color::Cyan).bold()),
        Span::styled(
            "] Select columns to drop",
            Style::default().fg(Color::White),
        ),
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

fn draw_target_selector(
    frame: &mut Frame,
    search: &str,
    columns: &[String],
    filtered: &[usize],
    selected: usize,
) {
    let area = frame.area();

    let popup_width = 50u16;
    let popup_height = 18u16;
    let x = area.width.saturating_sub(popup_width) / 2;
    let y = area.height.saturating_sub(popup_height) / 2;

    let popup_area = Rect::new(
        x,
        y,
        popup_width.min(area.width),
        popup_height.min(area.height),
    );

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta))
        .title(" Select Target Column ")
        .title_style(Style::default().fg(Color::Magenta).bold());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Split inner area into search box and list
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(inner);

    // Search box
    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Search ")
        .title_style(Style::default().fg(Color::DarkGray));

    let search_text = format!("{}", search);
    let search_para = Paragraph::new(Line::from(vec![
        Span::styled(search_text, Style::default().fg(Color::White)),
        Span::styled("▌", Style::default().fg(Color::Magenta)),
    ]))
    .block(search_block);

    frame.render_widget(search_para, chunks[0]);

    // Column list with visible window
    let max_visible = (chunks[1].height as usize).saturating_sub(0);
    let start_idx = if selected >= max_visible {
        selected - max_visible + 1
    } else {
        0
    };

    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .skip(start_idx)
        .take(max_visible)
        .map(|(i, &col_idx)| {
            let col_name = &columns[col_idx];
            let style = if i == selected {
                Style::default().fg(Color::Black).bg(Color::Magenta).bold()
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("  {}", col_name)).style(style)
        })
        .collect();

    let list = List::new(items);

    // Use ListState to track selection for scrolling
    let mut list_state = ListState::default();
    list_state.select(Some(selected.saturating_sub(start_idx)));

    frame.render_stateful_widget(list, chunks[1], &mut list_state);

    // Show count indicator at bottom
    if !filtered.is_empty() {
        let count_text = format!(" {}/{} columns ", selected + 1, filtered.len());
        let text_len = count_text.len();
        let count_span = Span::styled(count_text, Style::default().fg(Color::DarkGray));
        let count_area = Rect::new(
            popup_area.x + popup_area.width - text_len as u16 - 1,
            popup_area.y + popup_area.height - 1,
            text_len as u16,
            1,
        );
        frame.render_widget(Paragraph::new(count_span), count_area);
    }
}

fn draw_columns_to_drop_selector(
    frame: &mut Frame,
    search: &str,
    columns: &[String],
    filtered: &[usize],
    selected: usize,
    checked: &HashSet<usize>,
) {
    let area = frame.area();

    let popup_width = 55u16;
    let popup_height = 20u16;
    let x = area.width.saturating_sub(popup_width) / 2;
    let y = area.height.saturating_sub(popup_height) / 2;

    let popup_area = Rect::new(
        x,
        y,
        popup_width.min(area.width),
        popup_height.min(area.height),
    );

    frame.render_widget(Clear, popup_area);

    let title = format!(" Select Columns to Drop ({} selected) ", checked.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red))
        .title(title)
        .title_style(Style::default().fg(Color::Red).bold());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Split inner area into search box, list, and help text
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(inner);

    // Search box
    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Search ")
        .title_style(Style::default().fg(Color::DarkGray));

    let search_text = format!("{}", search);
    let search_para = Paragraph::new(Line::from(vec![
        Span::styled(search_text, Style::default().fg(Color::White)),
        Span::styled("▌", Style::default().fg(Color::Red)),
    ]))
    .block(search_block);

    frame.render_widget(search_para, chunks[0]);

    // Column list with visible window
    let max_visible = (chunks[1].height as usize).saturating_sub(0);
    let start_idx = if selected >= max_visible {
        selected - max_visible + 1
    } else {
        0
    };

    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .skip(start_idx)
        .take(max_visible)
        .map(|(i, &col_idx)| {
            let col_name = &columns[col_idx];
            let is_checked = checked.contains(&col_idx);
            let checkbox = if is_checked { "[x]" } else { "[ ]" };

            let style = if i == selected {
                Style::default().fg(Color::Black).bg(Color::Red).bold()
            } else if is_checked {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("  {} {}", checkbox, col_name)).style(style)
        })
        .collect();

    let list = List::new(items);

    // Use ListState to track selection for scrolling
    let mut list_state = ListState::default();
    list_state.select(Some(selected.saturating_sub(start_idx)));

    frame.render_stateful_widget(list, chunks[1], &mut list_state);

    // Help text at bottom
    let help_text = Line::from(vec![
        Span::styled("  Space", Style::default().fg(Color::Cyan)),
        Span::styled(" toggle  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", Style::default().fg(Color::Cyan)),
        Span::styled(" confirm  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::styled(" cancel", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(help_text), chunks[2]);

    // Show count indicator at bottom right of popup
    if !filtered.is_empty() {
        let count_text = format!(" {}/{} columns ", selected + 1, filtered.len());
        let text_len = count_text.len();
        let count_span = Span::styled(count_text, Style::default().fg(Color::DarkGray));
        let count_area = Rect::new(
            popup_area.x + popup_area.width - text_len as u16 - 1,
            popup_area.y + popup_area.height - 1,
            text_len as u16,
            1,
        );
        frame.render_widget(Paragraph::new(count_span), count_area);
    }
}

fn draw_edit_popup(frame: &mut Frame, state: &MenuState, input: &str) {
    let area = frame.area();

    let popup_width = 45u16;
    let popup_height = 7u16;
    let x = area.width.saturating_sub(popup_width) / 2;
    let y = area.height.saturating_sub(popup_height) / 2;

    let popup_area = Rect::new(
        x,
        y,
        popup_width.min(area.width),
        popup_height.min(area.height),
    );

    frame.render_widget(Clear, popup_area);

    let title = match state {
        MenuState::EditMissing { .. } => " Missing Threshold ",
        MenuState::EditGini { .. } => " Gini Threshold ",
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

/// Draw the event value selector popup
fn draw_event_value_selector(frame: &mut Frame, unique_values: &[String], selected: usize) {
    let area = frame.area();

    let popup_width = 50u16;
    let popup_height = 16u16;
    let x = area.width.saturating_sub(popup_width) / 2;
    let y = area.height.saturating_sub(popup_height) / 2;

    let popup_area = Rect::new(
        x,
        y,
        popup_width.min(area.width),
        popup_height.min(area.height),
    );

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .title(" Select EVENT Value (1) ")
        .title_style(Style::default().fg(Color::Green).bold());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Split inner area into description and list
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(inner);

    // Description
    let desc = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  Select the value that represents ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("EVENT (1)", Style::default().fg(Color::Green).bold()),
        ]),
    ]);
    frame.render_widget(desc, chunks[0]);

    // Value list
    let max_visible = (chunks[1].height as usize).saturating_sub(0);
    let start_idx = if selected >= max_visible {
        selected - max_visible + 1
    } else {
        0
    };

    let items: Vec<ListItem> = unique_values
        .iter()
        .enumerate()
        .skip(start_idx)
        .take(max_visible)
        .map(|(i, value)| {
            let style = if i == selected {
                Style::default().fg(Color::Black).bg(Color::Green).bold()
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("  {}", value)).style(style)
        })
        .collect();

    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(selected.saturating_sub(start_idx)));
    frame.render_stateful_widget(list, chunks[1], &mut list_state);

    // Help text
    let help_text = Line::from(vec![
        Span::styled("  Enter", Style::default().fg(Color::Cyan)),
        Span::styled(" select  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::styled(" cancel", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(help_text), chunks[2]);

    // Count indicator
    if !unique_values.is_empty() {
        let count_text = format!(" {}/{} ", selected + 1, unique_values.len());
        let text_len = count_text.len();
        let count_span = Span::styled(count_text, Style::default().fg(Color::DarkGray));
        let count_area = Rect::new(
            popup_area.x + popup_area.width - text_len as u16 - 1,
            popup_area.y + popup_area.height - 1,
            text_len as u16,
            1,
        );
        frame.render_widget(Paragraph::new(count_span), count_area);
    }
}

/// Draw the non-event value selector popup
fn draw_non_event_value_selector(
    frame: &mut Frame,
    unique_values: &[String],
    selected: usize,
    event_value: &str,
) {
    let area = frame.area();

    let popup_width = 50u16;
    let popup_height = 16u16;
    let x = area.width.saturating_sub(popup_width) / 2;
    let y = area.height.saturating_sub(popup_height) / 2;

    let popup_area = Rect::new(
        x,
        y,
        popup_width.min(area.width),
        popup_height.min(area.height),
    );

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Select NON-EVENT Value (0) ")
        .title_style(Style::default().fg(Color::Yellow).bold());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Split inner area into description and list
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(inner);

    // Description showing the already selected event value
    let desc = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Event (1): ", Style::default().fg(Color::DarkGray)),
            Span::styled(event_value, Style::default().fg(Color::Green).bold()),
        ]),
        Line::from(vec![
            Span::styled(
                "  Select the value for ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("NON-EVENT (0)", Style::default().fg(Color::Yellow).bold()),
        ]),
    ]);
    frame.render_widget(desc, chunks[0]);

    // Value list
    let max_visible = (chunks[1].height as usize).saturating_sub(0);
    let start_idx = if selected >= max_visible {
        selected - max_visible + 1
    } else {
        0
    };

    let items: Vec<ListItem> = unique_values
        .iter()
        .enumerate()
        .skip(start_idx)
        .take(max_visible)
        .map(|(i, value)| {
            let style = if i == selected {
                Style::default().fg(Color::Black).bg(Color::Yellow).bold()
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("  {}", value)).style(style)
        })
        .collect();

    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(selected.saturating_sub(start_idx)));
    frame.render_stateful_widget(list, chunks[1], &mut list_state);

    // Help text
    let help_text = Line::from(vec![
        Span::styled("  Enter", Style::default().fg(Color::Cyan)),
        Span::styled(" select  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::styled(" back", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(help_text), chunks[2]);

    // Count indicator
    if !unique_values.is_empty() {
        let count_text = format!(" {}/{} ", selected + 1, unique_values.len());
        let text_len = count_text.len();
        let count_span = Span::styled(count_text, Style::default().fg(Color::DarkGray));
        let count_area = Rect::new(
            popup_area.x + popup_area.width - text_len as u16 - 1,
            popup_area.y + popup_area.height - 1,
            text_len as u16,
            1,
        );
        frame.render_widget(Paragraph::new(count_span), count_area);
    }
}
