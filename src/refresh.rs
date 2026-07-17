// ABOUTME: Granola refresh-token auth flow for post-storage.dek Granola builds
// ABOUTME: Exchanges a stored (rotating) refresh token for a fresh access token

// Recent Granola desktop builds migrate the session DEK out of the on-disk
// `storage.dek` file into an entitlement-gated macOS keychain item
// (`com.granola.app.dek`, access group `QZ7DHHLN25.granola`) and delete the
// file. That keychain item is only readable by Granola-signed code, so muesli
// can no longer decrypt `supabase.json.enc` to recover the session token.
//
// Instead we authenticate the way the app itself refreshes: POST a refresh
// token to `/v1/refresh-access-token` and use the returned access token as the
// Bearer credential. Granola rotates the refresh token on each call (and does
// not immediately revoke the prior one), so we persist the rotated token to
// `<data_dir>/auth.json` for the next run.

use crate::{Error, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Client version advertised to the Granola API. Kept in sync with a recent
/// desktop build so the refresh endpoint accepts our request.
const CLIENT_VERSION: &str = "7.427.3";

#[derive(Serialize, Deserialize, Default)]
struct AuthStore {
    refresh_token: String,
}

#[derive(Deserialize)]
struct RefreshResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
}

/// Attempts the refresh-token flow.
///
/// Returns `Ok(Some(access_token))` on success, `Ok(None)` if no refresh token
/// is available anywhere (so the caller can fall through to other auth methods),
/// or `Err` if a refresh token exists but the exchange fails.
pub fn try_refresh_token(api_base: &str, data_dir: Option<&Path>) -> Result<Option<String>> {
    let auth_path = auth_file_path(data_dir)?;

    // A token we persisted ourselves is authoritative: if its exchange fails,
    // surface the error. A token merely bootstrapped from Granola's leftover
    // plaintext session is best-effort: if it fails, return Ok(None) so the
    // caller can fall through to the legacy session-file path (older Granola
    // builds whose storage.dek still decrypts).
    let (refresh_token, persisted) = match load_refresh_token(&auth_path)? {
        Some(t) => (t, true),
        None => match bootstrap_from_granola()? {
            Some(t) => (t, false),
            None => return Ok(None),
        },
    };

    let resp = match exchange(api_base, &refresh_token) {
        Ok(resp) => resp,
        Err(e) if !persisted => {
            eprintln!("muesli: bootstrap refresh token rejected ({e}); trying other auth methods");
            return Ok(None);
        }
        Err(e) => return Err(e),
    };

    // The exchange succeeded. Persist the (rotated) refresh token so we own the
    // rotation chain going forward, even when we bootstrapped this run.
    let store = resp
        .refresh_token
        .as_deref()
        .filter(|t| !t.is_empty())
        .unwrap_or(&refresh_token);
    save_refresh_token(&auth_path, store)?;

    Ok(Some(resp.access_token))
}

/// Stores a refresh token provided out-of-band (e.g. `muesli auth --set`).
pub fn set_refresh_token(data_dir: Option<&Path>, token: &str) -> Result<()> {
    let auth_path = auth_file_path(data_dir)?;
    save_refresh_token(&auth_path, token)
}

/// Reports whether a refresh token is currently stored, and its source.
pub fn status(data_dir: Option<&Path>) -> Result<String> {
    let auth_path = auth_file_path(data_dir)?;
    if load_refresh_token(&auth_path)?.is_some() {
        return Ok(format!("refresh token stored at {}", auth_path.display()));
    }
    match bootstrap_from_granola()? {
        Some(_) => Ok(
            "no stored token yet; can bootstrap from Granola's supabase.json on next sync".into(),
        ),
        None => Ok("no refresh token available; run `muesli auth --set <TOKEN>`".into()),
    }
}

fn exchange(api_base: &str, refresh_token: &str) -> Result<RefreshResponse> {
    let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
    let url = format!("{}/v1/refresh-access-token", api_base.trim_end_matches('/'));

    let response = client
        .post(&url)
        .header("Accept", "*/*")
        .header("Content-Type", "application/json")
        .header("User-Agent", format!("Granola/{CLIENT_VERSION}"))
        .header("X-Client-Version", CLIENT_VERSION)
        .header("X-Granola-Platform", "darwin")
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .send()?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(Error::Auth(format!(
            "refresh-access-token returned {}. The stored refresh token may be revoked; \
             re-bootstrap by logging into Granola or `muesli auth --set <TOKEN>`. Response: {}",
            status.as_u16(),
            truncate(&body, 200)
        )));
    }

    let text = response.text()?;
    serde_json::from_str(&text).map_err(Error::from)
}

fn auth_file_path(data_dir: Option<&Path>) -> Result<PathBuf> {
    let dir = match data_dir {
        Some(d) => d.to_path_buf(),
        None => default_data_dir()?,
    };
    Ok(dir.join("auth.json"))
}

fn default_data_dir() -> Result<PathBuf> {
    if let Ok(xdg) = env::var("XDG_DATA_HOME") {
        return Ok(PathBuf::from(xdg).join("muesli"));
    }
    let home = env::var("HOME").map_err(|_| Error::Auth("HOME not set".into()))?;
    Ok(PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("muesli"))
}

fn load_refresh_token(auth_path: &Path) -> Result<Option<String>> {
    if !auth_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(auth_path)?;
    let store: AuthStore = serde_json::from_str(&content)?;
    if store.refresh_token.is_empty() {
        Ok(None)
    } else {
        Ok(Some(store.refresh_token))
    }
}

fn save_refresh_token(auth_path: &Path, token: &str) -> Result<()> {
    if let Some(parent) = auth_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let store = AuthStore {
        refresh_token: token.to_string(),
    };
    let json = serde_json::to_string(&store)?;
    fs::write(auth_path, json)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(auth_path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Reads a refresh token from Granola's leftover plaintext `supabase.json`.
///
/// Newer Granola builds keep the live session in `supabase.json.enc` (which we
/// cannot decrypt), but the pre-migration plaintext `supabase.json` is usually
/// still on disk, and its refresh token remains valid because Granola does not
/// revoke rotated tokens. This is a one-time bootstrap; after the first
/// exchange we own the rotation chain via `auth.json`.
fn bootstrap_from_granola() -> Result<Option<String>> {
    let home = match env::var("HOME") {
        Ok(h) => h,
        Err(_) => return Ok(None),
    };
    let path = PathBuf::from(home).join("Library/Application Support/Granola/supabase.json");
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)?;
    Ok(extract_refresh_token(&content))
}

/// Extracts `workos_tokens.refresh_token` from a Granola session JSON blob.
fn extract_refresh_token(content: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(content).ok()?;
    let workos_str = json.get("workos_tokens")?.as_str()?;
    let workos: serde_json::Value = serde_json::from_str(workos_str).ok()?;
    let rt = workos.get("refresh_token")?.as_str()?;
    if rt.is_empty() {
        None
    } else {
        Some(rt.to_string())
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut boundary = max;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    format!("{}...", &s[..boundary])
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn extract_refresh_token_happy_path() {
        let content =
            r#"{ "workos_tokens": "{\"access_token\": \"a\", \"refresh_token\": \"rt_123\"}" }"#;
        assert_eq!(extract_refresh_token(content), Some("rt_123".into()));
    }

    #[test]
    fn extract_refresh_token_missing() {
        assert_eq!(extract_refresh_token(r#"{"session_id":"x"}"#), None);
        assert_eq!(
            extract_refresh_token(r#"{"workos_tokens":"{\"access_token\":\"a\"}"}"#),
            None
        );
    }

    #[test]
    fn save_then_load_round_trips() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("auth.json");
        assert_eq!(load_refresh_token(&path).unwrap(), None);
        save_refresh_token(&path, "rt_abc").unwrap();
        assert_eq!(load_refresh_token(&path).unwrap(), Some("rt_abc".into()));
    }

    #[test]
    fn save_sets_owner_only_permissions() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("auth.json");
        save_refresh_token(&path, "rt_abc").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn empty_token_reads_as_none() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("auth.json");
        fs::write(&path, r#"{"refresh_token":""}"#).unwrap();
        assert_eq!(load_refresh_token(&path).unwrap(), None);
    }
}
