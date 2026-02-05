// ABOUTME: Keyboard event handling for the TUI dashboard
// ABOUTME: Maps key events to app state transitions for Normal and Search modes

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{App, Mode};

/// Handle a key event and update app state accordingly.
pub fn handle_key_event(app: &mut App, key: KeyEvent) {
    match app.mode {
        Mode::Normal => handle_normal_mode(app, key),
        Mode::Search => handle_search_mode(app, key),
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
        KeyCode::Char('j') | KeyCode::Down => {
            app.select_next();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.select_prev();
        }
        KeyCode::Char('/') => {
            app.mode = Mode::Search;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::queries::DocumentRow;
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
        App::new(docs, None)
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
}
