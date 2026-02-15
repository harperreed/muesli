# Enhanced Trends Tab Design

**Date**: 2026-02-14
**Status**: Approved

## Summary

Enhance the Trends tab in the analytics TUI from a single volume chart into
four scrollable sections: Rhythm (meeting patterns), People (collaborator
dynamics), Content (label/topic shifts), and Superlatives (fun one-liner stats).

## Trends Layout

The Trends tab becomes a single scrollable view with four stacked sections
separated by styled headers. The existing granularity toggle (`d/w/m`) applies
only to the volume chart in Section 1. All other sections use their natural
time granularity.

```
┌─────────────────── Analytics ───────────────────┐
│ [1 Dashboard]  [2 Trends]   [d]aily [w]eek [m]o │
├──────────────────────────────────────────────────┤
│ ┌── Rhythm ─────────────────────────────────────┐│
│ │ Weekly Meeting Volume (12 weeks)              ││
│ │ 2026-W07  (8) ████████████████████████░░░░    ││
│ │ 2026-W06  (5) ███████████████░░░░░░░░░░░░░   ││
│ │ ...                                           ││
│ │                                               ││
│ │ Day of Week                                   ││
│ │ Mon  (92) ████████████████████░░░░░░░░░░      ││
│ │ Tue (118) ████████████████████████████░░      ││
│ │ Wed (104) ████████████████████████░░░░░░      ││
│ │ Thu  (87) ██████████████████░░░░░░░░░░░░      ││
│ │ Fri  (61) ████████████░░░░░░░░░░░░░░░░░░      ││
│ │ Sat   (2) ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░      ││
│ │ Sun   (0) ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░      ││
│ │                                               ││
│ │ Hour of Day                                   ││
│ │ 08:00  (12) ████░░░░░░░░░░░░░░░░░░░░░░░      ││
│ │ 09:00  (45) ████████████████░░░░░░░░░░░      ││
│ │ 10:00  (78) ████████████████████████████      ││
│ │ ...                                           ││
│ │                                               ││
│ │ Avg Attendees/Meeting (by month)              ││
│ │ 2026-02  (4.2) ████████████████░░░░░░░░      ││
│ │ 2026-01  (3.8) ██████████████░░░░░░░░░░      ││
│ │ ...                                           ││
│ └───────────────────────────────────────────────┘│
│                                                  │
│ ┌── People ─────────────────────────────────────┐│
│ │ Top Collaborators (last 30 days)              ││
│ │ James Cham       (8) ████████████████████░░░  ││
│ │ Kristopher Kub.. (5) ████████████░░░░░░░░░░░  ││
│ │ ...                                           ││
│ │                                               ││
│ │ New Faces This Month: 12                      ││
│ │   Alice Johnson, Bob Smith, Charlie Wu, ...   ││
│ │                                               ││
│ │ Top Companies                                 ││
│ │ Acme Corp  (34) ████████████████████░░░░░░░░  ││
│ │ Initech    (21) ████████████░░░░░░░░░░░░░░░░  ││
│ │ ...                                           ││
│ └───────────────────────────────────────────────┘│
│                                                  │
│ ┌── Content ────────────────────────────────────┐│
│ │ Label Trends (last 6 months)                  ││
│ │          Jan  Feb  Mar  Apr  May  Jun          ││
│ │ Planning  8    6   12    9    7   11           ││
│ │ 1:1       5    7    4    6    8    5           ││
│ │ ...                                           ││
│ │                                               ││
│ │ Busiest Weeks                                 ││
│ │ 1. 2025-W38 (14 meetings)                    ││
│ │ 2. 2025-W42 (12 meetings)                    ││
│ │ ...                                           ││
│ └───────────────────────────────────────────────┘│
│                                                  │
│ ┌── Superlatives ───────────────────────────────┐│
│ │ Marathon Meeting: "Q4 Strategy Offsite"       ││
│ │   2025-09-15 | 3h 24m                        ││
│ │                                               ││
│ │ Social Butterfly: James Cham                  ││
│ │   Met with 42 different people                ││
│ │                                               ││
│ │ Could've Been an Email: 8 meetings            ││
│ │   5+ attendees, under 15 minutes              ││
│ │                                               ││
│ │ Meeting Streak: 23 consecutive days           ││
│ │   2025-10-01 to 2025-10-23                    ││
│ │                                               ││
│ │ Solo Meetings: 42                             ││
│ │   You and your thoughts                       ││
│ │                                               ││
│ │ Recurring Champion: "Nerd Immunity"           ││
│ │   29 occurrences since 2024-04-12             ││
│ └───────────────────────────────────────────────┘│
├──────────────────────────────────────────────────┤
│ [a] meetings  [1/2] tabs  [d/w/m] grain  [q]quit│
└──────────────────────────────────────────────────┘
```

## New Data Types

```rust
struct DayOfWeekCount {
    day_name: String,   // "Mon", "Tue", ...
    day_num: i64,       // 0=Mon, 6=Sun (ISO weekday)
    count: i64,
}

struct HourOfDayCount {
    hour: i64,          // 0-23
    count: i64,
}

struct MeetingSizeByMonth {
    month: String,      // "2026-02"
    avg_attendees: f64,
}

struct RecentCollaborator {
    name: String,
    count: i64,
}

struct CompanyFrequency {
    company: String,
    count: i64,
}

struct LabelByMonth {
    label: String,
    month: String,
    count: i64,
}

struct Superlatives {
    marathon: Option<(String, String, i64)>,     // (title, date, seconds)
    social_butterfly: Option<(String, i64)>,     // (name, unique_co_attendees)
    email_meetings: i64,                         // count of 5+ attendees, <15min
    streak_days: i64,                            // consecutive days with meetings
    streak_start: Option<String>,                // start date
    streak_end: Option<String>,                  // end date
    solo_meetings: i64,                          // meetings with 0-1 attendees
    recurring_champ: Option<(String, i64)>,      // (title, count)
}
```

## New DB Queries

All queries go in `src/db/queries.rs`:

1. **`meetings_by_weekday()`** — `GROUP BY isodow(CAST(created_at AS TIMESTAMP))`
2. **`meetings_by_hour()`** — `GROUP BY hour(CAST(created_at AS TIMESTAMP))`
3. **`avg_attendees_by_month(months)`** — Join documents+attendees, avg per month
4. **`top_collaborators_recent(days, limit)`** — Attendees in last N days
5. **`new_attendees_this_month()`** — Names that first appeared this month
6. **`top_companies(limit)`** — Group attendees by company_name
7. **`label_trends(months)`** — Label counts by month (pivotable)
8. **`busiest_weeks(limit)`** — Top N weeks by meeting count
9. **`superlatives()`** — Single function returning the Superlatives struct:
   - Marathon: `ORDER BY duration_seconds DESC LIMIT 1`
   - Social butterfly: Attendee with most distinct co-attendees via self-join
   - Email meetings: `WHERE attendee_count >= 5 AND duration_seconds < 900`
   - Streak: Window function over distinct meeting days
   - Solo: `LEFT JOIN attendees ... HAVING count(a.id) <= 1`
   - Recurring champ: `GROUP BY title ORDER BY count(*) DESC LIMIT 1`

## AnalyticsState Changes

Add new fields to `AnalyticsState` in `src/tui/app.rs`:

```rust
// Rhythm
pub weekday_counts: Vec<DayOfWeekCount>,
pub hourly_counts: Vec<HourOfDayCount>,
pub meeting_size_trend: Vec<MeetingSizeByMonth>,

// People
pub recent_collaborators: Vec<RecentCollaborator>,
pub new_faces: Vec<String>,
pub top_companies: Vec<CompanyFrequency>,

// Content
pub label_trends: Vec<LabelByMonth>,
pub busiest_weeks: Vec<WeeklyCount>,

// Superlatives
pub superlatives: Option<Superlatives>,
```

All loaded lazily alongside existing analytics data in `load_analytics()`.

## File Changes

| File | Change |
|------|--------|
| `src/db/queries.rs` | Add 9 new query functions + 8 new structs |
| `src/tui/app.rs` | Extend AnalyticsState with new fields |
| `src/tui/ui.rs` | Replace `draw_trends_tab()` with 4 section renderers |
| `src/tui/run.rs` | Load new data in `load_analytics()` |

No new files.

## Keybindings

No changes — existing `j/k` scroll, `d/w/m` granularity (Section 1 only),
`r` refresh all work as-is.

## Graceful Degradation

- Duration-dependent stats (marathon, email meetings) show "N/A" when all
  durations are NULL
- Company breakdown shows "No company data" when all company_name are NULL
- Superlatives that can't be computed are omitted from the display
