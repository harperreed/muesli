// ABOUTME: TUI application state and data management
// ABOUTME: Holds document list, selection state, search query, and mode

use crate::db::queries::{DocumentRow, Stats};

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

/// Application state for the TUI dashboard.
pub struct App {
    pub documents: Vec<DocumentRow>,
    pub all_documents: Vec<DocumentRow>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub search_query: String,
    pub mode: Mode,
    pub preview_content: Option<String>,
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
}
