//! Shared UI utilities for CLI modules
//!
//! Contains reusable rendering helpers used across both the wizard and the
//! dashboard configuration menu.

use std::sync::OnceLock;

use ratatui::{
    layout::{Alignment, Rect},
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::theme;

/// Minimum terminal dimensions required to render the TUI correctly.
pub const MIN_COLS: u16 = 80;
pub const MIN_ROWS: u16 = 24;

static NO_COLOR: OnceLock<bool> = OnceLock::new();

/// Returns true when color output should be suppressed.
///
/// Checks for:
/// - `NO_COLOR` env var (any value, per <https://no-color.org>)
/// - `TERM=dumb` env var (terminals that cannot render ANSI sequences)
///
/// The result is cached via [`OnceLock`] so the environment is only read once.
pub fn no_color_mode() -> bool {
    *NO_COLOR.get_or_init(|| {
        std::env::var("NO_COLOR").is_ok()
            || std::env::var("TERM").map(|t| t == "dumb").unwrap_or(false)
    })
}

/// Wrap a ratatui `Style` to strip colors in no-color mode.
///
/// Pass any fully-styled `Style` value. In normal mode it is returned unchanged.
/// When `no_color_mode()` is active, `Style::default()` is returned instead so
/// that the terminal renders plain text without ANSI color codes.
///
/// Use this for the **main visual elements** (borders, titles, selection highlights,
/// help-bar key labels) — not every single styled span.
#[inline]
pub fn themed(style: Style) -> Style {
    if no_color_mode() {
        Style::default()
    } else {
        style
    }
}

/// Render a centered "terminal too small" warning overlay.
///
/// Shown whenever the terminal dimensions drop below [`MIN_COLS`]×[`MIN_ROWS`].
pub fn draw_too_small_overlay(f: &mut Frame) {
    let area = f.area();
    f.render_widget(Clear, area);

    let msg = format!(
        " Terminal too small ({}x{}) — resize to at least {}x{} ",
        area.width, area.height, MIN_COLS, MIN_ROWS
    );
    let msg_width = (msg.len() as u16 + 2).min(area.width);
    let msg_height = 3u16;
    let x = area.width.saturating_sub(msg_width) / 2;
    let y = area.height.saturating_sub(msg_height) / 2;
    let popup = Rect::new(x, y, msg_width, msg_height.min(area.height));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(themed(Style::default().fg(theme::WARNING)));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    f.render_widget(
        Paragraph::new(Span::styled(
            msg.trim(),
            themed(Style::default().fg(theme::WARNING)),
        ))
        .alignment(Alignment::Center),
        inner,
    );
}

/// Check terminal size before entering TUI. Returns an error message if too small.
pub fn check_terminal_size() -> Result<(), String> {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((0, 0));
    if cols < MIN_COLS || rows < MIN_ROWS {
        Err(format!(
            "Terminal too small: {}x{} (minimum {}x{}). Please resize and try again.",
            cols, rows, MIN_COLS, MIN_ROWS
        ))
    } else {
        Ok(())
    }
}

/// Render the Lo-phi ASCII logo into `area`.
///
/// The logo is centred horizontally within `area` and consists of:
/// - 6 lines of block-character ASCII art in Sky bold (LOGO_LO)
/// - 1 blank line
/// - A tagline with a Mauve bold "φ " prefix (LOGO_PHI) and Muted body text
pub fn render_logo(f: &mut Frame, area: Rect) {
    let logo_lines = vec![
        Line::from(Span::styled(
            "██╗      ██████╗       ██████╗ ██╗  ██╗██╗",
            themed(Style::default().fg(theme::LOGO_LO).bold()),
        )),
        Line::from(Span::styled(
            "██║     ██╔═══██╗      ██╔══██╗██║  ██║██║",
            themed(Style::default().fg(theme::LOGO_LO).bold()),
        )),
        Line::from(Span::styled(
            "██║     ██║   ██║█████╗██████╔╝███████║██║",
            themed(Style::default().fg(theme::LOGO_LO).bold()),
        )),
        Line::from(Span::styled(
            "██║     ██║   ██║╚════╝██╔═══╝ ██╔══██║██║",
            themed(Style::default().fg(theme::LOGO_LO).bold()),
        )),
        Line::from(Span::styled(
            "███████╗╚██████╔╝      ██║     ██║  ██║██║",
            themed(Style::default().fg(theme::LOGO_LO).bold()),
        )),
        Line::from(Span::styled(
            "╚══════╝ ╚═════╝       ╚═╝     ╚═╝  ╚═╝╚═╝",
            themed(Style::default().fg(theme::LOGO_LO).bold()),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("φ ", themed(Style::default().fg(theme::LOGO_PHI).bold())),
            Span::styled(
                "Feature Reduction as simple as phi",
                themed(Style::default().fg(theme::MUTED)),
            ),
        ]),
    ];

    let logo_paragraph = Paragraph::new(logo_lines).alignment(Alignment::Center);
    f.render_widget(logo_paragraph, area);
}
