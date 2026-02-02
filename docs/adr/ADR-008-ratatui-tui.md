# ADR-008: Ratatui Terminal UI Framework

**Status:** Accepted
**Date:** 2026-02-01

---

## Context

Lo-phi requires an interactive configuration interface allowing users to review and adjust pipeline parameters (thresholds, solver options, target column selection) before execution. The interface must work in terminal environments (SSH sessions, Docker containers, CI/CD pipelines) where graphical UIs are unavailable. Users need to see current settings, edit values with validation, select columns from lists, and confirm before running expensive computations.

Three UI approaches exist for terminal applications: CLI-only with manual flag specification (minimal interactivity), terminal UI frameworks (rich interaction without graphics), and web-based UIs served via localhost (browser-based interaction). The solution must balance ease of use, deployment simplicity, and consistency with Rust ecosystem conventions. Additionally, the tool must support both interactive and non-interactive modes for automation.

**Key Factors:**
- SSH compatibility - must work over remote terminal connections
- No GUI dependencies - runs in headless environments (Docker, cloud VMs)
- Parameter validation - prevent invalid inputs before pipeline execution
- Discoverability - users should explore options without reading documentation
- Automation support - non-interactive mode for CI/CD pipelines

## Decision

**Chosen Solution:** Ratatui v0.29 for terminal UI, with crossterm backend and `--no-confirm` flag for non-interactive mode

Ratatui provides a declarative widget-based TUI framework with keyboard navigation, three-column layout displaying thresholds/solver/data parameters, and real-time validation. Users can edit settings via single-key shortcuts ([C] for thresholds, [S] for solver), see live updates in the UI, and proceed with [Enter].

## Alternatives Considered

### Alternative 1: CLI-Only with Flags

**Description:** Remove interactive menu entirely, requiring all parameters specified via command-line flags (current `--no-confirm` mode as only mode).

**Pros:**
- Simplest implementation - no TUI framework dependency
- Easily scriptable - full automation without interactive prompts
- Low learning curve - standard CLI conventions (--flag value)
- Lightweight binary - no terminal UI rendering code

**Cons:**
- Poor discoverability - users must read docs to find all 20+ flags
- Tedious parameter adjustment - re-run entire command to change one threshold
- No validation until execution starts - typos waste time
- Difficult to see current configuration at a glance
- Intimidating for non-technical users (credit analysts, not engineers)

**Rejection Reason:** Accessibility barrier for primary user base (credit risk analysts, data scientists) who expect exploratory workflows with immediate feedback. CLI-only suitable for automation but hostile to interactive exploration.

---

### Alternative 2: Web UI (localhost server)

**Description:** Embed web server (e.g., actix-web) serving HTML/JavaScript configuration interface, launch browser automatically on start.

**Pros:**
- Rich UI capabilities - dropdowns, sliders, real-time charts
- Familiar interaction model - most users comfortable with web forms
- Mobile-friendly - could configure from phone browser
- Visual appeal - CSS styling, colors, icons

**Cons:**
- Heavyweight deployment - requires open port, CORS handling, static file serving
- Security concerns - localhost server could be exploited if firewall misconfigured
- SSH incompatibility - requires port forwarding for remote usage
- Browser dependency - fails on systems without graphical browser (headless servers)
- 5-10MB binary size increase for web framework and static assets
- Overkill for simple configuration form

**Rejection Reason:** Deployment complexity and SSH incompatibility disqualify web UI for tool targeting terminal-native users. Security surface area (open port) unacceptable for data processing tool.

---

### Alternative 3: GUI Framework (egui)

**Description:** Use immediate-mode GUI framework like egui to render native desktop windows with graphical controls.

**Pros:**
- Native desktop feel - buttons, checkboxes, dropdown menus
- Mouse interaction - point-and-click simpler than keyboard navigation
- Visual polish - professional appearance

**Cons:**
- Requires graphical display - fails over SSH and in Docker containers
- Platform-specific dependencies - X11/Wayland on Linux, Cocoa on macOS
- 10-20MB binary size increase
- Incompatible with core use case (remote server usage)
- Alienates terminal-native users who prefer keyboard-only interaction

**Rejection Reason:** GUI requirement breaks fundamental constraint of terminal-only operation. SSH and container usage are primary deployment targets, not edge cases.

## Consequences

### Positive Outcomes

- **SSH Compatibility:** Works flawlessly over SSH connections, supporting remote server workflows where data scientists analyze datasets on cloud VMs.
- **Keyboard Efficiency:** Expert users can navigate entire configuration in <10 seconds using single-key shortcuts, faster than mouse-based forms.
- **Dual-Mode Support:** `--no-confirm` flag enables full automation for CI/CD pipelines while preserving interactive mode for exploratory analysis.

### Negative Outcomes / Trade-offs

- **Learning Curve:** First-time users must learn keyboard shortcuts ([C], [S], [T], [W], [D], [F], [A]). Mitigated by on-screen help text listing all shortcuts.
- **Limited Interactivity:** Cannot show live previews (e.g., histograms of feature distributions), though this is acceptable given tool's batch processing nature.

### Neutral / Future Considerations

- **Mouse Support:** Crossterm supports mouse events; future enhancement could add clickable UI elements while preserving keyboard navigation for power users.

## Implementation Notes

**Key Files:**
- `src/cli/config_menu.rs` - Full file (~800 lines): Three-column layout, keyboard event handling, validation logic
- `Cargo.toml` - Dependencies: `ratatui = "0.29"`, `crossterm = "0.28"`
- `src/main.rs` - Lines 302-390: Menu invocation loop with conversion workflow integration

**Dependencies:**
- `ratatui = "0.29"` - TUI framework with widget primitives
- `crossterm = "0.28"` - Terminal backend (keyboard events, rendering)

## References

- Ratatui Documentation: https://ratatui.rs/
- Crossterm Crate: https://docs.rs/crossterm/
- TUI Design Patterns: "Designing for the Terminal" - https://lucasfcosta.com/2019/04/07/streams-introduction.html
