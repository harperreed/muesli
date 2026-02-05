// ABOUTME: TUI layout and widget rendering
// ABOUTME: Draws the search bar, meeting list, preview pane, and help bar

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::app::{App, Mode};

/// Render the complete TUI layout.
pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // search bar
            Constraint::Min(5),    // main content
            Constraint::Length(1), // help bar
        ])
        .split(frame.area());

    draw_search_bar(frame, app, chunks[0]);
    draw_main_content(frame, app, chunks[1]);
    draw_help_bar(frame, app, chunks[2]);
}

fn draw_search_bar(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    // Search input
    let search_style = if app.mode == Mode::Search {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let search_text = if app.search_query.is_empty() {
        if app.mode == Mode::Search {
            "Type to search...".to_string()
        } else {
            "Press / to search".to_string()
        }
    } else {
        app.search_query.clone()
    };

    let search = Paragraph::new(search_text)
        .style(search_style)
        .block(Block::default().borders(Borders::ALL).title("Search"));
    frame.render_widget(search, chunks[0]);

    // Stats summary
    let stats_text = if let Some(ref stats) = app.stats {
        format!(
            "{} meetings | {} attendees | {:.1}/wk",
            stats.total_meetings, stats.unique_attendees, stats.meetings_per_week
        )
    } else {
        "No stats".to_string()
    };

    let stats =
        Paragraph::new(stats_text).block(Block::default().borders(Borders::ALL).title("Stats"));
    frame.render_widget(stats, chunks[1]);
}

fn draw_main_content(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    draw_meeting_list(frame, app, chunks[0]);
    draw_preview(frame, app, chunks[1]);
}

fn draw_meeting_list(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .enumerate()
        .map(|(i, &idx)| {
            let doc = &app.documents[idx];
            let date = doc.created_at.format("%Y-%m-%d").to_string();
            let title = doc.title.as_deref().unwrap_or("Untitled");
            let dur = doc
                .duration_seconds
                .map(|d| format!(" ({}m)", d / 60))
                .unwrap_or_default();

            let style = if i == app.selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let prefix = if i == app.selected { "> " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{}{}", prefix, date), style),
                Span::styled(format!(" {}{}", title, dur), style),
            ]))
        })
        .collect();

    let title = format!("Meetings ({}/{})", app.filtered.len(), app.documents.len());
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));

    frame.render_widget(list, area);
}

fn draw_preview(frame: &mut Frame, app: &App, area: Rect) {
    let content = if let Some(ref preview) = app.preview_content {
        preview.clone()
    } else if let Some(doc) = app.selected_document() {
        let title = doc.title.as_deref().unwrap_or("Untitled");
        let date = doc.created_at.format("%Y-%m-%d %H:%M");
        let dur = doc
            .duration_seconds
            .map(|d| format!("{} min", d / 60))
            .unwrap_or_else(|| "unknown".to_string());
        format!(
            "# {}\n\nDate: {}\nDuration: {}\nID: {}\n\nPress Enter to open full transcript.",
            title, date, dur, doc.doc_id
        )
    } else {
        "No document selected".to_string()
    };

    let preview = Paragraph::new(content)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title("Preview"));
    frame.render_widget(preview, area);
}

fn draw_help_bar(frame: &mut Frame, app: &App, area: Rect) {
    let help_text = match app.mode {
        Mode::Normal => "[q] quit  [j/k] navigate  [/] search  [Enter] open",
        Mode::Search => "[Enter] confirm  [Esc] cancel  [type] filter",
    };

    let help = Paragraph::new(help_text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, area);
}
