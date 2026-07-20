// ABOUTME: Token discovery with precedence chain
// ABOUTME: CLI flag → env var → Granola session file (default)

use crate::{Error, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub fn resolve_token(
    cli_token: Option<String>,
    api_base: &str,
    data_dir: Option<&Path>,
) -> Result<String> {
    // 1. CLI flag (explicit override)
    if let Some(token) = cli_token {
        return Ok(token);
    }

    // 2. Environment variable (explicit override)
    if let Ok(token) = env::var("BEARER_TOKEN") {
        return Ok(token);
    }

    // 3. Refresh-token flow (current Granola builds). storage.dek was removed
    //    and the session DEK now lives in an entitlement-gated keychain we
    //    cannot read, so decrypting supabase.json.enc is impossible. Instead we
    //    exchange a stored/bootstrapped refresh token for a fresh access token.
    if let Some(token) = crate::refresh::try_refresh_token(api_base, data_dir)? {
        return Ok(token);
    }

    // 4. Legacy Granola session file (older builds still writing storage.dek)
    if let Some(token) = try_session_file()? {
        return Ok(token);
    }

    Err(Error::Auth(
        "No credentials found. Provide --token or BEARER_TOKEN, bootstrap a refresh token \
         (`muesli auth --set <TOKEN>`), or log in to Granola"
            .into(),
    ))
}

fn try_session_file() -> Result<Option<String>> {
    let home = env::var("HOME").map_err(|_| Error::Auth("HOME not set".into()))?;
    let path = PathBuf::from(home).join("Library/Application Support/Granola/supabase.json");

    parse_session_file(&path)
}

fn parse_session_file(path: &PathBuf) -> Result<Option<String>> {
    // Newer Granola builds write the session encrypted at supabase.json.enc
    // (Electron safeStorage wraps a DEK, which decrypts an AES-256-GCM blob).
    // The plaintext supabase.json, when also present, is a stale leftover
    // whose JWT is long revoked. Prefer the encrypted source of truth.
    let enc_path = {
        let mut os = path.as_os_str().to_owned();
        os.push(".enc");
        PathBuf::from(os)
    };

    let content = if enc_path.exists() {
        let session_dir = enc_path
            .parent()
            .ok_or_else(|| Error::Auth(format!("cannot find parent of {}", enc_path.display())))?;
        crate::session_decrypt::decrypt_session(session_dir)?
    } else if path.exists() {
        fs::read_to_string(path)?
    } else {
        return Ok(None);
    };

    extract_access_token(&content)
}

fn extract_access_token(content: &str) -> Result<Option<String>> {
    let json: serde_json::Value = serde_json::from_str(content)?;

    // workos_tokens is a stringified JSON blob inside the outer JSON.
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
        let token =
            resolve_token(Some("cli_token".into()), "https://api.granola.ai", None).unwrap();
        assert_eq!(token, "cli_token");
    }

    #[test]
    fn test_resolve_token_env() {
        env::set_var("BEARER_TOKEN", "env_token");
        let token = resolve_token(None, "https://api.granola.ai", None).unwrap();
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

    #[test]
    fn test_extract_access_token_happy_path() {
        let content = r#"{
            "workos_tokens": "{\"access_token\": \"jwt_value\", \"refresh\": \"x\"}"
        }"#;
        let token = extract_access_token(content).unwrap();
        assert_eq!(token, Some("jwt_value".into()));
    }

    #[test]
    fn test_extract_access_token_missing_workos() {
        let content = r#"{"session_id": "abc"}"#;
        let token = extract_access_token(content).unwrap();
        assert!(token.is_none());
    }

    #[test]
    fn test_extract_access_token_missing_access_token() {
        let content = r#"{"workos_tokens": "{\"refresh\": \"x\"}"}"#;
        let token = extract_access_token(content).unwrap();
        assert!(token.is_none());
    }
}
