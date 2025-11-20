// ABOUTME: AI summarization using OpenAI API
// ABOUTME: Chunks transcripts and generates meeting summaries

use crate::{Error, Result};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
    },
    Client,
};

const SUMMARY_PROMPT: &str = r#"You are an expert at summarizing meeting transcripts.

Summarize the following meeting transcript in a clear, structured format:

1. **Key Topics** (3-5 bullet points)
2. **Action Items** (if any, with who/what)
3. **Decisions Made** (if any)
4. **Follow-ups** (if any)

Be concise but comprehensive. Focus on actionable insights."#;

pub async fn summarize_transcript(transcript: &str, api_key: &str) -> Result<String> {
    let config = OpenAIConfig::new().with_api_key(api_key);
    let client = Client::with_config(config);

    // Chunk if too long (OpenAI has token limits)
    let chunks = chunk_transcript(transcript, 6000);

    if chunks.len() > 1 {
        // Multiple chunks - summarize each then combine
        let mut chunk_summaries = Vec::new();

        for (i, chunk) in chunks.iter().enumerate() {
            println!("Summarizing chunk {}/{}...", i + 1, chunks.len());
            let summary = summarize_chunk(&client, chunk).await?;
            chunk_summaries.push(summary);
        }

        // Combine summaries
        let combined = chunk_summaries.join("\n\n---\n\n");
        summarize_chunk(&client, &combined).await
    } else {
        // Single chunk
        summarize_chunk(&client, &chunks[0]).await
    }
}

async fn summarize_chunk(client: &Client<OpenAIConfig>, text: &str) -> Result<String> {
    let messages = vec![
        ChatCompletionRequestMessage::System(
            ChatCompletionRequestSystemMessageArgs::default()
                .content(SUMMARY_PROMPT)
                .build()
                .map_err(|e| {
                    Error::Summarization(format!("Failed to build system message: {}", e))
                })?,
        ),
        ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessageArgs::default()
                .content(text)
                .build()
                .map_err(|e| {
                    Error::Summarization(format!("Failed to build user message: {}", e))
                })?,
        ),
    ];

    let request = CreateChatCompletionRequestArgs::default()
        .model("gpt-4o-mini")
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
        assert!(SUMMARY_PROMPT.contains("Key Topics"));
        assert!(SUMMARY_PROMPT.contains("Action Items"));
        assert!(SUMMARY_PROMPT.contains("Decisions Made"));
    }
}
