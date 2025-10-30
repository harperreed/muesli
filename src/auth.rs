// ABOUTME: Token discovery with precedence chain
// ABOUTME: CLI flag → macOS session → XDG session → env var

use crate::{Error, Result};
use std::env;
use std::fs;
use std::path::PathBuf;

pub fn resolve_token(cli_token: Option<String>) -> Result<String> {
    // 1. CLI flag
    if let Some(token) = cli_token {
        return Ok(token);
    }

    // 2. macOS session file
    if let Some(token) = try_macos_session()? {
        return Ok(token);
    }

    // 3. XDG session file
    if let Some(token) = try_xdg_session()? {
        return Ok(token);
    }

    // 4. Environment variable
    if let Ok(token) = env::var("BEARER_TOKEN") {
        return Ok(token);
    }

    Err(Error::Auth("No bearer token found. Provide via --token, session file, or BEARER_TOKEN env var".into()))
}

fn try_macos_session() -> Result<Option<String>> {
    let home = env::var("HOME").map_err(|_| Error::Auth("HOME not set".into()))?;
    let path = PathBuf::from(home)
        .join("Library/Application Support/Granola/supabase.json");

    parse_session_file(&path)
}

fn try_xdg_session() -> Result<Option<String>> {
    let config_home = env::var("XDG_CONFIG_HOME")
        .unwrap_or_else(|_| {
            let home = env::var("HOME").unwrap_or_default();
            format!("{}/.config", home)
        });

    let path = PathBuf::from(config_home).join("granola/supabase.json");
    parse_session_file(&path)
}

fn parse_session_file(path: &PathBuf) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    let json: serde_json::Value = serde_json::from_str(&content)?;

    // Parse workos_tokens (which is a stringified JSON)
    if let Some(workos_str) = json.get("workos_tokens").and_then(|v| v.as_str()) {
        let workos: serde_json::Value = serde_json::from_str(workos_str)?;
        if let Some(access_token) = workos.get("access_token").and_then(|v| v.as_str()) {
            return Ok(Some(access_token.to_string()));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_token_cli_precedence() {
        let token = resolve_token(Some("cli_token".into())).unwrap();
        assert_eq!(token, "cli_token");
    }

    #[test]
    fn test_resolve_token_env() {
        env::set_var("BEARER_TOKEN", "env_token");
        let token = resolve_token(None).unwrap();
        assert_eq!(token, "env_token");
        env::remove_var("BEARER_TOKEN");
    }

    #[test]
    fn test_parse_session_file_valid() {
        let temp = TempDir::new().unwrap();
        let session_path = temp.path().join("supabase.json");

        let content = r#"{
            "workos_tokens": "{\"access_token\": \"test_token_123\"}"
        }"#;
        fs::write(&session_path, content).unwrap();

        let token = parse_session_file(&session_path).unwrap();
        assert_eq!(token, Some("test_token_123".into()));
    }

    #[test]
    fn test_parse_session_file_missing() {
        let temp = TempDir::new().unwrap();
        let session_path = temp.path().join("missing.json");

        let token = parse_session_file(&session_path).unwrap();
        assert!(token.is_none());
    }
}
