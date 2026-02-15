# Analytics TUI Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a tabbed analytics dashboard to the muesli TUI with meeting insights powered by DuckDB queries.

**Architecture:** View Enum + Flat State approach. A `View` enum toggles between the existing Meetings layout and a new Analytics view. Analytics has two sub-tabs (Dashboard and Trends) with lazy-loaded query results. All rendering uses ASCII bar charts via a shared helper function.

**Tech Stack:** Rust, ratatui, crossterm, DuckDB, chrono

**Design doc:** `docs/plans/2026-02-14-analytics-tui-design.md`

**Test command:** `cargo test --lib --all-features --no-fail-fast`

**Lint command:** `cargo clippy --all-features -- -D warnings`

---

### Task 1: Add Daily and Monthly Count Queries

**Files:**
- Modify: `src/db/queries.rs`

**Step 1: Write the failing test for `meetings_per_day`**

Add to the `#[cfg(test)] mod tests` block in `src/db/queries.rs`:

```rust
#[test]
fn test_meetings_per_day() {
    let conn = open_in_memory().unwrap();
    let meta = make_test_metadata("Meeting A", &[], &[]);
    upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

    let daily = meetings_per_day(&conn, 90).unwrap();
    assert!(!daily.is_empty());
    assert_eq!(daily[0].count, 1);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib --all-features test_meetings_per_day`
Expected: FAIL — `meetings_per_day` not found

**Step 3: Write minimal implementation**

Add these types and functions to `src/db/queries.rs` (after `WeeklyCount` and `meetings_per_week`):

```rust
/// A row for daily meeting counts.
#[derive(Debug, Clone)]
pub struct DailyCount {
    pub day: String,
    pub count: i64,
}

/// A row for monthly meeting counts.
#[derive(Debug, Clone)]
pub struct MonthlyCount {
    pub month: String,
    pub count: i64,
}

/// Get meeting counts grouped by day for the last N days.
pub fn meetings_per_day(conn: &Connection, days: usize) -> Result<Vec<DailyCount>> {
    let mut stmt = conn.prepare(
        "SELECT strftime(CAST(created_at AS TIMESTAMP), '%Y-%m-%d') as day, count(*) as cnt
         FROM documents
         WHERE CAST(created_at AS TIMESTAMP) >= current_date - CAST(? AS INTEGER) * INTERVAL 1 DAY
         GROUP BY day
         ORDER BY day DESC",
    )?;
    let rows = stmt.query_map(params![days as i64], |row| {
        Ok(DailyCount {
            day: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Get meeting counts grouped by month for the last N months.
pub fn meetings_per_month(conn: &Connection, months: usize) -> Result<Vec<MonthlyCount>> {
    let mut stmt = conn.prepare(
        "SELECT strftime(CAST(created_at AS TIMESTAMP), '%Y-%m') as month, count(*) as cnt
         FROM documents
         WHERE CAST(created_at AS TIMESTAMP) >= current_date - CAST(? AS INTEGER) * INTERVAL 30 DAY
         GROUP BY month
         ORDER BY month DESC",
    )?;
    let rows = stmt.query_map(params![months as i64], |row| {
        Ok(MonthlyCount {
            month: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}
```

**Step 4: Write the failing test for `meetings_per_month`**

Add to tests:

```rust
#[test]
fn test_meetings_per_month() {
    let conn = open_in_memory().unwrap();
    let meta = make_test_metadata("Meeting A", &[], &[]);
    upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

    let monthly = meetings_per_month(&conn, 12).unwrap();
    assert!(!monthly.is_empty());
    assert_eq!(monthly[0].count, 1);
}
```

**Step 5: Run all tests to verify they pass**

Run: `cargo test --lib --all-features test_meetings_per_day test_meetings_per_month`
Expected: PASS

**Step 6: Commit**

```
agentjj commit -m "feat: add daily and monthly meeting count queries"
```

---

### Task 2: Add View, AnalyticsTab, and AnalyticsState to App

**Files:**
- Modify: `src/tui/app.rs`

**Step 1: Write the failing test for View enum and AnalyticsState**

Add to `#[cfg(test)] mod tests` in `src/tui/app.rs`:

```rust
#[test]
fn test_default_view_is_meetings() {
    let app = App::new(vec![], None, vec![]);
    assert_eq!(app.view, View::Meetings);
}

#[test]
fn test_toggle_view() {
    let mut app = App::new(vec![], None, vec![]);
    assert_eq!(app.view, View::Meetings);
    app.toggle_view();
    assert_eq!(app.view, View::Analytics);
    app.toggle_view();
    assert_eq!(app.view, View::Meetings);
}

#[test]
fn test_analytics_tab_switch() {
    let mut app = App::new(vec![], None, vec![]);
    app.toggle_view();
    assert_eq!(app.analytics.tab, AnalyticsTab::Dashboard);
    app.analytics_next_tab();
    assert_eq!(app.analytics.tab, AnalyticsTab::Trends);
    app.analytics_next_tab();
    assert_eq!(app.analytics.tab, AnalyticsTab::Dashboard);
}

#[test]
fn test_analytics_prev_tab() {
    let mut app = App::new(vec![], None, vec![]);
    app.analytics.tab = AnalyticsTab::Dashboard;
    app.analytics_prev_tab();
    assert_eq!(app.analytics.tab, AnalyticsTab::Trends);
    app.analytics_prev_tab();
    assert_eq!(app.analytics.tab, AnalyticsTab::Dashboard);
}

#[test]
fn test_trends_granularity_cycle() {
    let mut app = App::new(vec![], None, vec![]);
    assert_eq!(app.analytics.granularity, TrendsGranularity::Weekly);
    app.analytics.granularity = TrendsGranularity::Daily;
    assert_eq!(app.analytics.granularity, TrendsGranularity::Daily);
    app.analytics.granularity = TrendsGranularity::Monthly;
    assert_eq!(app.analytics.granularity, TrendsGranularity::Monthly);
}

#[test]
fn test_analytics_scroll() {
    let mut app = App::new(vec![], None, vec![]);
    assert_eq!(app.analytics.scroll, 0);
    app.analytics.scroll = app.analytics.scroll.saturating_add(1);
    assert_eq!(app.analytics.scroll, 1);
    app.analytics.scroll = app.analytics.scroll.saturating_sub(1);
    assert_eq!(app.analytics.scroll, 0);
    app.analytics.scroll = app.analytics.scroll.saturating_sub(1);
    assert_eq!(app.analytics.scroll, 0);
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib --all-features test_default_view`
Expected: FAIL — `View` not found

**Step 3: Write minimal implementation**

Add these types and update `App` in `src/tui/app.rs`:

Update the import line to include new query types:
```rust
use crate::db::queries::{
    AttendeeFrequency, DailyCount, DocumentRow, LabelFrequency, MonthlyCount, Stats, WeeklyCount,
};
```

Add enums and struct before `App`:
```rust
/// Top-level view for the TUI.
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    Meetings,
    Analytics,
}

/// Sub-tab within the Analytics view.
#[derive(Debug, Clone, PartialEq)]
pub enum AnalyticsTab {
    Dashboard,
    Trends,
}

/// Time granularity for the Trends tab.
#[derive(Debug, Clone, PartialEq)]
pub enum TrendsGranularity {
    Daily,
    Weekly,
    Monthly,
}

/// State for the analytics view, including cached query results.
pub struct AnalyticsState {
    pub tab: AnalyticsTab,
    pub granularity: TrendsGranularity,
    pub scroll: u16,
    pub loaded: bool,
    pub stats: Option<Stats>,
    pub top_attendees: Vec<AttendeeFrequency>,
    pub labels: Vec<LabelFrequency>,
    pub weekly_counts: Vec<WeeklyCount>,
    pub daily_counts: Vec<DailyCount>,
    pub monthly_counts: Vec<MonthlyCount>,
    pub avg_duration: f64,
    pub total_hours: f64,
}

impl Default for AnalyticsState {
    fn default() -> Self {
        Self {
            tab: AnalyticsTab::Dashboard,
            granularity: TrendsGranularity::Weekly,
            scroll: 0,
            loaded: false,
            stats: None,
            top_attendees: Vec::new(),
            labels: Vec::new(),
            weekly_counts: Vec::new(),
            daily_counts: Vec::new(),
            monthly_counts: Vec::new(),
            avg_duration: 0.0,
            total_hours: 0.0,
        }
    }
}
```

Add `view` and `analytics` fields to `App` struct:
```rust
pub view: View,
pub analytics: AnalyticsState,
```

Initialize them in `App::new()`:
```rust
view: View::Meetings,
analytics: AnalyticsState::default(),
```

Add methods to `App`:
```rust
/// Toggle between Meetings and Analytics views.
pub fn toggle_view(&mut self) {
    self.view = match self.view {
        View::Meetings => View::Analytics,
        View::Analytics => View::Meetings,
    };
}

/// Switch to the next analytics sub-tab.
pub fn analytics_next_tab(&mut self) {
    self.analytics.tab = match self.analytics.tab {
        AnalyticsTab::Dashboard => AnalyticsTab::Trends,
        AnalyticsTab::Trends => AnalyticsTab::Dashboard,
    };
    self.analytics.scroll = 0;
}

/// Switch to the previous analytics sub-tab.
pub fn analytics_prev_tab(&mut self) {
    self.analytics_next_tab();
}
```

**Step 4: Run all tests to verify they pass**

Run: `cargo test --lib --all-features -- tui::app`
Expected: PASS (all existing + new tests)

**Step 5: Commit**

```
agentjj commit -m "feat: add View, AnalyticsTab, and AnalyticsState to TUI app"
```

---

### Task 3: Add Analytics Keybindings to Events

**Files:**
- Modify: `src/tui/events.rs`

**Step 1: Write the failing tests**

Add to `#[cfg(test)] mod tests` in `src/tui/events.rs`:

```rust
#[test]
fn test_a_toggles_to_analytics() {
    let mut app = make_app();
    assert_eq!(app.view, View::Meetings);
    handle_key_event(&mut app, key(KeyCode::Char('a')));
    assert_eq!(app.view, View::Analytics);
}

#[test]
fn test_a_toggles_back_to_meetings() {
    let mut app = make_app();
    app.view = View::Analytics;
    handle_key_event(&mut app, key(KeyCode::Char('a')));
    assert_eq!(app.view, View::Meetings);
}

#[test]
fn test_esc_from_analytics_returns_to_meetings() {
    let mut app = make_app();
    app.view = View::Analytics;
    handle_key_event(&mut app, key(KeyCode::Esc));
    assert_eq!(app.view, View::Meetings);
    assert!(!app.should_quit);
}

#[test]
fn test_analytics_tab_switch_with_numbers() {
    let mut app = make_app();
    app.view = View::Analytics;
    handle_key_event(&mut app, key(KeyCode::Char('2')));
    assert_eq!(app.analytics.tab, AnalyticsTab::Trends);
    handle_key_event(&mut app, key(KeyCode::Char('1')));
    assert_eq!(app.analytics.tab, AnalyticsTab::Dashboard);
}

#[test]
fn test_analytics_tab_switch_with_arrows() {
    let mut app = make_app();
    app.view = View::Analytics;
    handle_key_event(&mut app, key(KeyCode::Right));
    assert_eq!(app.analytics.tab, AnalyticsTab::Trends);
    handle_key_event(&mut app, key(KeyCode::Left));
    assert_eq!(app.analytics.tab, AnalyticsTab::Dashboard);
}

#[test]
fn test_analytics_scroll() {
    let mut app = make_app();
    app.view = View::Analytics;
    handle_key_event(&mut app, key(KeyCode::Char('j')));
    assert_eq!(app.analytics.scroll, 1);
    handle_key_event(&mut app, key(KeyCode::Char('k')));
    assert_eq!(app.analytics.scroll, 0);
}

#[test]
fn test_analytics_granularity_keys() {
    let mut app = make_app();
    app.view = View::Analytics;
    app.analytics.tab = AnalyticsTab::Trends;

    handle_key_event(&mut app, key(KeyCode::Char('d')));
    assert_eq!(app.analytics.granularity, TrendsGranularity::Daily);
    handle_key_event(&mut app, key(KeyCode::Char('w')));
    assert_eq!(app.analytics.granularity, TrendsGranularity::Weekly);
    handle_key_event(&mut app, key(KeyCode::Char('m')));
    assert_eq!(app.analytics.granularity, TrendsGranularity::Monthly);
}

#[test]
fn test_q_quits_from_analytics() {
    let mut app = make_app();
    app.view = View::Analytics;
    handle_key_event(&mut app, key(KeyCode::Char('q')));
    assert!(app.should_quit);
}

#[test]
fn test_analytics_refresh_flag() {
    let mut app = make_app();
    app.view = View::Analytics;
    app.analytics.loaded = true;
    handle_key_event(&mut app, key(KeyCode::Char('r')));
    assert!(!app.analytics.loaded);
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib --all-features test_a_toggles`
Expected: FAIL — `View` not imported, `view` field not found

**Step 3: Write minimal implementation**

Update the import in `events.rs` to include new types:
```rust
use super::app::{AnalyticsTab, App, FocusedPane, Mode, TrendsGranularity, View};
```

Update `handle_key_event` to dispatch based on view:
```rust
pub fn handle_key_event(app: &mut App, key: KeyEvent) {
    match app.view {
        View::Meetings => match app.mode {
            Mode::Normal => handle_normal_mode(app, key),
            Mode::Search => handle_search_mode(app, key),
            Mode::AttendeeFilter => handle_attendee_filter_mode(app, key),
        },
        View::Analytics => handle_analytics_mode(app, key),
    }
}
```

Add `a` key handling to `handle_normal_mode`:
```rust
KeyCode::Char('a') => {
    app.toggle_view();
}
```

Add the analytics handler function:
```rust
fn handle_analytics_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => {
            app.should_quit = true;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        KeyCode::Esc | KeyCode::Char('a') => {
            app.view = View::Meetings;
        }
        KeyCode::Char('1') => {
            app.analytics.tab = AnalyticsTab::Dashboard;
            app.analytics.scroll = 0;
        }
        KeyCode::Char('2') => {
            app.analytics.tab = AnalyticsTab::Trends;
            app.analytics.scroll = 0;
        }
        KeyCode::Right => {
            app.analytics_next_tab();
        }
        KeyCode::Left => {
            app.analytics_prev_tab();
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.analytics.scroll = app.analytics.scroll.saturating_add(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.analytics.scroll = app.analytics.scroll.saturating_sub(1);
        }
        KeyCode::Char('d') => {
            app.analytics.granularity = TrendsGranularity::Daily;
        }
        KeyCode::Char('w') => {
            app.analytics.granularity = TrendsGranularity::Weekly;
        }
        KeyCode::Char('m') => {
            app.analytics.granularity = TrendsGranularity::Monthly;
        }
        KeyCode::Char('r') => {
            app.analytics.loaded = false;
        }
        _ => {}
    }
}
```

**Step 4: Update test imports**

Add `View`, `AnalyticsTab`, `TrendsGranularity` to the `use super::*` import in the test module if needed. Since tests use `use super::*`, the new types from `app` should be available via the re-export in the `use super::app::...` import at the top.

**Step 5: Run all event tests to verify they pass**

Run: `cargo test --lib --all-features -- tui::events`
Expected: PASS

**Step 6: Commit**

```
agentjj commit -m "feat: add analytics keybindings to TUI events"
```

---

### Task 4: Add Bar Chart Helper and Analytics Rendering

**Files:**
- Modify: `src/tui/ui.rs`

**Step 1: Write the failing test for `render_bar`**

Add to `#[cfg(test)] mod tests` in `src/tui/ui.rs`:

```rust
#[test]
fn test_render_bar_full() {
    let bar = render_bar(10, 10, 10);
    assert_eq!(bar, "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}");
}

#[test]
fn test_render_bar_half() {
    let bar = render_bar(5, 10, 10);
    assert_eq!(bar, "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}");
}

#[test]
fn test_render_bar_empty() {
    let bar = render_bar(0, 10, 10);
    assert_eq!(bar, "\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}");
}

#[test]
fn test_render_bar_zero_max() {
    let bar = render_bar(5, 0, 10);
    assert_eq!(bar, "\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}");
}

#[test]
fn test_format_compact_small() {
    assert_eq!(format_compact(42), "42");
    assert_eq!(format_compact(999), "999");
}

#[test]
fn test_format_compact_thousands() {
    assert_eq!(format_compact(1500), "1.5K");
    assert_eq!(format_compact(10000), "10.0K");
}

#[test]
fn test_format_compact_millions() {
    assert_eq!(format_compact(1500000), "1.5M");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib --all-features test_render_bar`
Expected: FAIL — `render_bar` not found

**Step 3: Write bar chart helper and compact formatter**

Add these functions to `src/tui/ui.rs` (above the `draw` function):

```rust
/// Render an ASCII horizontal bar chart segment.
/// Returns a string of filled and empty block characters.
fn render_bar(value: i64, max: i64, width: usize) -> String {
    if max == 0 {
        return "\u{2591}".repeat(width);
    }
    let filled = ((value as f64 / max as f64) * width as f64) as usize;
    let filled = filled.min(width);
    let empty = width - filled;
    format!(
        "{}{}",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(empty),
    )
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
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib --all-features test_render_bar test_format_compact`
Expected: PASS

**Step 5: Write the analytics draw functions**

Update `draw()` to dispatch based on view:

```rust
pub fn draw(frame: &mut Frame, app: &App) {
    match app.view {
        View::Meetings => draw_meetings_view(frame, app),
        View::Analytics => draw_analytics_view(frame, app),
    }
}

fn draw_meetings_view(frame: &mut Frame, app: &App) {
    // ... existing draw logic (search bar, main content, help bar, attendee popup)
}
```

Move the existing body of `draw()` into `draw_meetings_view()`.

Add the analytics rendering functions:

```rust
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
            stats.total_meetings, stats.unique_attendees, stats.meetings_per_week
        )));
        lines.push(Line::from(format!(
            "  {:.1} hrs total | {:.0} min avg",
            app.analytics.total_hours, app.analytics.avg_duration
        )));
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
            "  {:<20} ({:>3}) {}",
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
    let label_max = app
        .analytics
        .labels
        .first()
        .map(|l| l.count)
        .unwrap_or(1);
    for label in app.analytics.labels.iter().take(10) {
        let bar = render_bar(label.count, label_max, 20);
        lines.push(Line::from(format!(
            "  {:<20} ({:>3}) {}",
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

fn draw_trends_tab(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    let granularity_label = match app.analytics.granularity {
        TrendsGranularity::Daily => "Daily",
        TrendsGranularity::Weekly => "Weekly",
        TrendsGranularity::Monthly => "Monthly",
    };

    lines.push(Line::from(Span::styled(
        format!("--- {} Meeting Volume ---", granularity_label),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));

    match app.analytics.granularity {
        TrendsGranularity::Daily => {
            let max = app.analytics.daily_counts.iter().map(|d| d.count).max().unwrap_or(1);
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
            let max = app.analytics.weekly_counts.iter().map(|w| w.count).max().unwrap_or(1);
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
            let max = app.analytics.monthly_counts.iter().map(|m| m.count).max().unwrap_or(1);
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
    lines.push(Line::from(Span::styled(
        "--- Duration ---",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(format!(
        "  Avg: {:.0} min | Total: {:.1} hrs",
        app.analytics.avg_duration, app.analytics.total_hours
    )));

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

fn draw_analytics_help_bar(frame: &mut Frame, app: &App, area: Rect) {
    let help_text = match app.analytics.tab {
        AnalyticsTab::Dashboard => {
            "[a] meetings  [1/2] tabs  [j/k] scroll  [r] refresh  [q] quit"
        }
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

/// Truncate a string to a maximum width, appending "..." if needed.
fn truncate_str(s: &str, max_width: usize) -> String {
    if s.len() <= max_width {
        format!("{:<width$}", s, width = max_width)
    } else {
        format!("{}...", &s[..max_width.saturating_sub(3)])
    }
}
```

**Step 6: Update imports in ui.rs**

Add `View` to the import:
```rust
use super::app::{App, FocusedPane, Mode, View, AnalyticsTab};
```

**Step 7: Run all tests to verify they pass**

Run: `cargo test --lib --all-features -- tui::ui`
Expected: PASS

**Step 8: Commit**

```
agentjj commit -m "feat: add analytics view rendering with bar charts and tab navigation"
```

---

### Task 5: Wire Up Lazy Loading in the Run Loop

**Files:**
- Modify: `src/tui/run.rs`

**Step 1: Add the `load_analytics` function**

Add after the `load_preview` function in `src/tui/run.rs`:

```rust
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
    app.analytics.loaded = true;
}
```

**Step 2: Add lazy loading trigger in `run_loop`**

Inside `run_loop`, after the `handle_key_event` call and before the `should_quit` check, add:

```rust
// Lazy-load analytics data on first switch (or refresh)
if app.view == super::app::View::Analytics && !app.analytics.loaded {
    load_analytics(app, conn);
}
```

Add the import at the top of run.rs:
```rust
use super::app::View;
```

**Step 3: Run full test suite to verify compilation and tests pass**

Run: `cargo test --lib --all-features --no-fail-fast`
Expected: PASS

Run: `cargo clippy --all-features -- -D warnings`
Expected: No warnings

**Step 4: Commit**

```
agentjj commit -m "feat: wire up lazy analytics loading in TUI run loop"
```

---

### Task 6: Final Integration Test and Cleanup

**Step 1: Run the full test suite**

Run: `cargo test --lib --all-features --no-fail-fast`
Expected: All tests PASS

**Step 2: Run clippy**

Run: `cargo clippy --all-features -- -D warnings`
Expected: No warnings

**Step 3: Run the TUI manually to verify**

Run: `cargo run --all-features -- tui`

Verify:
- Press `a` to switch to Analytics view
- Tab bar shows `[1 Dashboard] [2 Trends]`
- Dashboard shows Summary, Top Attendees, Labels sections with bar charts
- Press `2` to switch to Trends tab
- Press `d`, `w`, `m` to toggle granularity
- Press `j`/`k` to scroll
- Press `r` to refresh
- Press `a` or `Esc` to return to Meetings
- `q` quits from either view

**Step 4: Commit if any cleanup was needed**

```
agentjj commit -m "chore: cleanup and verify analytics TUI integration"
```
