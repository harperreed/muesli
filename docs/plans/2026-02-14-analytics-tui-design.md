# Analytics Dashboard for Muesli TUI

**Date**: 2026-02-14
**Status**: Approved
**Inspired by**: [msgvault](https://github.com/wesm/msgvault) (Wes McKinney) and ccvault

## Summary

Add an analytics dashboard to the muesli TUI that surfaces meeting insights via
DuckDB-powered aggregation queries. The analytics view is accessible via a
tab-switch (`a` key) from the existing meetings view, and contains two sub-tabs:
a stacked all-in-one Dashboard and a time-series Trends view.

## Design Decisions

- **Approach**: View Enum + Flat State (Approach A) — minimal refactor, current
  code stays intact, analytics data loaded lazily on first switch
- **Navigation**: `a` key toggles between Meetings and Analytics views
- **Charts**: ASCII horizontal bar charts (`████████░░░░`)
- **Time granularity**: Daily, weekly, and monthly (toggled with `d`/`w`/`m`)
- **Analytics layout**: All-in-one dashboard (summary + people + labels stacked)
  plus a separate Trends tab for time-series deep dive

## Navigation Model

### Top-Level View Enum

```rust
enum View {
    Meetings,   // current layout (search + list + preview)
    Analytics,  // analytics dashboard
}
```

- `a` in Normal mode toggles between Meetings and Analytics
- Help bar updates to show context-appropriate keybindings
- `Esc` from Analytics returns to Meetings (not quit)
- `q` still quits from either view

### Analytics Sub-Tabs

```rust
enum AnalyticsTab {
    Dashboard,  // all-in-one: summary + people + labels
    Trends,     // time-series charts
}
```

- `1`/`2` or `←`/`→` switch sub-tabs
- Active tab visually highlighted

## Data Model

### New Types in app.rs

```rust
enum TrendsGranularity {
    Daily,
    Weekly,
    Monthly,
}

struct AnalyticsState {
    tab: AnalyticsTab,
    granularity: TrendsGranularity,
    scroll: u16,
    // Cached query results
    stats: Option<Stats>,
    top_attendees: Vec<AttendeeFrequency>,
    labels: Vec<LabelFrequency>,
    weekly_counts: Vec<WeeklyCount>,
    daily_counts: Vec<DailyCount>,
    monthly_counts: Vec<MonthlyCount>,
    avg_duration: f64,
    total_hours: f64,
}
```

Data loaded lazily on first switch to Analytics view. Refresh with `r`.

## Dashboard Tab Layout (All-in-One)

```
┌─────────────────── Analytics ───────────────────┐
│ [1 Dashboard]  [2 Trends]            [r]efresh  │
├──────────────────────────────────────────────────┤
│ ┌── Summary ──────────────────────────────────┐  │
│ │ 142 meetings | 47 attendees | 3.2/wk        │  │
│ │ 237.5 hrs total | 48 min avg | 12 wk span   │  │
│ └─────────────────────────────────────────────┘  │
│                                                  │
│ ┌── Top Attendees ────────────────────────────┐  │
│ │ Alice Johnson  (28) ████████████████░░░░░░  │  │
│ │ Bob Smith      (21) ████████████░░░░░░░░░░  │  │
│ │ Charlie Wu     (15) ████████░░░░░░░░░░░░░░  │  │
│ │ Dana Lee       (12) ██████░░░░░░░░░░░░░░░░  │  │
│ │ Eve Park        (8) ████░░░░░░░░░░░░░░░░░░  │  │
│ └─────────────────────────────────────────────┘  │
│                                                  │
│ ┌── Labels ───────────────────────────────────┐  │
│ │ Planning   (34) ████████████████████░░░░░░  │  │
│ │ 1:1        (28) ████████████████░░░░░░░░░░  │  │
│ │ Review     (19) ███████████░░░░░░░░░░░░░░░  │  │
│ │ Standup    (15) █████████░░░░░░░░░░░░░░░░░  │  │
│ └─────────────────────────────────────────────┘  │
├──────────────────────────────────────────────────┤
│ [a] meetings  [1/2] tabs  [j/k] scroll  [q] quit│
└──────────────────────────────────────────────────┘
```

## Trends Tab Layout

```
┌─────────────────── Analytics ───────────────────┐
│ [1 Dashboard]  [2 Trends]   [d]aily [w]eek [m]o │
├──────────────────────────────────────────────────┤
│ ┌── Weekly Meeting Volume (12 weeks) ─────────┐  │
│ │ 2026-W07  (8) ████████████████████████░░░░  │  │
│ │ 2026-W06  (5) ███████████████░░░░░░░░░░░░░  │  │
│ │ 2026-W05  (7) █████████████████████░░░░░░░  │  │
│ │ 2026-W04  (3) █████████░░░░░░░░░░░░░░░░░░  │  │
│ │ 2026-W03  (6) ██████████████████░░░░░░░░░░  │  │
│ │ 2026-W02  (4) ████████████░░░░░░░░░░░░░░░░  │  │
│ └─────────────────────────────────────────────┘  │
│                                                  │
│ ┌── Duration Trend ───────────────────────────┐  │
│ │ Avg: 48 min | Longest week: W07 (62 min)    │  │
│ └─────────────────────────────────────────────┘  │
├──────────────────────────────────────────────────┤
│ [a] meetings  [1/2] tabs  [d/w/m] grain  [q]quit│
└──────────────────────────────────────────────────┘
```

Granularity toggles: `d` daily, `w` weekly, `m` monthly.

## New DB Queries

Two new queries in `queries.rs` (same pattern as existing `meetings_per_week`):

1. **`meetings_per_day(days: usize)`** — Group by day, return `Vec<DailyCount>`
2. **`meetings_per_month(months: usize)`** — Group by `%Y-%m`, return `Vec<MonthlyCount>`

## File Changes

| File | Change |
|------|--------|
| `src/tui/app.rs` | Add `View`, `AnalyticsTab`, `TrendsGranularity`, `AnalyticsState` to `App` |
| `src/tui/ui.rs` | Add `draw_analytics()`, `draw_dashboard_tab()`, `draw_trends_tab()`, `draw_bar_chart()` |
| `src/tui/events.rs` | Handle `a` for view switch, `1`/`2` for tabs, `d`/`w`/`m` for granularity, analytics scroll |
| `src/tui/run.rs` | Load analytics data lazily on first switch, pass `conn` for refresh |
| `src/db/queries.rs` | Add `meetings_per_day()`, `meetings_per_month()`, `DailyCount`, `MonthlyCount` |

No new files — everything extends existing modules.

## Keybinding Summary

### Meetings View (unchanged except `a`)
- `a` — switch to Analytics
- All existing keys unchanged

### Analytics View
- `a` — switch to Meetings
- `1`/`2` or `←`/`→` — switch sub-tabs
- `j`/`k` or `↑`/`↓` — scroll
- `d`/`w`/`m` — granularity toggle (Trends tab only)
- `r` — refresh data
- `Esc` — return to Meetings
- `q` — quit
