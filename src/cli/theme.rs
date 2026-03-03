//! Catppuccin Mocha color theme for Lo-phi TUI
//!
//! Provides semantic color constants mapped to Catppuccin Mocha palette values.
//! All colors are encoded as `Color::Rgb` so they work on any true-color terminal
//! without any external font or color name dependency.
//!
//! # Semantic Roles
//!
//! | Constant  | Mocha Color | Hex      | Use                                  |
//! |-----------|-------------|----------|--------------------------------------|
//! | PRIMARY   | Sapphire    | #74c7ec  | Borders, general navigation          |
//! | ACCENT    | Mauve       | #cba6f7  | Phi symbol, target step, solver      |
//! | SUCCESS   | Green       | #a6e3a1  | Solver enabled, confirmed values     |
//! | WARNING   | Yellow      | #f9e2af  | Threshold inputs, active edit mode   |
//! | ERROR     | Red         | #f38ba8  | Validation errors, drop-columns step |
//! | DANGER    | Maroon      | #eba0ac  | Quit confirmation overlay            |
//! | KEYS      | Blue        | #89b4fa  | Keyboard shortcut labels in help bar |
//! | LOGO_LO   | Sky         | #89dceb  | "Lo-" part of ASCII logo             |
//! | LOGO_PHI  | Mauve       | #cba6f7  | "φ" glyph in logo tagline            |
//! | TEXT      | Text        | #cdd6f4  | Primary body text                    |
//! | SUBTEXT   | Subtext 0   | #a6adc8  | Section headers (THRESHOLDS, etc.)   |
//! | MUTED     | Overlay 0   | #6c7086  | Labels, descriptions, dim text       |
//! | SURFACE   | Surface 1   | #45475a  | Input field borders                  |
//! | DIVIDER   | Surface 2   | #585b70  | Column dividers (│ separators)       |
//! | BASE      | Base        | #1e1e2e  | Inverted selection background        |

use ratatui::style::Color;

// ---------------------------------------------------------------------------
// Accent / semantic role constants
// ---------------------------------------------------------------------------

/// Primary border and navigation color (Sapphire #74c7ec)
pub const PRIMARY: Color = Color::Rgb(116, 199, 236);

/// Accent color for phi, target step, and solver (Mauve #cba6f7)
pub const ACCENT: Color = Color::Rgb(203, 166, 247);

/// Success / solver-enabled / confirmed values (Green #a6e3a1)
pub const SUCCESS: Color = Color::Rgb(166, 227, 161);

/// Active edit / threshold inputs (Yellow #f9e2af)
pub const WARNING: Color = Color::Rgb(249, 226, 175);

/// Validation errors, drop-columns (Red #f38ba8)
pub const ERROR: Color = Color::Rgb(243, 139, 168);

/// Quit overlay danger color (Maroon #eba0ac)
pub const DANGER: Color = Color::Rgb(235, 160, 172);

/// Keyboard shortcut key labels (Blue #89b4fa)
pub const KEYS: Color = Color::Rgb(137, 180, 250);

// ---------------------------------------------------------------------------
// Logo colors
// ---------------------------------------------------------------------------

/// "Lo-" block characters in ASCII logo (Sky #89dceb)
pub const LOGO_LO: Color = Color::Rgb(137, 220, 235);

/// "φ" phi glyph in logo tagline (Mauve #cba6f7) — same as ACCENT
pub const LOGO_PHI: Color = ACCENT;

// ---------------------------------------------------------------------------
// Text shades
// ---------------------------------------------------------------------------

/// Primary body text (Text #cdd6f4)
pub const TEXT: Color = Color::Rgb(205, 214, 244);

/// Section headers like THRESHOLDS / SOLVER / DATA (Subtext 0 #a6adc8)
pub const SUBTEXT: Color = Color::Rgb(166, 173, 200);

/// Labels, descriptions, dim text (Overlay 0 #6c7086)
pub const MUTED: Color = Color::Rgb(108, 112, 134);

// ---------------------------------------------------------------------------
// Surface / base shades
// ---------------------------------------------------------------------------

/// Input field borders (Surface 1 #45475a)
pub const SURFACE: Color = Color::Rgb(69, 71, 90);

/// Column dividers │ (Surface 2 #585b70)
pub const DIVIDER: Color = Color::Rgb(88, 91, 112);

/// Inverted selection background (Base #1e1e2e)
pub const BASE: Color = Color::Rgb(30, 30, 46);
