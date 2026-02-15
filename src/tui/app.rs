// ABOUTME: TUI application state and data management
// ABOUTME: Holds document list, selection state, search query, and mode

use std::borrow::Cow;

use ratatui::text::{Line, Span, Text};

use crate::db::queries::{
    AttendeeFrequency, CompanyFrequency, DailyCount, DayOfWeekCount, DocumentRow, HourOfDayCount,
    LabelByMonth, LabelFrequency, MeetingSizeByMonth, MonthlyCount, RecentCollaborator, Stats,
    Superlatives, WeeklyCount,
};

/// Interaction mode for the TUI.
#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    Search,
    AttendeeFilter,
}

/// Which pane currently has keyboard focus.
#[derive(Debug, Clone, PartialEq)]
pub enum FocusedPane {
    MeetingList,
    Preview,
}

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
            weekday_counts: Vec::new(),
            hourly_counts: Vec::new(),
            meeting_size_trend: Vec::new(),
            recent_collaborators: Vec::new(),
            new_faces: Vec::new(),
            top_companies: Vec::new(),
            label_trends: Vec::new(),
            busiest_weeks: Vec::new(),
            superlatives: None,
        }
    }
}

/// Application state for the TUI dashboard.
pub struct App {
    pub documents: Vec<DocumentRow>,
    pub all_documents: Vec<DocumentRow>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub search_query: String,
    pub mode: Mode,
    pub preview_content: Option<String>,
    /// Cached parsed markdown from `preview_content`, to avoid re-parsing every frame.
    pub preview_parsed: Option<ratatui::text::Text<'static>>,
    pub stats: Option<Stats>,
    pub should_quit: bool,
    pub attendees: Vec<String>,
    pub attendee_query: String,
    pub attendee_filtered: Vec<usize>,
    pub attendee_selected: usize,
    pub active_attendee_filter: Option<String>,
    /// Flag set when attendee filter was just applied, so run loop can re-query DB.
    pub attendee_filter_changed: bool,
    /// Which pane has keyboard focus (Tab switches between them).
    pub focused_pane: FocusedPane,
    /// Vertical scroll offset for the preview pane.
    pub preview_scroll: u16,
    /// Current top-level view (Meetings or Analytics).
    pub view: View,
    /// Cached state for the analytics view.
    pub analytics: AnalyticsState,
}

/// Convert a `Text<'_>` to `Text<'static>` by making all `Cow` string data owned.
fn text_to_static(text: Text<'_>) -> Text<'static> {
    Text {
        alignment: text.alignment,
        style: text.style,
        lines: text
            .lines
            .into_iter()
            .map(|line| Line {
                style: line.style,
                alignment: line.alignment,
                spans: line
                    .spans
                    .into_iter()
                    .map(|span| Span {
                        style: span.style,
                        content: Cow::Owned(span.content.into_owned()),
                    })
                    .collect(),
            })
            .collect(),
    }
}

impl App {
    pub fn new(documents: Vec<DocumentRow>, stats: Option<Stats>, attendees: Vec<String>) -> Self {
        let filtered: Vec<usize> = (0..documents.len()).collect();
        let attendee_filtered: Vec<usize> = (0..attendees.len()).collect();
        let all_documents = documents.clone();
        App {
            documents,
            all_documents,
            filtered,
            selected: 0,
            search_query: String::new(),
            mode: Mode::Normal,
            preview_content: None,
            preview_parsed: None,
            stats,
            should_quit: false,
            attendees,
            attendee_query: String::new(),
            attendee_filtered,
            attendee_selected: 0,
            active_attendee_filter: None,
            attendee_filter_changed: false,
            focused_pane: FocusedPane::MeetingList,
            preview_scroll: 0,
            view: View::Meetings,
            analytics: AnalyticsState::default(),
        }
    }

    /// Move selection to the next item in the filtered list.
    pub fn select_next(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
        }
    }

    /// Move selection to the previous item in the filtered list.
    pub fn select_prev(&mut self) {
        if !self.filtered.is_empty() {
            if self.selected == 0 {
                self.selected = self.filtered.len() - 1;
            } else {
                self.selected -= 1;
            }
        }
    }

    /// Apply the search filter against document titles.
    pub fn apply_filter(&mut self) {
        let query = self.search_query.to_lowercase();
        if query.is_empty() {
            self.filtered = (0..self.documents.len()).collect();
        } else {
            self.filtered = self
                .documents
                .iter()
                .enumerate()
                .filter(|(_, doc)| {
                    doc.title
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&query)
                })
                .map(|(i, _)| i)
                .collect();
        }
        // Reset selection to stay in bounds
        if self.selected >= self.filtered.len() {
            self.selected = 0;
        }
    }

    /// Clear the search filter and show all documents.
    pub fn clear_filter(&mut self) {
        self.search_query.clear();
        self.filtered = (0..self.documents.len()).collect();
        self.selected = 0;
    }

    /// Get the currently selected document, if any.
    pub fn selected_document(&self) -> Option<&DocumentRow> {
        self.filtered
            .get(self.selected)
            .and_then(|&idx| self.documents.get(idx))
    }

    /// Filter the attendee list by the current attendee_query.
    pub fn apply_attendee_filter(&mut self) {
        let query = self.attendee_query.to_lowercase();
        if query.is_empty() {
            self.attendee_filtered = (0..self.attendees.len()).collect();
        } else {
            self.attendee_filtered = self
                .attendees
                .iter()
                .enumerate()
                .filter(|(_, name)| name.to_lowercase().contains(&query))
                .map(|(i, _)| i)
                .collect();
        }
        if self.attendee_selected >= self.attendee_filtered.len() {
            self.attendee_selected = 0;
        }
    }

    /// Apply the selected attendee as the document filter.
    /// Sets the flag so run.rs can re-query the DB.
    pub fn select_attendee(&mut self) {
        if let Some(&idx) = self.attendee_filtered.get(self.attendee_selected) {
            let name = self.attendees[idx].clone();
            self.active_attendee_filter = Some(name);
            self.attendee_filter_changed = true;
        }
        self.attendee_query.clear();
        self.attendee_filtered = (0..self.attendees.len()).collect();
        self.attendee_selected = 0;
        self.mode = Mode::Normal;
    }

    /// Clear the attendee filter and restore the full document list.
    pub fn clear_attendee_filter(&mut self) {
        self.active_attendee_filter = None;
        self.documents = self.all_documents.clone();
        self.search_query.clear();
        self.filtered = (0..self.documents.len()).collect();
        self.selected = 0;
        self.attendee_filter_changed = false;
    }

    /// Move selection to the next attendee in the filtered list.
    pub fn attendee_select_next(&mut self) {
        if !self.attendee_filtered.is_empty() {
            self.attendee_selected = (self.attendee_selected + 1) % self.attendee_filtered.len();
        }
    }

    /// Toggle keyboard focus between meeting list and preview pane.
    pub fn toggle_focus(&mut self) {
        self.focused_pane = match self.focused_pane {
            FocusedPane::MeetingList => FocusedPane::Preview,
            FocusedPane::Preview => FocusedPane::MeetingList,
        };
    }

    /// Scroll the preview pane down by one line.
    pub fn scroll_preview_down(&mut self) {
        self.preview_scroll = self.preview_scroll.saturating_add(1);
    }

    /// Scroll the preview pane up by one line.
    pub fn scroll_preview_up(&mut self) {
        self.preview_scroll = self.preview_scroll.saturating_sub(1);
    }

    /// Reset preview scroll to the top (called when selection changes).
    pub fn reset_preview_scroll(&mut self) {
        self.preview_scroll = 0;
    }

    /// Parse `preview_content` into styled `Text` and cache it in `preview_parsed`.
    /// Call this after setting `preview_content` to avoid re-parsing on every frame draw.
    pub fn parse_preview(&mut self) {
        self.preview_parsed = self
            .preview_content
            .as_deref()
            .map(|s| text_to_static(tui_markdown::from_str(s)));
    }

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

    /// Move selection to the previous attendee in the filtered list.
    pub fn attendee_select_prev(&mut self) {
        if !self.attendee_filtered.is_empty() {
            if self.attendee_selected == 0 {
                self.attendee_selected = self.attendee_filtered.len() - 1;
            } else {
                self.attendee_selected -= 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_docs(titles: &[&str]) -> Vec<DocumentRow> {
        titles
            .iter()
            .enumerate()
            .map(|(i, t)| DocumentRow {
                doc_id: format!("doc{}", i),
                title: Some(t.to_string()),
                created_at: Utc::now(),
                duration_seconds: Some(3600),
                filename: Some(format!("file{}", i)),
            })
            .collect()
    }

    fn make_attendees() -> Vec<String> {
        vec![
            "Alice".to_string(),
            "Bob".to_string(),
            "Charlie".to_string(),
        ]
    }

    #[test]
    fn test_new_empty() {
        let app = App::new(vec![], None, vec![]);
        assert!(app.filtered.is_empty());
        assert_eq!(app.selected, 0);
        assert_eq!(app.mode, Mode::Normal);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_new_with_docs() {
        let docs = make_docs(&["Meeting A", "Meeting B"]);
        let app = App::new(docs, None, vec![]);
        assert_eq!(app.filtered.len(), 2);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_new_preserves_all_documents() {
        let docs = make_docs(&["A", "B"]);
        let app = App::new(docs, None, vec![]);
        assert_eq!(app.all_documents.len(), 2);
        assert_eq!(app.documents.len(), 2);
    }

    #[test]
    fn test_select_next() {
        let docs = make_docs(&["A", "B", "C"]);
        let mut app = App::new(docs, None, vec![]);
        assert_eq!(app.selected, 0);
        app.select_next();
        assert_eq!(app.selected, 1);
        app.select_next();
        assert_eq!(app.selected, 2);
        app.select_next();
        assert_eq!(app.selected, 0); // wraps
    }

    #[test]
    fn test_select_prev() {
        let docs = make_docs(&["A", "B", "C"]);
        let mut app = App::new(docs, None, vec![]);
        assert_eq!(app.selected, 0);
        app.select_prev();
        assert_eq!(app.selected, 2); // wraps to end
        app.select_prev();
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn test_filter() {
        let docs = make_docs(&["Q4 Planning", "Weekly Standup", "Design Review"]);
        let mut app = App::new(docs, None, vec![]);
        app.search_query = "planning".to_string();
        app.apply_filter();
        assert_eq!(app.filtered.len(), 1);
        assert_eq!(app.filtered[0], 0);
    }

    #[test]
    fn test_clear_filter() {
        let docs = make_docs(&["Q4 Planning", "Weekly Standup"]);
        let mut app = App::new(docs, None, vec![]);
        app.search_query = "planning".to_string();
        app.apply_filter();
        assert_eq!(app.filtered.len(), 1);

        app.clear_filter();
        assert_eq!(app.filtered.len(), 2);
        assert!(app.search_query.is_empty());
    }

    #[test]
    fn test_selected_document() {
        let docs = make_docs(&["A", "B"]);
        let app = App::new(docs, None, vec![]);
        let doc = app.selected_document().unwrap();
        assert_eq!(doc.title.as_deref(), Some("A"));
    }

    #[test]
    fn test_select_next_empty() {
        let mut app = App::new(vec![], None, vec![]);
        app.select_next(); // should not panic
        app.select_prev(); // should not panic
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_attendee_filter_narrows_list() {
        let docs = make_docs(&["A"]);
        let attendees = make_attendees();
        let mut app = App::new(docs, None, attendees);
        assert_eq!(app.attendee_filtered.len(), 3);

        app.attendee_query = "ali".to_string();
        app.apply_attendee_filter();
        assert_eq!(app.attendee_filtered.len(), 1);
        assert_eq!(app.attendees[app.attendee_filtered[0]], "Alice");
    }

    #[test]
    fn test_attendee_filter_case_insensitive() {
        let docs = make_docs(&["A"]);
        let attendees = make_attendees();
        let mut app = App::new(docs, None, attendees);

        app.attendee_query = "BOB".to_string();
        app.apply_attendee_filter();
        assert_eq!(app.attendee_filtered.len(), 1);
        assert_eq!(app.attendees[app.attendee_filtered[0]], "Bob");
    }

    #[test]
    fn test_select_attendee_sets_filter() {
        let docs = make_docs(&["A", "B"]);
        let attendees = make_attendees();
        let mut app = App::new(docs, None, attendees);

        // Select "Bob" (index 1)
        app.attendee_selected = 1;
        app.select_attendee();
        assert_eq!(app.active_attendee_filter, Some("Bob".to_string()));
        assert!(app.attendee_filter_changed);
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.attendee_query.is_empty());
    }

    #[test]
    fn test_clear_attendee_filter_restores_docs() {
        let docs = make_docs(&["A", "B", "C"]);
        let attendees = make_attendees();
        let mut app = App::new(docs, None, attendees);
        assert_eq!(app.all_documents.len(), 3);

        // Simulate an attendee filter reducing documents
        app.documents = make_docs(&["A"]);
        app.filtered = vec![0];
        app.active_attendee_filter = Some("Alice".to_string());

        app.clear_attendee_filter();
        assert_eq!(app.documents.len(), 3);
        assert_eq!(app.filtered.len(), 3);
        assert!(app.active_attendee_filter.is_none());
        assert!(app.search_query.is_empty());
    }

    #[test]
    fn test_attendee_select_next_prev() {
        let docs = make_docs(&["A"]);
        let attendees = make_attendees();
        let mut app = App::new(docs, None, attendees);

        assert_eq!(app.attendee_selected, 0);
        app.attendee_select_next();
        assert_eq!(app.attendee_selected, 1);
        app.attendee_select_next();
        assert_eq!(app.attendee_selected, 2);
        app.attendee_select_next();
        assert_eq!(app.attendee_selected, 0); // wraps

        app.attendee_select_prev();
        assert_eq!(app.attendee_selected, 2); // wraps to end
    }

    #[test]
    fn test_attendee_select_empty() {
        let docs = make_docs(&["A"]);
        let mut app = App::new(docs, None, vec![]);
        app.attendee_select_next(); // should not panic
        app.attendee_select_prev(); // should not panic
        assert_eq!(app.attendee_selected, 0);
    }

    #[test]
    fn test_default_focus_is_meeting_list() {
        let app = App::new(vec![], None, vec![]);
        assert_eq!(app.focused_pane, FocusedPane::MeetingList);
    }

    #[test]
    fn test_toggle_focus() {
        let mut app = App::new(vec![], None, vec![]);
        assert_eq!(app.focused_pane, FocusedPane::MeetingList);

        app.toggle_focus();
        assert_eq!(app.focused_pane, FocusedPane::Preview);

        app.toggle_focus();
        assert_eq!(app.focused_pane, FocusedPane::MeetingList);
    }

    #[test]
    fn test_preview_scroll() {
        let mut app = App::new(vec![], None, vec![]);
        assert_eq!(app.preview_scroll, 0);

        app.scroll_preview_down();
        app.scroll_preview_down();
        assert_eq!(app.preview_scroll, 2);

        app.scroll_preview_up();
        assert_eq!(app.preview_scroll, 1);

        // Should not underflow
        app.scroll_preview_up();
        app.scroll_preview_up();
        assert_eq!(app.preview_scroll, 0);
    }

    #[test]
    fn test_reset_preview_scroll() {
        let mut app = App::new(vec![], None, vec![]);
        app.preview_scroll = 42;
        app.reset_preview_scroll();
        assert_eq!(app.preview_scroll, 0);
    }

    #[test]
    fn test_parse_preview_caches_parsed_text() {
        let mut app = App::new(vec![], None, vec![]);
        assert!(app.preview_parsed.is_none());

        app.preview_content = Some("# Hello\n\nSome **bold** text".to_string());
        app.parse_preview();
        assert!(app.preview_parsed.is_some());

        let parsed = app.preview_parsed.as_ref().unwrap();
        assert!(!parsed.lines.is_empty(), "parsed text should have lines");
    }

    #[test]
    fn test_parse_preview_none_when_no_content() {
        let mut app = App::new(vec![], None, vec![]);
        app.preview_content = None;
        app.parse_preview();
        assert!(app.preview_parsed.is_none());
    }

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

    #[test]
    fn test_analytics_tab_switch_resets_scroll() {
        let mut app = App::new(vec![], None, vec![]);
        app.analytics.scroll = 5;
        app.analytics_next_tab();
        assert_eq!(app.analytics.scroll, 0);
    }

    #[test]
    fn test_text_to_static_preserves_content() {
        let original = tui_markdown::from_str("# Test\n\n- item one\n- item two");
        let original_line_count = original.lines.len();
        let static_text = super::text_to_static(original);
        assert_eq!(
            static_text.lines.len(),
            original_line_count,
            "static text should have the same number of lines"
        );
    }
}
