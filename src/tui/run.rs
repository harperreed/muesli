// ABOUTME: TUI entry point and event loop
// ABOUTME: Initializes terminal, loads data, runs the main loop, and restores on exit

use std::io;

use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::db;
use crate::storage::Paths;
use crate::Result;

use super::app::{App, View};
use super::events::handle_key_event;
use super::ui;

/// Run the interactive TUI dashboard.
pub fn run_tui(paths: &Paths) -> Result<()> {
    // Load data from DuckDB
    let conn = db::connection::open_or_create(&paths.db_path)?;
    let documents = db::queries::list_documents(&conn)?;
    let stats = db::queries::get_stats(&conn).ok();
    let attendees = db::queries::list_all_attendees(&conn).unwrap_or_default();

    let mut app = App::new(documents, stats, attendees);

    // Load preview for initial selection
    load_preview(&mut app, paths);
    app.parse_preview();

    // Setup terminal
    enable_raw_mode().map_err(|e| crate::Error::Filesystem(io::Error::other(e)))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|e| crate::Error::Filesystem(io::Error::other(e)))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|e| crate::Error::Filesystem(io::Error::other(e)))?;

    // Install panic hook to restore terminal on crash
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));

    // Main event loop
    let result = run_loop(&mut terminal, &mut app, paths, &conn);

    // Restore terminal
    disable_raw_mode().map_err(|e| crate::Error::Filesystem(io::Error::other(e)))?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .map_err(|e| crate::Error::Filesystem(io::Error::other(e)))?;
    terminal
        .show_cursor()
        .map_err(|e| crate::Error::Filesystem(io::Error::other(e)))?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    paths: &Paths,
    conn: &duckdb::Connection,
) -> Result<()> {
    let mut last_selected = app.selected;
    let mut last_doc_count = app.documents.len();

    loop {
        terminal
            .draw(|frame| ui::draw(frame, app))
            .map_err(|e| crate::Error::Filesystem(io::Error::other(e)))?;

        if let Event::Key(key) =
            event::read().map_err(|e| crate::Error::Filesystem(io::Error::other(e)))?
        {
            let was_enter = key.code == crossterm::event::KeyCode::Enter
                && app.mode == super::app::Mode::Normal;

            handle_key_event(app, key);

            // Lazy-load analytics data on first switch (or refresh)
            if app.view == View::Analytics && !app.analytics.loaded {
                load_analytics(app, conn);
            }

            if app.should_quit {
                break;
            }

            // Handle attendee filter changes by re-querying the DB
            if app.attendee_filter_changed {
                app.attendee_filter_changed = false;
                if let Some(ref name) = app.active_attendee_filter {
                    if let Ok(filtered_docs) = db::queries::filter_by_attendee(conn, name) {
                        app.documents = filtered_docs;
                        app.search_query.clear();
                        app.filtered = (0..app.documents.len()).collect();
                        app.selected = 0;
                        load_preview(app, paths);
                        app.parse_preview();
                        last_selected = app.selected;
                        last_doc_count = app.documents.len();
                    }
                }
            }

            // Refresh preview when document list changes (e.g., attendee filter cleared)
            if app.documents.len() != last_doc_count {
                load_preview(app, paths);
                app.parse_preview();
                last_selected = app.selected;
                last_doc_count = app.documents.len();
            }

            // Open in $EDITOR on Enter
            if was_enter {
                if let Some(doc) = app.selected_document() {
                    if let Some(ref fname) = doc.filename {
                        let md_path = paths.transcripts_dir.join(format!("{}.md", fname));
                        if md_path.exists() {
                            // Restore terminal for editor
                            let _ = disable_raw_mode();
                            let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
                            let _ = terminal.show_cursor();

                            let editor =
                                std::env::var("EDITOR").unwrap_or_else(|_| "less".to_string());
                            let _ = std::process::Command::new(&editor).arg(&md_path).status();

                            // Re-enter TUI
                            let _ = enable_raw_mode();
                            let _ = execute!(terminal.backend_mut(), EnterAlternateScreen);
                            terminal.clear().ok();
                        }
                    }
                }
            }

            // Update preview when selection changes
            if app.selected != last_selected {
                load_preview(app, paths);
                app.parse_preview();
                app.reset_preview_scroll();
                last_selected = app.selected;
            }
        }
    }

    Ok(())
}

/// Strip YAML frontmatter from markdown content.
/// Uses `splitn(3, "---\n")` so that `---` separators within the body are preserved.
fn strip_frontmatter(content: &str) -> &str {
    if content.starts_with("---\n") {
        content.splitn(3, "---\n").nth(2).unwrap_or(content)
    } else {
        content
    }
}

/// Load the preview content for the currently selected document.
fn load_preview(app: &mut App, paths: &Paths) {
    app.preview_content = None;
    if let Some(doc) = app.selected_document() {
        if let Some(ref fname) = doc.filename {
            let md_path = paths.transcripts_dir.join(format!("{}.md", fname));
            if let Ok(content) = std::fs::read_to_string(&md_path) {
                let body = strip_frontmatter(&content).to_string();
                // Truncate for preview performance
                let max_preview = 4000;
                if body.len() > max_preview {
                    let mut boundary = max_preview;
                    while boundary > 0 && !body.is_char_boundary(boundary) {
                        boundary -= 1;
                    }
                    app.preview_content = Some(format!("{}...", &body[..boundary]));
                } else {
                    app.preview_content = Some(body);
                }
            }
        }
    }
}

/// Load analytics data from the database into the app state.
fn load_analytics(app: &mut App, conn: &duckdb::Connection) {
    use crate::db::queries;

    app.analytics.stats = queries::get_stats(conn).ok();
    app.analytics.top_attendees = queries::top_attendees(conn, 10).unwrap_or_default();
    app.analytics.labels = queries::label_distribution(conn).unwrap_or_default();
    app.analytics.weekly_counts = queries::meetings_per_week(conn, 12).unwrap_or_default();
    app.analytics.daily_counts = queries::meetings_per_day(conn, 90).unwrap_or_default();
    app.analytics.monthly_counts = queries::meetings_per_month(conn, 12).unwrap_or_default();
    app.analytics.avg_duration = queries::average_duration(conn).unwrap_or(0.0);
    app.analytics.total_hours = app
        .analytics
        .stats
        .as_ref()
        .map(|s| s.total_duration_seconds as f64 / 3600.0)
        .unwrap_or(0.0);

    // Rhythm
    app.analytics.weekday_counts = queries::meetings_by_weekday(conn).unwrap_or_default();
    app.analytics.hourly_counts = queries::meetings_by_hour(conn).unwrap_or_default();
    app.analytics.meeting_size_trend =
        queries::avg_attendees_by_month(conn, 12).unwrap_or_default();

    // People
    app.analytics.recent_collaborators =
        queries::top_collaborators_recent(conn, 30, 10).unwrap_or_default();
    app.analytics.new_faces = queries::new_attendees_this_month(conn).unwrap_or_default();
    app.analytics.top_companies = queries::top_companies(conn, 10).unwrap_or_default();

    // Content
    app.analytics.label_trends = queries::label_trends(conn, 6).unwrap_or_default();
    app.analytics.busiest_weeks = queries::busiest_weeks(conn, 5).unwrap_or_default();

    // Superlatives
    app.analytics.superlatives = queries::superlatives(conn).ok();

    app.analytics.loaded = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_frontmatter_removes_yaml_header() {
        let input = "---\ntitle: Hello\ndate: 2025-01-01\n---\n# Body\n\nContent here.\n";
        let result = strip_frontmatter(input);
        assert_eq!(result, "# Body\n\nContent here.\n");
    }

    #[test]
    fn test_strip_frontmatter_preserves_body_internal_separators() {
        let input =
            "---\ntitle: Hello\n---\n# Body\n\nBefore separator\n\n---\n\nAfter separator\n";
        let result = strip_frontmatter(input);
        assert_eq!(
            result, "# Body\n\nBefore separator\n\n---\n\nAfter separator\n",
            "body-internal --- separators must be preserved"
        );
    }

    #[test]
    fn test_strip_frontmatter_no_frontmatter() {
        let input = "# Just a heading\n\nSome content.\n";
        let result = strip_frontmatter(input);
        assert_eq!(
            result, input,
            "content without frontmatter should pass through unchanged"
        );
    }

    #[test]
    fn test_strip_frontmatter_empty_frontmatter() {
        let input = "---\n---\nBody only.\n";
        let result = strip_frontmatter(input);
        assert_eq!(result, "Body only.\n");
    }
}
