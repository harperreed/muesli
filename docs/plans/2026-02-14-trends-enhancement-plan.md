# Enhanced Trends Tab Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Expand the Trends tab from a single volume chart into four rich sections: Rhythm, People, Content, and Superlatives.

**Architecture:** Add 9 new DuckDB query functions and 8 new types to `queries.rs`, extend `AnalyticsState` in `app.rs` with new fields, replace `draw_trends_tab()` in `ui.rs` with section renderers, and wire everything through `load_analytics()` in `run.rs`.

**Tech Stack:** Rust, DuckDB (SQL with `strftime`, `isodow`, `hour`, window functions), ratatui (TUI rendering)

---

### Task 1: Rhythm Queries — Weekday, Hour, Meeting Size

**Files:**
- Modify: `src/db/queries.rs`

**Context:** All existing query functions follow the same pattern — prepare a statement, query_map with params, collect into a Vec of a typed struct. The time column is `created_at` stored as VARCHAR (RFC3339). Cast to TIMESTAMP for DuckDB time functions. See `meetings_per_week()` at line 389 for the template.

**Step 1: Add the three new structs**

Add after `MonthlyCount` (line 372):

```rust
/// Meeting count by day of week (Monday=1 through Sunday=7).
#[derive(Debug, Clone)]
pub struct DayOfWeekCount {
    pub day_name: String,
    pub day_num: i64,
    pub count: i64,
}

/// Meeting count by hour of day (0-23).
#[derive(Debug, Clone)]
pub struct HourOfDayCount {
    pub hour: i64,
    pub count: i64,
}

/// Average attendee count per meeting, grouped by month.
#[derive(Debug, Clone)]
pub struct MeetingSizeByMonth {
    pub month: String,
    pub avg_attendees: f64,
}
```

**Step 2: Add `meetings_by_weekday()` query function**

Add after `meetings_per_month()` (line 452):

```rust
/// Get meeting counts grouped by day of week (ISO: Monday=1, Sunday=7).
pub fn meetings_by_weekday(conn: &Connection) -> Result<Vec<DayOfWeekCount>> {
    let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let mut stmt = conn.prepare(
        "SELECT isodow(CAST(created_at AS TIMESTAMP)) as dow, count(*) as cnt
         FROM documents
         GROUP BY dow
         ORDER BY dow",
    )?;
    let rows = stmt.query_map([], |row| {
        let dow: i64 = row.get(0)?;
        Ok(DayOfWeekCount {
            day_name: day_names.get((dow - 1) as usize).unwrap_or(&"?").to_string(),
            day_num: dow,
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

**Step 3: Add `meetings_by_hour()` query function**

```rust
/// Get meeting counts grouped by hour of day (0-23).
pub fn meetings_by_hour(conn: &Connection) -> Result<Vec<HourOfDayCount>> {
    let mut stmt = conn.prepare(
        "SELECT hour(CAST(created_at AS TIMESTAMP)) as hr, count(*) as cnt
         FROM documents
         GROUP BY hr
         ORDER BY hr",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(HourOfDayCount {
            hour: row.get(0)?,
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

**Step 4: Add `avg_attendees_by_month()` query function**

```rust
/// Get average attendees per meeting, grouped by month.
pub fn avg_attendees_by_month(conn: &Connection, months: usize) -> Result<Vec<MeetingSizeByMonth>> {
    let mut stmt = conn.prepare(
        "SELECT strftime(CAST(d.created_at AS TIMESTAMP), '%Y-%m') as month,
                avg(att_count) as avg_att
         FROM documents d
         LEFT JOIN (
             SELECT doc_id, count(*) as att_count FROM attendees GROUP BY doc_id
         ) a ON d.doc_id = a.doc_id
         WHERE CAST(d.created_at AS TIMESTAMP) >= current_date - CAST(? AS INTEGER) * INTERVAL 30 DAY
         GROUP BY month
         ORDER BY month DESC",
    )?;
    let rows = stmt.query_map(params![months as i64], |row| {
        Ok(MeetingSizeByMonth {
            month: row.get(0)?,
            avg_attendees: row.get::<_, Option<f64>>(1)?.unwrap_or(0.0),
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}
```

**Step 5: Write tests**

Add in the `tests` module at the bottom of queries.rs:

```rust
#[test]
fn test_meetings_by_weekday() {
    let conn = open_in_memory().unwrap();
    let meta = make_test_metadata("Meeting", &[], &[]);
    upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

    let counts = meetings_by_weekday(&conn).unwrap();
    assert!(!counts.is_empty());
    let total: i64 = counts.iter().map(|c| c.count).sum();
    assert_eq!(total, 1);
    // Verify day_name is valid
    for c in &counts {
        assert!(["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].contains(&c.day_name.as_str()));
    }
}

#[test]
fn test_meetings_by_hour() {
    let conn = open_in_memory().unwrap();
    let meta = make_test_metadata("Meeting", &[], &[]);
    upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

    let counts = meetings_by_hour(&conn).unwrap();
    assert!(!counts.is_empty());
    let total: i64 = counts.iter().map(|c| c.count).sum();
    assert_eq!(total, 1);
    for c in &counts {
        assert!(c.hour >= 0 && c.hour <= 23);
    }
}

#[test]
fn test_avg_attendees_by_month() {
    let conn = open_in_memory().unwrap();
    let meta = make_metadata_with_attendees("Meeting", &["Alice", "Bob"], &[]);
    upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

    let sizes = avg_attendees_by_month(&conn, 24).unwrap();
    assert!(!sizes.is_empty());
    assert!(sizes[0].avg_attendees >= 1.0);
}
```

**Step 6: Run tests**

Run: `cargo test --lib db::queries::tests`
Expected: All tests pass including 3 new ones.

**Step 7: Commit**

```
git add src/db/queries.rs
git commit -m "feat: add rhythm queries (weekday, hour, meeting size)"
```

---

### Task 2: People Queries — Collaborators, New Faces, Companies

**Files:**
- Modify: `src/db/queries.rs`

**Step 1: Add new structs**

Add after the structs from Task 1:

```rust
/// An attendee with their recent meeting frequency.
#[derive(Debug, Clone)]
pub struct RecentCollaborator {
    pub name: String,
    pub count: i64,
}

/// A company with its meeting frequency.
#[derive(Debug, Clone)]
pub struct CompanyFrequency {
    pub company: String,
    pub count: i64,
}
```

**Step 2: Add `top_collaborators_recent()` query function**

```rust
/// Get top attendees from the last N days.
pub fn top_collaborators_recent(conn: &Connection, days: usize, limit: usize) -> Result<Vec<RecentCollaborator>> {
    let mut stmt = conn.prepare(
        "SELECT a.name, count(*) as cnt
         FROM attendees a
         JOIN documents d ON a.doc_id = d.doc_id
         WHERE a.name IS NOT NULL
           AND CAST(d.created_at AS TIMESTAMP) >= current_date - CAST(? AS INTEGER) * INTERVAL 1 DAY
         GROUP BY a.name
         ORDER BY cnt DESC
         LIMIT ?",
    )?;
    let rows = stmt.query_map(params![days as i64, limit as i64], |row| {
        Ok(RecentCollaborator {
            name: row.get(0)?,
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

**Step 3: Add `new_attendees_this_month()` query function**

```rust
/// Get attendees who first appeared this month (never seen in prior months).
pub fn new_attendees_this_month(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT a.name
         FROM attendees a
         JOIN documents d ON a.doc_id = d.doc_id
         WHERE a.name IS NOT NULL
           AND strftime(CAST(d.created_at AS TIMESTAMP), '%Y-%m') = strftime(current_date, '%Y-%m')
           AND a.name NOT IN (
               SELECT DISTINCT a2.name
               FROM attendees a2
               JOIN documents d2 ON a2.doc_id = d2.doc_id
               WHERE a2.name IS NOT NULL
                 AND CAST(d2.created_at AS TIMESTAMP) < date_trunc('month', current_date)
           )
         GROUP BY a.name
         ORDER BY a.name",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}
```

**Step 4: Add `top_companies()` query function**

```rust
/// Get the top N companies by meeting frequency.
pub fn top_companies(conn: &Connection, limit: usize) -> Result<Vec<CompanyFrequency>> {
    let mut stmt = conn.prepare(
        "SELECT company_name, count(DISTINCT doc_id) as cnt
         FROM attendees
         WHERE company_name IS NOT NULL AND company_name != ''
         GROUP BY company_name
         ORDER BY cnt DESC
         LIMIT ?",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok(CompanyFrequency {
            company: row.get(0)?,
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

**Step 5: Write tests**

```rust
#[test]
fn test_top_collaborators_recent() {
    let conn = open_in_memory().unwrap();
    let mut meta = make_metadata_with_attendees("Meeting", &["Alice", "Bob"], &[]);
    meta.created_at = Utc::now();
    upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

    let collabs = top_collaborators_recent(&conn, 30, 10).unwrap();
    assert_eq!(collabs.len(), 2);
    assert_eq!(collabs[0].count, 1);
}

#[test]
fn test_new_attendees_this_month() {
    let conn = open_in_memory().unwrap();
    let mut meta = make_metadata_with_attendees("Meeting", &["Alice"], &[]);
    meta.created_at = Utc::now();
    upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

    let new_faces = new_attendees_this_month(&conn).unwrap();
    // Alice is new because there are no prior months
    assert!(new_faces.contains(&"Alice".to_string()));
}

#[test]
fn test_top_companies() {
    let conn = open_in_memory().unwrap();
    let meta = make_metadata_with_attendees("Meeting", &["Alice"], &[]);
    upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

    let companies = top_companies(&conn, 10).unwrap();
    // make_metadata_with_attendees sets company to "Acme"
    assert_eq!(companies.len(), 1);
    assert_eq!(companies[0].company, "Acme");
}
```

**Step 6: Run tests, commit**

Run: `cargo test --lib db::queries::tests`

```
git add src/db/queries.rs
git commit -m "feat: add people queries (collaborators, new faces, companies)"
```

---

### Task 3: Content Queries — Label Trends, Busiest Weeks

**Files:**
- Modify: `src/db/queries.rs`

**Step 1: Add `LabelByMonth` struct**

```rust
/// A label count for a specific month, used for label trend analysis.
#[derive(Debug, Clone)]
pub struct LabelByMonth {
    pub label: String,
    pub month: String,
    pub count: i64,
}
```

**Step 2: Add `label_trends()` query function**

```rust
/// Get label counts by month for the last N months.
pub fn label_trends(conn: &Connection, months: usize) -> Result<Vec<LabelByMonth>> {
    let mut stmt = conn.prepare(
        "SELECT l.label,
                strftime(CAST(d.created_at AS TIMESTAMP), '%Y-%m') as month,
                count(*) as cnt
         FROM labels l
         JOIN documents d ON l.doc_id = d.doc_id
         WHERE CAST(d.created_at AS TIMESTAMP) >= current_date - CAST(? AS INTEGER) * INTERVAL 30 DAY
         GROUP BY l.label, month
         ORDER BY l.label, month",
    )?;
    let rows = stmt.query_map(params![months as i64], |row| {
        Ok(LabelByMonth {
            label: row.get(0)?,
            month: row.get(1)?,
            count: row.get(2)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}
```

**Step 3: Add `busiest_weeks()` query function**

```rust
/// Get the top N busiest weeks by meeting count.
pub fn busiest_weeks(conn: &Connection, limit: usize) -> Result<Vec<WeeklyCount>> {
    let mut stmt = conn.prepare(
        "SELECT strftime(CAST(created_at AS TIMESTAMP), '%Y-W%W') as week, count(*) as cnt
         FROM documents
         GROUP BY week
         ORDER BY cnt DESC
         LIMIT ?",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok(WeeklyCount {
            week: row.get(0)?,
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

**Step 4: Write tests**

```rust
#[test]
fn test_label_trends() {
    let conn = open_in_memory().unwrap();
    let meta = make_test_metadata("Meeting", &[], &["Planning", "Review"]);
    upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

    let trends = label_trends(&conn, 24).unwrap();
    assert!(!trends.is_empty());
    let labels: Vec<&str> = trends.iter().map(|t| t.label.as_str()).collect();
    assert!(labels.contains(&"Planning"));
}

#[test]
fn test_busiest_weeks() {
    let conn = open_in_memory().unwrap();
    let meta = make_test_metadata("Meeting", &[], &[]);
    upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();
    upsert_document(&conn, &meta, "doc2", "b", None, None).unwrap();

    let weeks = busiest_weeks(&conn, 5).unwrap();
    assert!(!weeks.is_empty());
    assert_eq!(weeks[0].count, 2);
}
```

**Step 5: Run tests, commit**

```
git add src/db/queries.rs
git commit -m "feat: add content queries (label trends, busiest weeks)"
```

---

### Task 4: Superlatives Query

**Files:**
- Modify: `src/db/queries.rs`

**Step 1: Add `Superlatives` struct**

```rust
/// Fun one-liner stats about meeting history.
#[derive(Debug, Clone, Default)]
pub struct Superlatives {
    /// Longest meeting: (title, date, duration_seconds)
    pub marathon: Option<(String, String, i64)>,
    /// Person who met with the most different co-attendees: (name, unique_count)
    pub social_butterfly: Option<(String, i64)>,
    /// Count of meetings with 5+ attendees and under 15 minutes
    pub email_meetings: i64,
    /// Longest consecutive-days-with-meetings streak
    pub streak_days: i64,
    /// Start date of the streak
    pub streak_start: Option<String>,
    /// End date of the streak
    pub streak_end: Option<String>,
    /// Meetings with 0-1 attendees
    pub solo_meetings: i64,
    /// Most repeated meeting title: (title, count)
    pub recurring_champ: Option<(String, i64)>,
}
```

**Step 2: Add `superlatives()` query function**

This function runs several small queries and assembles the struct. Each query is independent and non-fatal (uses `.ok()` or `.unwrap_or_default()`).

```rust
/// Compute fun one-liner stats about meeting history.
pub fn superlatives(conn: &Connection) -> Result<Superlatives> {
    let mut s = Superlatives::default();

    // Marathon: longest meeting by duration
    s.marathon = conn
        .query_row(
            "SELECT title, strftime(CAST(created_at AS TIMESTAMP), '%Y-%m-%d'), duration_seconds
             FROM documents
             WHERE duration_seconds IS NOT NULL
             ORDER BY duration_seconds DESC
             LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?
                        .unwrap_or_else(|| "Untitled".to_string()),
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .ok();

    // Social butterfly: attendee who met with most distinct co-attendees
    s.social_butterfly = conn
        .query_row(
            "SELECT a1.name, count(DISTINCT a2.name) as co_count
             FROM attendees a1
             JOIN attendees a2 ON a1.doc_id = a2.doc_id AND a1.name != a2.name
             WHERE a1.name IS NOT NULL AND a2.name IS NOT NULL
             GROUP BY a1.name
             ORDER BY co_count DESC
             LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .ok();

    // Could've been an email: 5+ attendees, under 15 min
    s.email_meetings = conn
        .query_row(
            "SELECT count(*) FROM (
                 SELECT d.doc_id
                 FROM documents d
                 JOIN attendees a ON d.doc_id = a.doc_id
                 WHERE d.duration_seconds IS NOT NULL AND d.duration_seconds < 900
                 GROUP BY d.doc_id
                 HAVING count(a.id) >= 5
             )",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Meeting streak: longest consecutive days with at least one meeting
    // Uses DuckDB window functions on distinct meeting days
    s.streak_days = 0;
    if let Ok(mut stmt) = conn.prepare(
        "WITH meeting_days AS (
             SELECT DISTINCT CAST(CAST(created_at AS TIMESTAMP) AS DATE) as day
             FROM documents
         ),
         gaps AS (
             SELECT day,
                    day - CAST(ROW_NUMBER() OVER (ORDER BY day) AS INTEGER) * INTERVAL 1 DAY as grp
             FROM meeting_days
         ),
         streaks AS (
             SELECT min(day) as streak_start,
                    max(day) as streak_end,
                    count(*) as streak_len
             FROM gaps
             GROUP BY grp
         )
         SELECT streak_start, streak_end, streak_len
         FROM streaks
         ORDER BY streak_len DESC
         LIMIT 1",
    ) {
        if let Ok(mut rows) = stmt.query([]) {
            if let Ok(Some(row)) = rows.next() {
                let start: String = row.get(0).unwrap_or_default();
                let end: String = row.get(1).unwrap_or_default();
                let len: i64 = row.get(2).unwrap_or(0);
                s.streak_days = len;
                s.streak_start = Some(start);
                s.streak_end = Some(end);
            }
        }
    }

    // Solo meetings: 0 or 1 attendees (just you)
    s.solo_meetings = conn
        .query_row(
            "SELECT count(*) FROM (
                 SELECT d.doc_id
                 FROM documents d
                 LEFT JOIN attendees a ON d.doc_id = a.doc_id
                 GROUP BY d.doc_id
                 HAVING count(a.id) <= 1
             )",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Recurring champion: most repeated meeting title
    s.recurring_champ = conn
        .query_row(
            "SELECT title, count(*) as cnt
             FROM documents
             WHERE title IS NOT NULL AND title != ''
             GROUP BY title
             ORDER BY cnt DESC
             LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .ok();

    Ok(s)
}
```

**Step 3: Write tests**

```rust
#[test]
fn test_superlatives_empty_db() {
    let conn = open_in_memory().unwrap();
    let s = superlatives(&conn).unwrap();
    assert!(s.marathon.is_none());
    assert!(s.social_butterfly.is_none());
    assert_eq!(s.email_meetings, 0);
    assert_eq!(s.streak_days, 0);
    assert_eq!(s.solo_meetings, 0);
    assert!(s.recurring_champ.is_none());
}

#[test]
fn test_superlatives_with_data() {
    let conn = open_in_memory().unwrap();
    let meta = make_metadata_with_attendees("Standup", &["Alice", "Bob"], &[]);
    upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();
    upsert_document(&conn, &meta, "doc2", "b", None, None).unwrap();

    let s = superlatives(&conn).unwrap();
    // Marathon: duration_seconds=3600 from test metadata
    assert!(s.marathon.is_some());
    let (title, _, dur) = s.marathon.unwrap();
    assert_eq!(title, "Standup");
    assert_eq!(dur, 3600);
    // Recurring: "Standup" appears twice
    assert!(s.recurring_champ.is_some());
    let (champ_title, champ_count) = s.recurring_champ.unwrap();
    assert_eq!(champ_title, "Standup");
    assert_eq!(champ_count, 2);
}
```

**Step 4: Run tests, commit**

```
git add src/db/queries.rs
git commit -m "feat: add superlatives query (marathon, butterfly, streaks)"
```

---

### Task 5: Extend AnalyticsState with New Fields

**Files:**
- Modify: `src/tui/app.rs`

**Step 1: Update the import**

Add the new types to the import from `crate::db::queries` (line 8-10):

```rust
use crate::db::queries::{
    AttendeeFrequency, CompanyFrequency, DailyCount, DayOfWeekCount, DocumentRow,
    HourOfDayCount, LabelByMonth, LabelFrequency, MeetingSizeByMonth, MonthlyCount,
    RecentCollaborator, Stats, Superlatives, WeeklyCount,
};
```

**Step 2: Add fields to `AnalyticsState`**

Add after `total_hours` (line 62):

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

**Step 3: Update `Default` impl**

Add defaults for each new field in the `Default` impl (after line 79):

```rust
            weekday_counts: Vec::new(),
            hourly_counts: Vec::new(),
            meeting_size_trend: Vec::new(),
            recent_collaborators: Vec::new(),
            new_faces: Vec::new(),
            top_companies: Vec::new(),
            label_trends: Vec::new(),
            busiest_weeks: Vec::new(),
            superlatives: None,
```

**Step 4: Run tests**

Run: `cargo test --lib tui::app::tests`
Expected: All existing tests pass (no behavior changes, just new fields).

**Step 5: Commit**

```
git add src/tui/app.rs
git commit -m "feat: extend AnalyticsState with rhythm, people, content, superlatives"
```

---

### Task 6: Replace draw_trends_tab with Four Sections

**Files:**
- Modify: `src/tui/ui.rs`

**Context:** The current `draw_trends_tab()` (lines 521-619) renders a single volume chart and duration stats. Replace the entire function body with four stacked sections using the same `render_bar()`, `truncate_str()`, and `format_compact()` helpers that already exist. Keep the same signature.

**Step 1: Replace `draw_trends_tab()` body**

Replace the function (keeping the signature) with the four-section renderer. Each section uses a styled header line (`--- Section Name ---` in Yellow+Bold), followed by content lines, followed by a blank line separator.

The replacement is large, so here's the structure:

```rust
fn draw_trends_tab(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    // ── Section 1: Rhythm ──
    draw_rhythm_section(&mut lines, app);

    // ── Section 2: People ──
    draw_people_section(&mut lines, app);

    // ── Section 3: Content ──
    draw_content_section(&mut lines, app);

    // ── Section 4: Superlatives ──
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
```

Then implement each section as a helper function. The implementations follow the exact same pattern as the existing `draw_dashboard_tab()` — push styled header lines, then data lines with `render_bar()`.

**`draw_rhythm_section`**: Volume chart (existing d/w/m logic), then weekday bar chart, hour bar chart, meeting size trend.

**`draw_people_section`**: Recent collaborators bar chart, new faces list, company bar chart.

**`draw_content_section`**: Label trends as a simple table (label | month columns), busiest weeks ranked list.

**`draw_superlatives_section`**: Each superlative as a styled one-liner with title and detail. Use `Color::Magenta` for superlative titles to make them pop.

**Step 2: Run tests**

Run: `cargo test --lib tui::ui::tests`

**Step 3: Commit**

```
git add src/tui/ui.rs
git commit -m "feat: replace trends tab with rhythm, people, content, superlatives sections"
```

---

### Task 7: Wire Up load_analytics with New Queries

**Files:**
- Modify: `src/tui/run.rs`

**Step 1: Add new query calls to `load_analytics()`**

Add after the existing `app.analytics.total_hours` line (line 209), before `app.analytics.loaded = true`:

```rust
    // Rhythm
    app.analytics.weekday_counts = queries::meetings_by_weekday(conn).unwrap_or_default();
    app.analytics.hourly_counts = queries::meetings_by_hour(conn).unwrap_or_default();
    app.analytics.meeting_size_trend = queries::avg_attendees_by_month(conn, 12).unwrap_or_default();

    // People
    app.analytics.recent_collaborators = queries::top_collaborators_recent(conn, 30, 10).unwrap_or_default();
    app.analytics.new_faces = queries::new_attendees_this_month(conn).unwrap_or_default();
    app.analytics.top_companies = queries::top_companies(conn, 10).unwrap_or_default();

    // Content
    app.analytics.label_trends = queries::label_trends(conn, 6).unwrap_or_default();
    app.analytics.busiest_weeks = queries::busiest_weeks(conn, 5).unwrap_or_default();

    // Superlatives
    app.analytics.superlatives = queries::superlatives(conn).ok();
```

**Step 2: Run full test suite**

Run: `cargo test`
Expected: All tests pass.

**Step 3: Commit**

```
git add src/tui/run.rs
git commit -m "feat: wire up all enhanced trends queries in load_analytics"
```
