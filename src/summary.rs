// ABOUTME: AI summarization using OpenAI API
// ABOUTME: Chunks transcripts and generates meeting summaries

use crate::{Error, Result};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestUserMessageArgs,
        CreateChatCompletionRequestArgs,
    },
    Client,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

const DEFAULT_SUMMARY_PROMPT: &str = r#"You are an expert at turning messy transcripts into high-resolution, action-oriented summaries.

Given the transcript below, produce a structured summary with these sections:

1. Meeting Snapshot
2. Executive Summary (3–7 bullets)
3. Key Decisions (or "None")
4. Action Items (owner, task, due, priority, source)
5. Discussion Highlights by Topic
6. Risks, Concerns, and Open Questions
7. Nuanced Observations & Dynamics
8. Ambiguities, Gaps, and Things You Refused to Guess

Rules:
- Use headings and bullet points.
- Preserve important names, dates, and numbers accurately.
- Only use information from the transcript; label any inferences as "(inferred)".
- Be explicit when something is unclear, missing, or not specified.
- Ignore small talk; focus on substance."#;

#[derive(Serialize, Deserialize, Clone)]
pub struct SummaryConfig {
    pub model: String,
    pub context_window_chars: usize,
    pub custom_prompt: Option<String>,
}

impl Default for SummaryConfig {
    fn default() -> Self {
        Self {
            model: "gpt-5".to_string(),
            context_window_chars: 300_000, // ~400K tokens for GPT-5 API
            custom_prompt: None,
        }
    }
}

impl SummaryConfig {
    pub fn load(config_path: &Path) -> Result<Self> {
        if !config_path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(config_path)?;
        serde_json::from_str(&content).map_err(|e| {
            Error::Filesystem(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to parse summary config: {}", e),
            ))
        })
    }

    pub fn save(&self, config_path: &Path, tmp_dir: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        crate::storage::write_atomic(config_path, json.as_bytes(), tmp_dir)
    }

    pub fn prompt(&self) -> &str {
        self.custom_prompt
            .as_deref()
            .unwrap_or(DEFAULT_SUMMARY_PROMPT)
    }
}

pub async fn summarize_transcript(
    transcript: &str,
    api_key: &str,
    config: &SummaryConfig,
) -> Result<String> {
    let openai_config = OpenAIConfig::new().with_api_key(api_key);
    let client = Client::with_config(openai_config);

    // Chunk if too long (based on configured context window)
    let chunks = chunk_transcript(transcript, config.context_window_chars);

    if chunks.len() > 1 {
        // Multiple chunks - summarize each then combine
        let mut chunk_summaries = Vec::new();

        for (i, chunk) in chunks.iter().enumerate() {
            println!("Summarizing chunk {}/{}...", i + 1, chunks.len());
            let summary = summarize_chunk(&client, chunk, config).await?;
            chunk_summaries.push(summary);
        }

        // Combine summaries
        let combined = chunk_summaries.join("\n\n---\n\n");
        summarize_chunk(&client, &combined, config).await
    } else {
        // Single chunk
        summarize_chunk(&client, &chunks[0], config).await
    }
}

async fn summarize_chunk(
    client: &Client<OpenAIConfig>,
    text: &str,
    config: &SummaryConfig,
) -> Result<String> {
    // Build the full prompt with transcript embedded
    let full_prompt = format!(
        "{}\n\nTranscript:\n<<<TRANSCRIPT_START>>>\n{}\n<<<TRANSCRIPT_END>>>",
        config.prompt(),
        text
    );

    let messages = vec![ChatCompletionRequestMessage::User(
        ChatCompletionRequestUserMessageArgs::default()
            .content(full_prompt)
            .build()
            .map_err(|e| Error::Summarization(format!("Failed to build user message: {}", e)))?,
    )];

    let request = CreateChatCompletionRequestArgs::default()
        .model(&config.model)
        .messages(messages)
        .temperature(0.3)
        .build()
        .map_err(|e| Error::Summarization(format!("Failed to build request: {}", e)))?;

    let response = client
        .chat()
        .create(request)
        .await
        .map_err(|e| Error::Summarization(format!("OpenAI API error: {}", e)))?;

    response
        .choices
        .first()
        .and_then(|choice| choice.message.content.clone())
        .ok_or_else(|| Error::Summarization("No response from OpenAI".into()))
}

fn chunk_transcript(text: &str, max_chars: usize) -> Vec<String> {
    if text.len() <= max_chars {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current_chunk = String::new();

    for line in text.lines() {
        if current_chunk.len() + line.len() + 1 > max_chars && !current_chunk.is_empty() {
            chunks.push(current_chunk.clone());
            current_chunk.clear();
        }
        current_chunk.push_str(line);
        current_chunk.push('\n');
    }

    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    chunks
}

pub fn get_api_key_from_keychain() -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        use keyring::Entry;

        let entry = Entry::new("muesli", "openai_api_key")
            .map_err(|e| Error::Auth(format!("Failed to access keychain: {}", e)))?;

        entry.get_password().map_err(|e| {
            Error::Auth(format!(
                "OpenAI API key not found in keychain. Set it with: muesli set-api-key <key>. Error: {}",
                e
            ))
        })
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err(Error::Auth(
            "Keychain access only supported on macOS. Set OPENAI_API_KEY environment variable."
                .into(),
        ))
    }
}

pub fn set_api_key_in_keychain(api_key: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use keyring::Entry;

        let entry = Entry::new("muesli", "openai_api_key")
            .map_err(|e| Error::Auth(format!("Failed to access keychain: {}", e)))?;

        entry
            .set_password(api_key)
            .map_err(|e| Error::Auth(format!("Failed to store API key in keychain: {}", e)))?;

        println!("✅ OpenAI API key stored in keychain");
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err(Error::Auth(
            "Keychain access only supported on macOS. Set OPENAI_API_KEY environment variable."
                .into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_transcript_short() {
        let text = "Short transcript";
        let chunks = chunk_transcript(text, 1000);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].contains("Short transcript"));
    }

    #[test]
    fn test_chunk_transcript_long() {
        let text = "Line 1\n".repeat(200); // 1400 chars
        let chunks = chunk_transcript(&text, 500);
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.len() <= 500 || chunk.lines().count() == 1);
        }
    }

    #[test]
    fn test_summary_prompt_format() {
        assert!(DEFAULT_SUMMARY_PROMPT.contains("Meeting Snapshot"));
        assert!(DEFAULT_SUMMARY_PROMPT.contains("Action Items"));
        assert!(DEFAULT_SUMMARY_PROMPT.contains("Key Decisions"));
        assert!(DEFAULT_SUMMARY_PROMPT.contains("Ambiguities, Gaps"));
    }
}
