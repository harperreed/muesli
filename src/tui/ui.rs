// ABOUTME: TUI layout and widget rendering
// ABOUTME: Draws the search bar, meeting list, preview pane, help bar, and attendee filter popup

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
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

    // Draw attendee filter popup overlay when in AttendeeFilter mode
    if app.mode == Mode::AttendeeFilter {
        draw_attendee_popup(frame, app);
    }
}

fn draw_search_bar(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    // Search input with optional attendee filter badge
    let search_style = if app.mode == Mode::Search {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let badge = app
        .active_attendee_filter
        .as_ref()
        .map(|name| format!("[@{}] ", name))
        .unwrap_or_default();

    let search_text = if app.search_query.is_empty() {
        if app.mode == Mode::Search {
            format!("{}Type to search...", badge)
        } else {
            format!("{}Press / to search", badge)
        }
    } else {
        format!("{}{}", badge, app.search_query)
    };

    let search_line = if app.active_attendee_filter.is_some() {
        let badge_span = Span::styled(
            badge.clone(),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        );
        let rest = search_text[badge.len()..].to_string();
        Line::from(vec![badge_span, Span::styled(rest, search_style)])
    } else {
        Line::from(Span::styled(search_text, search_style))
    };

    let search =
        Paragraph::new(search_line).block(Block::default().borders(Borders::ALL).title("Search"));
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
        Mode::Normal => {
            if app.active_attendee_filter.is_some() {
                "[q] quit  [j/k] navigate  [/] search  [@] attendees  [C] clear filter  [Enter] open"
            } else {
                "[q] quit  [j/k] navigate  [/] search  [@] attendees  [Enter] open"
            }
        }
        Mode::Search => "[Enter] confirm  [Esc] cancel  [type] filter",
        Mode::AttendeeFilter => "[Enter] apply  [Esc] cancel  [j/k] select  [type] filter",
    };

    let help = Paragraph::new(help_text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, area);
}

/// Compute a centered rectangle within the given area.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Draw the attendee filter popup overlay.
fn draw_attendee_popup(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 50, frame.area());

    // Clear the area underneath
    frame.render_widget(Clear, area);

    // Split popup into search input and list
    let popup_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    // Attendee search input
    let input_text = if app.attendee_query.is_empty() {
        "Type to filter attendees...".to_string()
    } else {
        app.attendee_query.clone()
    };
    let input_style = Style::default().fg(Color::Yellow);
    let input = Paragraph::new(input_text).style(input_style).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Filter by Attendee"),
    );
    frame.render_widget(input, popup_chunks[0]);

    // Attendee list
    let items: Vec<ListItem> = app
        .attendee_filtered
        .iter()
        .enumerate()
        .map(|(i, &idx)| {
            let name = &app.attendees[idx];
            let style = if i == app.attendee_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let prefix = if i == app.attendee_selected {
                "> "
            } else {
                "  "
            };
            ListItem::new(Span::styled(format!("{}{}", prefix, name), style))
        })
        .collect();

    let count_text = format!(
        "Attendees ({}/{})",
        app.attendee_filtered.len(),
        app.attendees.len()
    );
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(count_text));
    frame.render_widget(list, popup_chunks[1]);
}
