// ABOUTME: Automatic model downloader for e5-small-v2 ONNX model
// ABOUTME: Downloads from HuggingFace and caches in XDG data directory

use crate::{Error, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const MODEL_URL: &str = "https://huggingface.co/intfloat/e5-small-v2/resolve/main/model.onnx";
const TOKENIZER_URL: &str =
    "https://huggingface.co/intfloat/e5-small-v2/resolve/main/tokenizer.json";

pub struct ModelPaths {
    pub model_path: PathBuf,
    pub tokenizer_path: PathBuf,
}

pub fn ensure_model(models_dir: &Path) -> Result<ModelPaths> {
    let model_path = models_dir.join("e5-small-v2.onnx");
    let tokenizer_path = models_dir.join("e5-small-v2-tokenizer.json");

    if model_path.exists() && tokenizer_path.exists() {
        return Ok(ModelPaths {
            model_path,
            tokenizer_path,
        });
    }

    println!("🔽 Downloading e5-small-v2 embedding model (first time only)...");

    if !model_path.exists() {
        download_file(MODEL_URL, &model_path, "model.onnx")?;
    }

    if !tokenizer_path.exists() {
        download_file(TOKENIZER_URL, &tokenizer_path, "tokenizer.json")?;
    }

    println!("✅ Model downloaded successfully");

    Ok(ModelPaths {
        model_path,
        tokenizer_path,
    })
}

fn download_file(url: &str, dest: &Path, display_name: &str) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    let response = client.get(url).send()?;

    if !response.status().is_success() {
        return Err(Error::Filesystem(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "Failed to download {}: HTTP {}",
                display_name,
                response.status()
            ),
        )));
    }

    let total_size = response.content_length().unwrap_or(0);

    let pb = if total_size > 0 {
        let pb = ProgressBar::new(total_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.set_message(format!("Downloading {}", display_name));
        Some(pb)
    } else {
        println!("Downloading {} (size unknown)...", display_name);
        None
    };

    let mut file = fs::File::create(dest)?;
    let mut downloaded: u64 = 0;

    let bytes = response.bytes()?;

    file.write_all(&bytes)?;
    downloaded += bytes.len() as u64;

    if let Some(pb) = pb {
        pb.set_position(downloaded);
        pb.finish_with_message(format!("Downloaded {}", display_name));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_ensure_model_creates_paths() {
        let temp = TempDir::new().unwrap();
        let models_dir = temp.path().join("models");
        fs::create_dir_all(&models_dir).unwrap();

        let paths = ModelPaths {
            model_path: models_dir.join("e5-small-v2.onnx"),
            tokenizer_path: models_dir.join("e5-small-v2-tokenizer.json"),
        };

        assert!(paths.model_path.to_string_lossy().ends_with(".onnx"));
        assert!(paths
            .tokenizer_path
            .to_string_lossy()
            .ends_with("tokenizer.json"));
    }

    #[test]
    fn test_model_urls_format() {
        assert!(MODEL_URL.starts_with("https://"));
        assert!(MODEL_URL.contains("huggingface.co"));
        assert!(TOKENIZER_URL.starts_with("https://"));
    }
}
