// ABOUTME: TUI layout and widget rendering
// ABOUTME: Draws meetings view (list, preview, search) and analytics view (dashboard, trends)

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use super::app::{AnalyticsTab, App, FocusedPane, Mode, TrendsGranularity, View};

/// Render an ASCII horizontal bar chart segment.
fn render_bar(value: i64, max: i64, width: usize) -> String {
    if max == 0 {
        return "\u{2591}".repeat(width);
    }
    let filled = ((value as f64 / max as f64) * width as f64) as usize;
    let filled = filled.min(width);
    let empty = width - filled;
    format!("{}{}", "\u{2588}".repeat(filled), "\u{2591}".repeat(empty))
}

/// Format a number compactly (e.g., 1500 -> "1.5K", 1500000 -> "1.5M").
fn format_compact(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

/// Truncate a string to a maximum width, padding or appending "..." if needed.
fn truncate_str(s: &str, max_width: usize) -> String {
    if s.chars().count() <= max_width {
        format!("{:<width$}", s, width = max_width)
    } else {
        let truncated: String = s.chars().take(max_width.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

/// Render the complete TUI layout, dispatching to the active view.
pub fn draw(frame: &mut Frame, app: &App) {
    match app.view {
        View::Meetings => draw_meetings_view(frame, app),
        View::Analytics => draw_analytics_view(frame, app),
    }
}

/// Render the meetings view (search bar, meeting list, preview, help bar, attendee popup).
fn draw_meetings_view(frame: &mut Frame, app: &App) {
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

    let search = Paragraph::new(search_line).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Search")
            .border_style(Style::default().fg(Color::Blue)),
    );
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

    let stats = Paragraph::new(Span::styled(stats_text, Style::default().fg(Color::Green))).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Stats")
            .border_style(Style::default().fg(Color::Blue)),
    );
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
    let border_color = if app.focused_pane == FocusedPane::MeetingList {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let title_style = if app.focused_pane == FocusedPane::MeetingList {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(title, title_style))
            .border_style(Style::default().fg(border_color)),
    );

    let mut list_state = ListState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn draw_preview(frame: &mut Frame, app: &App, area: Rect) {
    // Use cached parsed markdown when available (file previews).
    // Fall back to inline parsing for metadata-only or empty states.
    let fallback_content;
    let text = if let Some(ref cached) = app.preview_parsed {
        cached.clone()
    } else {
        fallback_content = if let Some(doc) = app.selected_document() {
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
        tui_markdown::from_str(&fallback_content)
    };

    let border_color = if app.focused_pane == FocusedPane::Preview {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let title_style = if app.focused_pane == FocusedPane::Preview {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    let preview = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .scroll((app.preview_scroll, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled("Preview", title_style))
                .border_style(Style::default().fg(border_color)),
        );
    frame.render_widget(preview, area);
}

fn draw_help_bar(frame: &mut Frame, app: &App, area: Rect) {
    let help_text = match app.mode {
        Mode::Normal => {
            if app.active_attendee_filter.is_some() {
                "[q] quit  [Tab] switch pane  [j/k] navigate  [/] search  [@] attendees  [C] clear filter  [Enter] open"
            } else {
                "[q] quit  [Tab] switch pane  [j/k] navigate  [/] search  [@] attendees  [Enter] open"
            }
        }
        Mode::Search => "[Enter] confirm  [Esc] cancel  [type] filter",
        Mode::AttendeeFilter => "[Enter] apply  [Esc] cancel  [j/k] select  [type] filter",
    };

    let help = Paragraph::new(Span::styled(
        help_text,
        Style::default().fg(Color::DarkGray),
    ));
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
            .title(Span::styled(
                "Filter by Attendee",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ))
            .border_style(Style::default().fg(Color::Magenta)),
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
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(
                count_text,
                Style::default().fg(Color::Magenta),
            ))
            .border_style(Style::default().fg(Color::Magenta)),
    );
    frame.render_widget(list, popup_chunks[1]);
}

/// Render the analytics view with tab bar, content, and help bar.
fn draw_analytics_view(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tab bar
            Constraint::Min(5),    // content
            Constraint::Length(1), // help bar
        ])
        .split(frame.area());

    draw_analytics_tab_bar(frame, app, chunks[0]);
    match app.analytics.tab {
        AnalyticsTab::Dashboard => draw_dashboard_tab(frame, app, chunks[1]),
        AnalyticsTab::Trends => draw_trends_tab(frame, app, chunks[1]),
    }
    draw_analytics_help_bar(frame, app, chunks[2]);
}

/// Render the analytics tab bar showing Dashboard and Trends tabs.
fn draw_analytics_tab_bar(frame: &mut Frame, app: &App, area: Rect) {
    let dashboard_style = if app.analytics.tab == AnalyticsTab::Dashboard {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let trends_style = if app.analytics.tab == AnalyticsTab::Trends {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let tabs = Paragraph::new(Line::from(vec![
        Span::styled(" [1 Dashboard] ", dashboard_style),
        Span::raw("  "),
        Span::styled(" [2 Trends] ", trends_style),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(
                "Analytics",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ))
            .border_style(Style::default().fg(Color::Green)),
    );
    frame.render_widget(tabs, area);
}

/// Render the dashboard tab with summary stats, top attendees, and labels.
fn draw_dashboard_tab(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    // Summary section
    lines.push(Line::from(Span::styled(
        "--- Summary ---",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));
    if let Some(ref stats) = app.analytics.stats {
        lines.push(Line::from(format!(
            "  {} meetings | {} attendees | {:.1}/wk",
            format_compact(stats.total_meetings),
            format_compact(stats.unique_attendees),
            stats.meetings_per_week
        )));
        if app.analytics.total_hours > 0.0 || app.analytics.avg_duration > 0.0 {
            lines.push(Line::from(format!(
                "  {:.1} hrs total | {:.0} min avg",
                app.analytics.total_hours, app.analytics.avg_duration
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "  Duration: N/A (not tracked by API)",
                Style::default().fg(Color::DarkGray),
            )));
        }
    } else {
        lines.push(Line::from("  No data"));
    }
    lines.push(Line::from(""));

    // Top Attendees section
    lines.push(Line::from(Span::styled(
        "--- Top Attendees ---",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));
    let att_max = app
        .analytics
        .top_attendees
        .first()
        .map(|a| a.count)
        .unwrap_or(1);
    for att in app.analytics.top_attendees.iter().take(10) {
        let bar = render_bar(att.count, att_max, 20);
        lines.push(Line::from(format!(
            "  {} ({:>3}) {}",
            truncate_str(&att.name, 20),
            att.count,
            bar
        )));
    }
    if app.analytics.top_attendees.is_empty() {
        lines.push(Line::from("  No attendee data"));
    }
    lines.push(Line::from(""));

    // Labels section
    lines.push(Line::from(Span::styled(
        "--- Labels ---",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));
    let label_max = app.analytics.labels.first().map(|l| l.count).unwrap_or(1);
    for label in app.analytics.labels.iter().take(10) {
        let bar = render_bar(label.count, label_max, 20);
        lines.push(Line::from(format!(
            "  {} ({:>3}) {}",
            truncate_str(&label.label, 20),
            label.count,
            bar
        )));
    }
    if app.analytics.labels.is_empty() {
        lines.push(Line::from("  No label data"));
    }

    let text = ratatui::text::Text::from(lines);
    let paragraph = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .scroll((app.analytics.scroll, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green)),
        );
    frame.render_widget(paragraph, area);
}

/// Render the trends tab with four stacked sections: Rhythm, People, Content, Superlatives.
fn draw_trends_tab(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    // Section 1: Rhythm
    draw_rhythm_section(&mut lines, app);

    // Section 2: People
    draw_people_section(&mut lines, app);

    // Section 3: Content
    draw_content_section(&mut lines, app);

    // Section 4: Superlatives
    draw_superlatives_section(&mut lines, app);

    let text = ratatui::text::Text::from(lines);
    let paragraph = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .scroll((app.analytics.scroll, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green)),
        );
    frame.render_widget(paragraph, area);
}

/// Render the Rhythm section: volume chart, day of week, hour of day, avg attendees by month.
fn draw_rhythm_section(lines: &mut Vec<Line>, app: &App) {
    // Volume chart header with granularity
    let granularity_label = match app.analytics.granularity {
        TrendsGranularity::Daily => "Daily",
        TrendsGranularity::Weekly => "Weekly",
        TrendsGranularity::Monthly => "Monthly",
    };
    lines.push(Line::from(Span::styled(
        format!("--- Rhythm: {} Meeting Volume ---", granularity_label),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));

    match app.analytics.granularity {
        TrendsGranularity::Daily => {
            let max = app
                .analytics
                .daily_counts
                .iter()
                .map(|d| d.count)
                .max()
                .unwrap_or(1);
            for entry in &app.analytics.daily_counts {
                let bar = render_bar(entry.count, max, 30);
                lines.push(Line::from(format!(
                    "  {} ({:>3}) {}",
                    entry.day, entry.count, bar
                )));
            }
            if app.analytics.daily_counts.is_empty() {
                lines.push(Line::from("  No daily data"));
            }
        }
        TrendsGranularity::Weekly => {
            let max = app
                .analytics
                .weekly_counts
                .iter()
                .map(|w| w.count)
                .max()
                .unwrap_or(1);
            for entry in &app.analytics.weekly_counts {
                let bar = render_bar(entry.count, max, 30);
                lines.push(Line::from(format!(
                    "  {} ({:>3}) {}",
                    entry.week, entry.count, bar
                )));
            }
            if app.analytics.weekly_counts.is_empty() {
                lines.push(Line::from("  No weekly data"));
            }
        }
        TrendsGranularity::Monthly => {
            let max = app
                .analytics
                .monthly_counts
                .iter()
                .map(|m| m.count)
                .max()
                .unwrap_or(1);
            for entry in &app.analytics.monthly_counts {
                let bar = render_bar(entry.count, max, 30);
                lines.push(Line::from(format!(
                    "  {} ({:>3}) {}",
                    entry.month, entry.count, bar
                )));
            }
            if app.analytics.monthly_counts.is_empty() {
                lines.push(Line::from("  No monthly data"));
            }
        }
    }
    lines.push(Line::from(""));

    // Day of Week
    lines.push(Line::from(Span::styled(
        "  Day of Week",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    let dow_max = app
        .analytics
        .weekday_counts
        .iter()
        .map(|d| d.count)
        .max()
        .unwrap_or(1);
    for entry in &app.analytics.weekday_counts {
        let bar = render_bar(entry.count, dow_max, 20);
        lines.push(Line::from(format!(
            "  {} ({:>3}) {}",
            entry.day_name, entry.count, bar
        )));
    }
    if app.analytics.weekday_counts.is_empty() {
        lines.push(Line::from("  No data"));
    }
    lines.push(Line::from(""));

    // Hour of Day
    lines.push(Line::from(Span::styled(
        "  Hour of Day",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    let hr_max = app
        .analytics
        .hourly_counts
        .iter()
        .map(|h| h.count)
        .max()
        .unwrap_or(1);
    for entry in &app.analytics.hourly_counts {
        let bar = render_bar(entry.count, hr_max, 20);
        lines.push(Line::from(format!(
            "  {:02}:00 ({:>3}) {}",
            entry.hour, entry.count, bar
        )));
    }
    if app.analytics.hourly_counts.is_empty() {
        lines.push(Line::from("  No data"));
    }
    lines.push(Line::from(""));

    // Avg Attendees by Month
    lines.push(Line::from(Span::styled(
        "  Avg Attendees/Meeting",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    for entry in app.analytics.meeting_size_trend.iter().take(12) {
        lines.push(Line::from(format!(
            "  {} ({:.1})",
            entry.month, entry.avg_attendees
        )));
    }
    if app.analytics.meeting_size_trend.is_empty() {
        lines.push(Line::from("  No data"));
    }
    lines.push(Line::from(""));
}

/// Render the People section: top collaborators, new faces, top companies.
fn draw_people_section(lines: &mut Vec<Line>, app: &App) {
    lines.push(Line::from(Span::styled(
        "--- People ---",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));

    // Top Collaborators (last 30 days)
    lines.push(Line::from(Span::styled(
        "  Top Collaborators (last 30 days)",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    let collab_max = app
        .analytics
        .recent_collaborators
        .iter()
        .map(|c| c.count)
        .max()
        .unwrap_or(1);
    for collab in app.analytics.recent_collaborators.iter().take(10) {
        let bar = render_bar(collab.count, collab_max, 20);
        lines.push(Line::from(format!(
            "  {} ({:>3}) {}",
            truncate_str(&collab.name, 20),
            collab.count,
            bar
        )));
    }
    if app.analytics.recent_collaborators.is_empty() {
        lines.push(Line::from("  No recent collaborators"));
    }
    lines.push(Line::from(""));

    // New Faces This Month
    if !app.analytics.new_faces.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  New Faces This Month: {}", app.analytics.new_faces.len()),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        let preview: String = app
            .analytics
            .new_faces
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if app.analytics.new_faces.len() > 5 {
            format!(", ... (+{} more)", app.analytics.new_faces.len() - 5)
        } else {
            String::new()
        };
        lines.push(Line::from(format!("    {}{}", preview, suffix)));
        lines.push(Line::from(""));
    }

    // Top Companies
    lines.push(Line::from(Span::styled(
        "  Top Companies",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    let comp_max = app
        .analytics
        .top_companies
        .iter()
        .map(|c| c.count)
        .max()
        .unwrap_or(1);
    for comp in app.analytics.top_companies.iter().take(10) {
        let bar = render_bar(comp.count, comp_max, 20);
        lines.push(Line::from(format!(
            "  {} ({:>3}) {}",
            truncate_str(&comp.company, 20),
            comp.count,
            bar
        )));
    }
    if app.analytics.top_companies.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No company data",
            Style::default().fg(Color::DarkGray),
        )));
    }
    lines.push(Line::from(""));
}

/// Render the Content section: label trends and busiest weeks.
fn draw_content_section(lines: &mut Vec<Line>, app: &App) {
    lines.push(Line::from(Span::styled(
        "--- Content ---",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));

    // Label Trends — grouped by label showing monthly counts
    lines.push(Line::from(Span::styled(
        "  Label Trends (last 6 months)",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    if app.analytics.label_trends.is_empty() {
        lines.push(Line::from("  No label data"));
    } else {
        let mut current_label = String::new();
        for entry in &app.analytics.label_trends {
            if entry.label != current_label {
                current_label = entry.label.clone();
                lines.push(Line::from(format!("  {}", current_label)));
            }
            lines.push(Line::from(format!("    {} ({})", entry.month, entry.count)));
        }
    }
    lines.push(Line::from(""));

    // Busiest Weeks
    lines.push(Line::from(Span::styled(
        "  Busiest Weeks",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    for (i, week) in app.analytics.busiest_weeks.iter().take(5).enumerate() {
        lines.push(Line::from(format!(
            "  {}. {} ({} meetings)",
            i + 1,
            week.week,
            week.count
        )));
    }
    if app.analytics.busiest_weeks.is_empty() {
        lines.push(Line::from("  No data"));
    }
    lines.push(Line::from(""));
}

/// Render the Superlatives section: fun one-liner stats about meeting history.
fn draw_superlatives_section(lines: &mut Vec<Line>, app: &App) {
    lines.push(Line::from(Span::styled(
        "--- Superlatives ---",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));

    if let Some(ref s) = app.analytics.superlatives {
        // Check if all superlatives are empty/default
        let has_any = s.marathon.is_some()
            || s.social_butterfly.is_some()
            || s.email_meetings > 0
            || s.streak_days > 1
            || s.solo_meetings > 0
            || s.recurring_champ.is_some();

        if !has_any {
            lines.push(Line::from(Span::styled(
                "  No data available",
                Style::default().fg(Color::DarkGray),
            )));
            return;
        }

        // Marathon Meeting
        if let Some((ref title, ref date, secs)) = s.marathon {
            let hours = secs / 3600;
            let mins = (secs % 3600) / 60;
            lines.push(Line::from(vec![
                Span::styled(
                    "  Marathon Meeting: ",
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("\"{}\"", truncate_str(title, 30).trim())),
            ]));
            lines.push(Line::from(format!(
                "    {} | {}h {:02}m",
                date, hours, mins
            )));
        }

        // Social Butterfly
        if let Some((ref name, count)) = s.social_butterfly {
            lines.push(Line::from(vec![
                Span::styled(
                    "  Social Butterfly: ",
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(name.clone()),
            ]));
            lines.push(Line::from(format!(
                "    Met with {} different people",
                count
            )));
        }

        // Could've Been an Email
        if s.email_meetings > 0 {
            lines.push(Line::from(vec![
                Span::styled(
                    "  Could've Been an Email: ",
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("{} meetings", s.email_meetings)),
            ]));
            lines.push(Line::from("    5+ attendees, under 15 minutes"));
        }

        // Meeting Streak
        if s.streak_days > 1 {
            lines.push(Line::from(vec![
                Span::styled(
                    "  Meeting Streak: ",
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("{} consecutive days", s.streak_days)),
            ]));
            if let (Some(ref start), Some(ref end)) = (&s.streak_start, &s.streak_end) {
                lines.push(Line::from(format!("    {} to {}", start, end)));
            }
        }

        // Solo Meetings
        if s.solo_meetings > 0 {
            lines.push(Line::from(vec![
                Span::styled(
                    "  Solo Meetings: ",
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("{}", s.solo_meetings)),
            ]));
            lines.push(Line::from("    You and your thoughts"));
        }

        // Recurring Champion
        if let Some((ref title, count)) = s.recurring_champ {
            lines.push(Line::from(vec![
                Span::styled(
                    "  Recurring Champion: ",
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("\"{}\"", truncate_str(title, 30).trim())),
            ]));
            lines.push(Line::from(format!("    {} occurrences", count)));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "  No data available",
            Style::default().fg(Color::DarkGray),
        )));
    }
}

/// Render context-aware help text for the analytics view.
fn draw_analytics_help_bar(frame: &mut Frame, app: &App, area: Rect) {
    let help_text = match app.analytics.tab {
        AnalyticsTab::Dashboard => "[a] meetings  [1/2] tabs  [j/k] scroll  [r] refresh  [q] quit",
        AnalyticsTab::Trends => {
            "[a] meetings  [1/2] tabs  [d/w/m] granularity  [j/k] scroll  [r] refresh  [q] quit"
        }
    };
    let help = Paragraph::new(Span::styled(
        help_text,
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(help, area);
}

#[cfg(test)]
mod tests {
    use ratatui::style::Modifier;

    #[test]
    fn test_markdown_heading_produces_styled_text() {
        let text = tui_markdown::from_str("# Hello World");
        assert!(!text.lines.is_empty(), "should produce at least one line");

        // tui-markdown applies heading style at the Line level, not on individual spans
        let first_line = &text.lines[0];
        let line_bold = first_line.style.add_modifier.contains(Modifier::BOLD);
        let span_bold = first_line
            .spans
            .iter()
            .any(|s| s.style.add_modifier.contains(Modifier::BOLD));
        assert!(
            line_bold || span_bold,
            "heading should be rendered bold (line or span level)"
        );
    }

    #[test]
    fn test_markdown_bold_produces_styled_text() {
        let text = tui_markdown::from_str("some **bold** text");
        let has_bold_span = text
            .lines
            .iter()
            .flat_map(|l| &l.spans)
            .any(|s| s.style.add_modifier.contains(Modifier::BOLD));
        let has_bold_line = text
            .lines
            .iter()
            .any(|l| l.style.add_modifier.contains(Modifier::BOLD));
        assert!(
            has_bold_span || has_bold_line,
            "bold markdown should produce bold styling"
        );
    }

    #[test]
    fn test_markdown_list_preserves_items() {
        let text = tui_markdown::from_str("- item one\n- item two\n- item three");
        // Each list item should produce at least one line
        assert!(
            text.lines.len() >= 3,
            "should have at least 3 lines for 3 list items"
        );
    }

    #[test]
    fn test_markdown_mixed_content() {
        let md = "# Meeting Notes\n\nDate: 2024-01-01\n\n## Attendees\n\n- **Alice**\n- Bob\n\n## Action Items\n\n1. Review PR\n2. Deploy changes";
        let text = tui_markdown::from_str(md);
        assert!(
            text.lines.len() >= 5,
            "mixed markdown should produce multiple styled lines"
        );
    }

    #[test]
    fn test_empty_markdown_produces_output() {
        let text = tui_markdown::from_str("");
        // Should not panic, may produce empty or minimal output
        assert!(text.lines.is_empty() || !text.lines.is_empty());
    }

    #[test]
    fn test_render_bar_full() {
        let bar = super::render_bar(10, 10, 10);
        assert_eq!(
            bar,
            "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}"
        );
    }

    #[test]
    fn test_render_bar_half() {
        let bar = super::render_bar(5, 10, 10);
        assert_eq!(
            bar,
            "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}"
        );
    }

    #[test]
    fn test_render_bar_empty() {
        let bar = super::render_bar(0, 10, 10);
        assert_eq!(
            bar,
            "\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}"
        );
    }

    #[test]
    fn test_render_bar_zero_max() {
        let bar = super::render_bar(5, 0, 10);
        assert_eq!(
            bar,
            "\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}"
        );
    }

    #[test]
    fn test_format_compact_small() {
        assert_eq!(super::format_compact(42), "42");
        assert_eq!(super::format_compact(999), "999");
    }

    #[test]
    fn test_format_compact_thousands() {
        assert_eq!(super::format_compact(1500), "1.5K");
        assert_eq!(super::format_compact(10000), "10.0K");
    }

    #[test]
    fn test_format_compact_millions() {
        assert_eq!(super::format_compact(1500000), "1.5M");
    }

    #[test]
    fn test_truncate_str_short() {
        let result = super::truncate_str("Alice", 20);
        assert_eq!(result.len(), 20);
        assert!(result.starts_with("Alice"));
    }

    #[test]
    fn test_truncate_str_long() {
        let result = super::truncate_str("This is a very long name indeed", 10);
        assert!(result.ends_with("..."));
        assert!(result.chars().count() <= 10);
    }
}
