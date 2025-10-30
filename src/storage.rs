// ABOUTME: XDG-compliant storage layer with atomic writes
// ABOUTME: Handles paths, permissions, and frontmatter parsing

use crate::{Error, Frontmatter, Result};
use directories::ProjectDirs;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Paths {
    pub data_dir: PathBuf,
    pub raw_dir: PathBuf,
    pub transcripts_dir: PathBuf,
    pub summaries_dir: PathBuf,
    pub index_dir: PathBuf,
    pub models_dir: PathBuf,
    pub tmp_dir: PathBuf,
}

impl Paths {
    pub fn new(data_dir_override: Option<PathBuf>) -> Result<Self> {
        let data_dir = if let Some(dir) = data_dir_override {
            dir
        } else {
            ProjectDirs::from("", "", "muesli")
                .ok_or_else(|| {
                    Error::Filesystem(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "Could not determine data directory",
                    ))
                })?
                .data_dir()
                .to_path_buf()
        };

        Ok(Paths {
            raw_dir: data_dir.join("raw"),
            transcripts_dir: data_dir.join("transcripts"),
            summaries_dir: data_dir.join("summaries"),
            index_dir: data_dir.join("index").join("tantivy"),
            models_dir: data_dir.join("models"),
            tmp_dir: data_dir.join("tmp"),
            data_dir,
        })
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in &[
            &self.raw_dir,
            &self.transcripts_dir,
            &self.summaries_dir,
            &self.index_dir,
            &self.models_dir,
            &self.tmp_dir,
        ] {
            fs::create_dir_all(dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = fs::Permissions::from_mode(0o700);
                fs::set_permissions(dir, perms)?;
            }
        }
        Ok(())
    }
}

pub fn write_atomic(path: &Path, content: &[u8], tmp_dir: &Path) -> Result<()> {
    use rand::Rng;

    // Create temp file
    let random: u32 = rand::thread_rng().gen();
    let tmp_path = tmp_dir.join(format!("{:x}.part", random));

    // Write to temp
    fs::write(&tmp_path, content)?;

    // Set permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&tmp_path, perms)?;
    }

    // Atomic rename
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&tmp_path, path)?;

    Ok(())
}

pub fn read_frontmatter(md_path: &Path) -> Result<Option<Frontmatter>> {
    if !md_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(md_path)?;

    // Look for YAML frontmatter (--- ... ---)
    if !content.starts_with("---\n") {
        return Ok(None);
    }

    if content.len() < 4 {
        return Ok(None);
    }
    let rest = &content[4..];
    if let Some(end_pos) = rest.find("\n---\n") {
        let yaml = &rest[..end_pos];
        let fm: Frontmatter = serde_yaml::from_str(yaml).map_err(|e| {
            Error::Filesystem(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to parse frontmatter: {}", e),
            ))
        })?;
        Ok(Some(fm))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_paths_new_with_override() {
        let temp = TempDir::new().unwrap();
        let paths = Paths::new(Some(temp.path().to_path_buf())).unwrap();
        assert_eq!(paths.data_dir, temp.path());
        assert_eq!(paths.raw_dir, temp.path().join("raw"));
    }

    #[test]
    fn test_ensure_dirs_creates_structure() {
        let temp = TempDir::new().unwrap();
        let paths = Paths::new(Some(temp.path().to_path_buf())).unwrap();
        paths.ensure_dirs().unwrap();

        assert!(paths.raw_dir.exists());
        assert!(paths.transcripts_dir.exists());
        assert!(paths.tmp_dir.exists());
    }

    #[test]
    #[cfg(unix)]
    fn test_ensure_dirs_sets_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let paths = Paths::new(Some(temp.path().to_path_buf())).unwrap();
        paths.ensure_dirs().unwrap();

        let perms = fs::metadata(&paths.raw_dir).unwrap().permissions();
        assert_eq!(
            perms.mode() & 0o777,
            0o700,
            "raw_dir should have 0o700 permissions"
        );

        let perms = fs::metadata(&paths.transcripts_dir).unwrap().permissions();
        assert_eq!(
            perms.mode() & 0o777,
            0o700,
            "transcripts_dir should have 0o700 permissions"
        );
    }
}

#[cfg(test)]
mod write_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_write_atomic_creates_file() {
        let temp = TempDir::new().unwrap();
        let paths = Paths::new(Some(temp.path().to_path_buf())).unwrap();
        paths.ensure_dirs().unwrap();

        let target = temp.path().join("test.txt");
        write_atomic(&target, b"hello", &paths.tmp_dir).unwrap();

        assert!(target.exists());
        assert_eq!(fs::read_to_string(&target).unwrap(), "hello");
    }

    #[test]
    #[cfg(unix)]
    fn test_write_atomic_sets_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let paths = Paths::new(Some(temp.path().to_path_buf())).unwrap();
        paths.ensure_dirs().unwrap();

        let target = temp.path().join("test.txt");
        write_atomic(&target, b"hello", &paths.tmp_dir).unwrap();

        let perms = fs::metadata(&target).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }
}

#[cfg(test)]
mod frontmatter_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_read_frontmatter_valid() {
        let temp = TempDir::new().unwrap();
        let md_path = temp.path().join("test.md");

        let content = r#"---
doc_id: "doc123"
source: "granola"
created_at: "2025-10-28T15:04:05Z"
title: "Test"
participants: []
generator: "muesli 1.0"
---

# Test Meeting
"#;
        fs::write(&md_path, content).unwrap();

        let fm = read_frontmatter(&md_path).unwrap();
        assert!(fm.is_some());
        assert_eq!(fm.unwrap().doc_id, "doc123");
    }

    #[test]
    fn test_read_frontmatter_missing_file() {
        let temp = TempDir::new().unwrap();
        let md_path = temp.path().join("missing.md");

        let fm = read_frontmatter(&md_path).unwrap();
        assert!(fm.is_none());
    }

    #[test]
    fn test_read_frontmatter_no_yaml() {
        let temp = TempDir::new().unwrap();
        let md_path = temp.path().join("test.md");
        fs::write(&md_path, "# Just content").unwrap();

        let fm = read_frontmatter(&md_path).unwrap();
        assert!(fm.is_none());
    }
}
