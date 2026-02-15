// ABOUTME: Keyboard event handling for the TUI dashboard
// ABOUTME: Maps key events to app state transitions for Normal, Search, AttendeeFilter, and Analytics modes

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{AnalyticsTab, App, FocusedPane, Mode, TrendsGranularity, View};

/// Handle a key event and update app state accordingly.
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

fn handle_normal_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.should_quit = true;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        KeyCode::Tab => {
            app.toggle_focus();
        }
        KeyCode::Char('j') | KeyCode::Down => match app.focused_pane {
            FocusedPane::MeetingList => app.select_next(),
            FocusedPane::Preview => app.scroll_preview_down(),
        },
        KeyCode::Char('k') | KeyCode::Up => match app.focused_pane {
            FocusedPane::MeetingList => app.select_prev(),
            FocusedPane::Preview => app.scroll_preview_up(),
        },
        KeyCode::Char('/') => {
            app.mode = Mode::Search;
        }
        KeyCode::Char('@') => {
            app.attendee_query.clear();
            app.attendee_filtered = (0..app.attendees.len()).collect();
            app.attendee_selected = 0;
            app.mode = Mode::AttendeeFilter;
        }
        KeyCode::Char('C') => {
            // Uppercase C (Shift+C) clears active attendee filter
            if app.active_attendee_filter.is_some() {
                app.clear_attendee_filter();
            }
        }
        KeyCode::Char('a') => {
            app.toggle_view();
        }
        KeyCode::Enter => {
            // Open in $EDITOR handled by run.rs
        }
        _ => {}
    }
}

fn handle_search_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.clear_filter();
            app.mode = Mode::Normal;
        }
        KeyCode::Enter => {
            app.mode = Mode::Normal;
        }
        KeyCode::Backspace => {
            app.search_query.pop();
            app.apply_filter();
        }
        KeyCode::Char(c) => {
            app.search_query.push(c);
            app.apply_filter();
        }
        _ => {}
    }
}

fn handle_attendee_filter_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.attendee_query.clear();
            app.attendee_filtered = (0..app.attendees.len()).collect();
            app.attendee_selected = 0;
            app.mode = Mode::Normal;
        }
        KeyCode::Enter => {
            app.select_attendee();
        }
        KeyCode::Backspace => {
            app.attendee_query.pop();
            app.apply_attendee_filter();
        }
        KeyCode::Down | KeyCode::Tab => {
            app.attendee_select_next();
        }
        KeyCode::Up | KeyCode::BackTab => {
            app.attendee_select_prev();
        }
        KeyCode::Char(c) => {
            // j/k for navigation only when query is empty (vim-style)
            if c == 'j' && app.attendee_query.is_empty() {
                app.attendee_select_next();
            } else if c == 'k' && app.attendee_query.is_empty() {
                app.attendee_select_prev();
            } else {
                app.attendee_query.push(c);
                app.apply_attendee_filter();
            }
        }
        _ => {}
    }
}

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
        KeyCode::Char('d') if app.analytics.tab == AnalyticsTab::Trends => {
            app.analytics.granularity = TrendsGranularity::Daily;
        }
        KeyCode::Char('w') if app.analytics.tab == AnalyticsTab::Trends => {
            app.analytics.granularity = TrendsGranularity::Weekly;
        }
        KeyCode::Char('m') if app.analytics.tab == AnalyticsTab::Trends => {
            app.analytics.granularity = TrendsGranularity::Monthly;
        }
        KeyCode::Char('r') => {
            app.analytics.loaded = false;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::queries::DocumentRow;
    use crate::tui::app::{AnalyticsTab, TrendsGranularity, View};
    use chrono::Utc;

    fn make_app() -> App {
        let docs = vec![
            DocumentRow {
                doc_id: "doc1".to_string(),
                title: Some("Planning".to_string()),
                created_at: Utc::now(),
                duration_seconds: Some(3600),
                filename: Some("file1".to_string()),
            },
            DocumentRow {
                doc_id: "doc2".to_string(),
                title: Some("Standup".to_string()),
                created_at: Utc::now(),
                duration_seconds: Some(900),
                filename: Some("file2".to_string()),
            },
        ];
        let attendees = vec![
            "Alice".to_string(),
            "Bob".to_string(),
            "Charlie".to_string(),
        ];
        App::new(docs, None, attendees)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    #[test]
    fn test_quit_on_q() {
        let mut app = make_app();
        handle_key_event(&mut app, key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn test_quit_on_esc() {
        let mut app = make_app();
        handle_key_event(&mut app, key(KeyCode::Esc));
        assert!(app.should_quit);
    }

    #[test]
    fn test_nav_j_k() {
        let mut app = make_app();
        assert_eq!(app.selected, 0);
        handle_key_event(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.selected, 1);
        handle_key_event(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_enter_search_mode() {
        let mut app = make_app();
        handle_key_event(&mut app, key(KeyCode::Char('/')));
        assert_eq!(app.mode, Mode::Search);
    }

    #[test]
    fn test_search_typing() {
        let mut app = make_app();
        app.mode = Mode::Search;

        handle_key_event(&mut app, key(KeyCode::Char('p')));
        handle_key_event(&mut app, key(KeyCode::Char('l')));
        assert_eq!(app.search_query, "pl");
        assert_eq!(app.filtered.len(), 1); // "Planning"

        handle_key_event(&mut app, key(KeyCode::Backspace));
        handle_key_event(&mut app, key(KeyCode::Backspace));
        assert_eq!(app.search_query, "");
        assert_eq!(app.filtered.len(), 2);
    }

    #[test]
    fn test_search_esc_clears() {
        let mut app = make_app();
        app.mode = Mode::Search;
        handle_key_event(&mut app, key(KeyCode::Char('x')));
        assert_eq!(app.filtered.len(), 0);

        handle_key_event(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.filtered.len(), 2); // cleared
    }

    #[test]
    fn test_search_enter_confirms() {
        let mut app = make_app();
        app.mode = Mode::Search;
        handle_key_event(&mut app, key(KeyCode::Char('s')));
        handle_key_event(&mut app, key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.search_query, "s"); // filter preserved
        assert_eq!(app.filtered.len(), 1); // "Standup"
    }

    #[test]
    fn test_at_enters_attendee_filter_mode() {
        let mut app = make_app();
        handle_key_event(&mut app, key(KeyCode::Char('@')));
        assert_eq!(app.mode, Mode::AttendeeFilter);
        assert!(app.attendee_query.is_empty());
        assert_eq!(app.attendee_filtered.len(), 3);
    }

    #[test]
    fn test_attendee_filter_typing_filters() {
        let mut app = make_app();
        app.mode = Mode::AttendeeFilter;

        handle_key_event(&mut app, key(KeyCode::Char('a')));
        handle_key_event(&mut app, key(KeyCode::Char('l')));
        assert_eq!(app.attendee_query, "al");
        assert_eq!(app.attendee_filtered.len(), 1); // "Alice"
    }

    #[test]
    fn test_attendee_filter_enter_applies() {
        let mut app = make_app();
        app.mode = Mode::AttendeeFilter;

        // Select Bob (navigate down once)
        handle_key_event(&mut app, key(KeyCode::Down));
        assert_eq!(app.attendee_selected, 1);

        handle_key_event(&mut app, key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.active_attendee_filter, Some("Bob".to_string()));
        assert!(app.attendee_filter_changed);
    }

    #[test]
    fn test_attendee_filter_esc_cancels() {
        let mut app = make_app();
        app.mode = Mode::AttendeeFilter;

        handle_key_event(&mut app, key(KeyCode::Char('b')));
        assert_eq!(app.attendee_query, "b");

        handle_key_event(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.attendee_query.is_empty());
        assert!(app.active_attendee_filter.is_none());
    }

    #[test]
    fn test_attendee_filter_backspace() {
        let mut app = make_app();
        app.mode = Mode::AttendeeFilter;

        handle_key_event(&mut app, key(KeyCode::Char('x')));
        assert_eq!(app.attendee_filtered.len(), 0);

        handle_key_event(&mut app, key(KeyCode::Backspace));
        assert_eq!(app.attendee_query, "");
        assert_eq!(app.attendee_filtered.len(), 3);
    }

    #[test]
    fn test_attendee_filter_nav_arrows() {
        let mut app = make_app();
        app.mode = Mode::AttendeeFilter;

        handle_key_event(&mut app, key(KeyCode::Down));
        assert_eq!(app.attendee_selected, 1);
        handle_key_event(&mut app, key(KeyCode::Up));
        assert_eq!(app.attendee_selected, 0);
    }

    #[test]
    fn test_tab_toggles_focus() {
        let mut app = make_app();
        assert_eq!(app.focused_pane, FocusedPane::MeetingList);

        handle_key_event(&mut app, key(KeyCode::Tab));
        assert_eq!(app.focused_pane, FocusedPane::Preview);

        handle_key_event(&mut app, key(KeyCode::Tab));
        assert_eq!(app.focused_pane, FocusedPane::MeetingList);
    }

    #[test]
    fn test_j_k_scrolls_preview_when_focused() {
        let mut app = make_app();
        app.focused_pane = FocusedPane::Preview;

        handle_key_event(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.preview_scroll, 1);

        handle_key_event(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.preview_scroll, 2);

        handle_key_event(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.preview_scroll, 1);
    }

    #[test]
    fn test_shift_c_clears_attendee_filter() {
        let mut app = make_app();

        // Simulate an active attendee filter (as if Bob was selected via @)
        app.active_attendee_filter = Some("Bob".to_string());
        app.documents = vec![app.documents[0].clone()]; // reduced doc list
        app.filtered = vec![0];

        // Press uppercase C (Shift+C) in Normal mode
        handle_key_event(&mut app, key(KeyCode::Char('C')));

        // Filter should be cleared and full document list restored
        assert!(app.active_attendee_filter.is_none());
        assert_eq!(app.documents.len(), 2); // restored from all_documents
        assert_eq!(app.filtered.len(), 2);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn test_shift_c_noop_without_active_filter() {
        let mut app = make_app();
        assert!(app.active_attendee_filter.is_none());

        // Press uppercase C when no filter is active - should be a no-op
        handle_key_event(&mut app, key(KeyCode::Char('C')));
        assert!(app.active_attendee_filter.is_none());
        assert_eq!(app.documents.len(), 2);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_j_k_navigates_list_when_list_focused() {
        let mut app = make_app();
        assert_eq!(app.focused_pane, FocusedPane::MeetingList);
        assert_eq!(app.selected, 0);

        handle_key_event(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.selected, 1);
        assert_eq!(app.preview_scroll, 0); // scroll should not change
    }

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
    fn test_granularity_keys_ignored_on_dashboard() {
        let mut app = make_app();
        app.view = View::Analytics;
        app.analytics.tab = AnalyticsTab::Dashboard;
        app.analytics.granularity = TrendsGranularity::Weekly;
        handle_key_event(&mut app, key(KeyCode::Char('d')));
        assert_eq!(app.analytics.granularity, TrendsGranularity::Weekly);
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
}
