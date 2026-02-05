// ABOUTME: TUI application state and data management
// ABOUTME: Holds document list, selection state, search query, and mode

use crate::db::queries::{DocumentRow, Stats};

/// Interaction mode for the TUI.
#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    Search,
}

/// Application state for the TUI dashboard.
pub struct App {
    pub documents: Vec<DocumentRow>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub search_query: String,
    pub mode: Mode,
    pub preview_content: Option<String>,
    pub stats: Option<Stats>,
    pub should_quit: bool,
}

impl App {
    pub fn new(documents: Vec<DocumentRow>, stats: Option<Stats>) -> Self {
        let filtered: Vec<usize> = (0..documents.len()).collect();
        App {
            documents,
            filtered,
            selected: 0,
            search_query: String::new(),
            mode: Mode::Normal,
            preview_content: None,
            stats,
            should_quit: false,
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

    #[test]
    fn test_new_empty() {
        let app = App::new(vec![], None);
        assert!(app.filtered.is_empty());
        assert_eq!(app.selected, 0);
        assert_eq!(app.mode, Mode::Normal);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_new_with_docs() {
        let docs = make_docs(&["Meeting A", "Meeting B"]);
        let app = App::new(docs, None);
        assert_eq!(app.filtered.len(), 2);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_select_next() {
        let docs = make_docs(&["A", "B", "C"]);
        let mut app = App::new(docs, None);
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
        let mut app = App::new(docs, None);
        assert_eq!(app.selected, 0);
        app.select_prev();
        assert_eq!(app.selected, 2); // wraps to end
        app.select_prev();
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn test_filter() {
        let docs = make_docs(&["Q4 Planning", "Weekly Standup", "Design Review"]);
        let mut app = App::new(docs, None);
        app.search_query = "planning".to_string();
        app.apply_filter();
        assert_eq!(app.filtered.len(), 1);
        assert_eq!(app.filtered[0], 0);
    }

    #[test]
    fn test_clear_filter() {
        let docs = make_docs(&["Q4 Planning", "Weekly Standup"]);
        let mut app = App::new(docs, None);
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
        let app = App::new(docs, None);
        let doc = app.selected_document().unwrap();
        assert_eq!(doc.title.as_deref(), Some("A"));
    }

    #[test]
    fn test_select_next_empty() {
        let mut app = App::new(vec![], None);
        app.select_next(); // should not panic
        app.select_prev(); // should not panic
        assert_eq!(app.selected, 0);
    }
}
