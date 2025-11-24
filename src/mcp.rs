// ABOUTME: Model Context Protocol server implementation
// ABOUTME: Exposes muesli functionality as MCP tools for AI assistants

use crate::storage::Paths;
use rmcp::{
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
        ServerHandler,
    },
    model::{
        CallToolResult, Content, ErrorData as McpError, GetPromptRequestParam, GetPromptResult,
        ListPromptsResult, PaginatedRequestParam, PromptMessage, PromptMessageRole,
    },
    prompt, prompt_handler, prompt_router,
    schemars::JsonSchema,
    service::{RequestContext, RoleServer},
    tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone)]
pub struct MuesliMcpService {
    paths: Arc<Paths>,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

impl MuesliMcpService {
    pub fn new(data_dir: Option<std::path::PathBuf>) -> crate::Result<Self> {
        let paths = Paths::new(data_dir)?;
        Ok(Self {
            paths: Arc::new(paths),
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct ListDocumentsRequest {}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct SearchDocumentsRequest {
    /// Search query string
    query: String,
    /// Maximum number of results (default: 10)
    #[serde(default = "default_limit")]
    limit: usize,
    /// Use semantic search with embeddings
    #[serde(default)]
    semantic: bool,
}

fn default_limit() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct GetDocumentRequest {
    /// Document ID to retrieve
    doc_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct SyncDocumentsRequest {
    /// API token for authentication (optional, uses default auth if not provided)
    #[serde(default)]
    token: Option<String>,
    /// Force reindex of all documents without re-downloading (requires index feature)
    #[serde(default)]
    reindex: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct SummarizeDocumentRequest {
    /// Document ID to summarize
    doc_id: String,
    /// OpenAI API key (optional, uses keychain or env if not provided)
    #[serde(default)]
    api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct CompareDocumentsRequest {
    /// Array of document IDs to compare
    doc_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct FollowUpCheckRequest {
    /// Previous meeting document ID
    previous_doc_id: String,
    /// Current meeting document ID
    current_doc_id: String,
}

#[tool_router]
impl MuesliMcpService {
    #[tool(description = "List all meeting transcripts with metadata")]
    async fn list_documents(
        &self,
        _params: Parameters<ListDocumentsRequest>,
    ) -> std::result::Result<CallToolResult, McpError> {
        // Get list of all markdown files
        let entries = std::fs::read_dir(&self.paths.transcripts_dir).map_err(|e| {
            McpError::internal_error(format!("Failed to read directory: {}", e), None)
        })?;

        let mut docs = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| {
                McpError::internal_error(format!("Failed to read entry: {}", e), None)
            })?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }

            // Read frontmatter
            if let Ok(Some(fm)) = crate::storage::read_frontmatter(&path) {
                docs.push(serde_json::json!({
                    "doc_id": fm.doc_id,
                    "title": fm.title,
                    "created_at": fm.created_at.to_rfc3339(),
                    "path": path.display().to_string(),
                }));
            }
        }

        let json_text = serde_json::to_string_pretty(&docs)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize: {}", e), None))?;
        Ok(CallToolResult::success(vec![Content::text(json_text)]))
    }

    #[tool(description = "Search meeting transcripts by text query")]
    async fn search_documents(
        &self,
        #[cfg_attr(not(feature = "index"), allow(unused_variables))] params: Parameters<
            SearchDocumentsRequest,
        >,
    ) -> std::result::Result<CallToolResult, McpError> {
        #[cfg(feature = "index")]
        {
            let query = &params.0.query;
            let limit = params.0.limit;

            // Check if index exists
            if !self.paths.index_dir.exists() {
                return Err(McpError::internal_error(
                    "No index found. Run 'muesli sync' first to build the index.",
                    None,
                ));
            }

            // Perform search
            #[cfg(feature = "embeddings")]
            if params.0.semantic {
                let results = crate::embeddings::semantic_search(&self.paths, query, limit)
                    .map_err(|e| {
                        McpError::internal_error(format!("Semantic search failed: {}", e), None)
                    })?;

                let json_results: Vec<_> = results
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "doc_id": r.doc_id,
                            "title": r.title,
                            "date": r.date,
                            "score": r.score,
                            "path": r.path,
                        })
                    })
                    .collect();

                let json_text = serde_json::to_string_pretty(&json_results).map_err(|e| {
                    McpError::internal_error(format!("Failed to serialize: {}", e), None)
                })?;
                return Ok(CallToolResult::success(vec![Content::text(json_text)]));
            }

            // Text search
            let index =
                crate::index::text::create_or_open_index(&self.paths.index_dir).map_err(|e| {
                    McpError::internal_error(format!("Failed to open index: {}", e), None)
                })?;

            let results = crate::index::text::search(&index, query, limit)
                .map_err(|e| McpError::internal_error(format!("Search failed: {}", e), None))?;

            let json_results: Vec<_> = results
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "doc_id": r.doc_id,
                        "title": r.title,
                        "date": r.date,
                        "path": r.path,
                    })
                })
                .collect();

            let json_text = serde_json::to_string_pretty(&json_results).map_err(|e| {
                McpError::internal_error(format!("Failed to serialize: {}", e), None)
            })?;
            Ok(CallToolResult::success(vec![Content::text(json_text)]))
        }
        #[cfg(not(feature = "index"))]
        {
            Err(McpError::internal_error(
                "Search feature not enabled. Rebuild with --features index",
                None,
            ))
        }
    }

    #[tool(description = "Get full transcript content by document ID")]
    async fn get_document(
        &self,
        params: Parameters<GetDocumentRequest>,
    ) -> std::result::Result<CallToolResult, McpError> {
        // Find the markdown file
        let entries = std::fs::read_dir(&self.paths.transcripts_dir).map_err(|e| {
            McpError::internal_error(format!("Failed to read directory: {}", e), None)
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                McpError::internal_error(format!("Failed to read entry: {}", e), None)
            })?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }

            // Check if this is the right document
            if let Ok(Some(fm)) = crate::storage::read_frontmatter(&path) {
                if fm.doc_id == params.0.doc_id {
                    // Read full content
                    let content = std::fs::read_to_string(&path).map_err(|e| {
                        McpError::internal_error(format!("Failed to read file: {}", e), None)
                    })?;

                    return Ok(CallToolResult::success(vec![Content::text(content)]));
                }
            }
        }

        Err(McpError::invalid_params(
            format!("Document not found: {}", params.0.doc_id),
            None,
        ))
    }

    #[tool(description = "Sync new meeting transcripts from the API")]
    async fn sync_documents(
        &self,
        params: Parameters<SyncDocumentsRequest>,
    ) -> std::result::Result<CallToolResult, McpError> {
        // Create API client
        let token = if let Some(ref t) = params.0.token {
            t.clone()
        } else {
            crate::auth::resolve_token(None).map_err(|e| {
                McpError::internal_error(format!("Failed to resolve auth token: {}", e), None)
            })?
        };

        let client = crate::api::ApiClient::new(token, None).map_err(|e| {
            McpError::internal_error(format!("Failed to create API client: {}", e), None)
        })?;

        // Perform sync
        #[cfg(feature = "index")]
        {
            crate::sync::sync_all(&client, &self.paths, params.0.reindex)
                .map_err(|e| McpError::internal_error(format!("Sync failed: {}", e), None))?;
        }
        #[cfg(not(feature = "index"))]
        {
            crate::sync::sync_all(&client, &self.paths, false)
                .map_err(|e| McpError::internal_error(format!("Sync failed: {}", e), None))?;
        }

        Ok(CallToolResult::success(vec![Content::text(
            "Sync completed successfully".to_string(),
        )]))
    }

    #[tool(description = "Generate AI summary of a meeting transcript")]
    #[cfg(feature = "summaries")]
    async fn summarize_document(
        &self,
        params: Parameters<SummarizeDocumentRequest>,
    ) -> std::result::Result<CallToolResult, McpError> {
        // Find the markdown file
        let entries = std::fs::read_dir(&self.paths.transcripts_dir).map_err(|e| {
            McpError::internal_error(format!("Failed to read directory: {}", e), None)
        })?;

        let mut transcript_path = None;
        for entry in entries {
            let entry = entry.map_err(|e| {
                McpError::internal_error(format!("Failed to read entry: {}", e), None)
            })?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }

            if let Ok(Some(fm)) = crate::storage::read_frontmatter(&path) {
                if fm.doc_id == params.0.doc_id {
                    transcript_path = Some(path);
                    break;
                }
            }
        }

        let path = transcript_path.ok_or_else(|| {
            McpError::invalid_params(format!("Document not found: {}", params.0.doc_id), None)
        })?;

        // Read transcript content
        let content = std::fs::read_to_string(&path)
            .map_err(|e| McpError::internal_error(format!("Failed to read file: {}", e), None))?;

        // Extract body (skip frontmatter)
        let body = if content.starts_with("---\n") {
            content
                .split("---\n")
                .nth(2)
                .unwrap_or(&content)
                .to_string()
        } else {
            content
        };

        // Get API key
        let api_key = if let Some(ref key) = params.0.api_key {
            key.clone()
        } else {
            std::env::var("OPENAI_API_KEY")
                .or_else(|_| crate::summary::get_api_key_from_keychain())
                .map_err(|e| {
                    McpError::internal_error(format!("Failed to get OpenAI API key: {}", e), None)
                })?
        };

        // Load config
        let config_path = self.paths.data_dir.join("summary_config.json");
        let config = crate::summary::SummaryConfig::load(&config_path)
            .map_err(|e| McpError::internal_error(format!("Failed to load config: {}", e), None))?;

        // Generate summary
        let summary = crate::summary::summarize_transcript(&body, &api_key, &config)
            .await
            .map_err(|e| McpError::internal_error(format!("Summarization failed: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(summary)]))
    }
}

// Prompt implementations
#[prompt_router]
impl MuesliMcpService {
    #[prompt(
        name = "analyze_meeting",
        description = "Generate a structured analysis prompt for a meeting transcript"
    )]
    async fn analyze_meeting_prompt(
        &self,
        params: Parameters<GetDocumentRequest>,
    ) -> Vec<PromptMessage> {
        let doc_id = &params.0.doc_id;

        // Find and read the document
        if let Ok(entries) = std::fs::read_dir(&self.paths.transcripts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                if path.extension().and_then(|s| s.to_str()) != Some("md") {
                    continue;
                }

                if let Ok(Some(fm)) = crate::storage::read_frontmatter(&path) {
                    if &fm.doc_id == doc_id {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let prompt_text = format!(
                                r#"Please analyze this meeting transcript and provide:

1. **Key Decisions**: What decisions were made?
2. **Action Items**: What tasks were assigned and to whom?
3. **Discussion Topics**: What were the main topics discussed?
4. **Open Questions**: What questions remain unanswered?
5. **Next Steps**: What are the recommended next steps?

# Meeting Transcript

{}"#,
                                content
                            );

                            return vec![PromptMessage::new_text(
                                PromptMessageRole::User,
                                prompt_text,
                            )];
                        }
                    }
                }
            }
        }

        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!("Error: Document not found: {}", doc_id),
        )]
    }

    #[prompt(
        name = "compare_meetings",
        description = "Generate a prompt to compare multiple meeting transcripts"
    )]
    async fn compare_meetings_prompt(
        &self,
        params: Parameters<CompareDocumentsRequest>,
    ) -> Vec<PromptMessage> {
        let doc_ids = &params.0.doc_ids;
        let mut transcripts = Vec::new();

        for doc_id in doc_ids {
            if let Ok(entries) = std::fs::read_dir(&self.paths.transcripts_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();

                    if path.extension().and_then(|s| s.to_str()) != Some("md") {
                        continue;
                    }

                    if let Ok(Some(fm)) = crate::storage::read_frontmatter(&path) {
                        if &fm.doc_id == doc_id {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                transcripts.push(format!(
                                    "## Meeting: {}\n\n{}",
                                    fm.title.unwrap_or_else(|| "Untitled".to_string()),
                                    content
                                ));
                                break;
                            }
                        }
                    }
                }
            }
        }

        if transcripts.is_empty() {
            return vec![PromptMessage::new_text(
                PromptMessageRole::User,
                "Error: No matching documents found".to_string(),
            )];
        }

        let prompt_text = format!(
            r#"Please compare these meeting transcripts and provide:

1. **Common Themes**: What topics appear across multiple meetings?
2. **Progress Tracking**: How have discussed items evolved over time?
3. **Recurring Issues**: What problems keep coming up?
4. **Stakeholder Involvement**: Who participates in which discussions?
5. **Trend Analysis**: What patterns emerge across meetings?

# Transcripts

{}"#,
            transcripts.join("\n\n---\n\n")
        );

        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            prompt_text,
        )]
    }

    #[prompt(
        name = "extract_action_items",
        description = "Extract all action items with owners and deadlines from a meeting"
    )]
    async fn extract_action_items_prompt(
        &self,
        params: Parameters<GetDocumentRequest>,
    ) -> Vec<PromptMessage> {
        let doc_id = &params.0.doc_id;

        if let Ok(entries) = std::fs::read_dir(&self.paths.transcripts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                if path.extension().and_then(|s| s.to_str()) != Some("md") {
                    continue;
                }

                if let Ok(Some(fm)) = crate::storage::read_frontmatter(&path) {
                    if &fm.doc_id == doc_id {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let prompt_text = format!(
                                r#"Please extract all action items from this meeting transcript.

For each action item, identify:
1. **Task Description**: What needs to be done?
2. **Owner**: Who is responsible? (if mentioned)
3. **Deadline**: When is it due? (if mentioned)
4. **Status**: Was it marked as completed, in-progress, or new?
5. **Dependencies**: Does it depend on anything else?

Format as a structured list with clear sections.

# Meeting Transcript

{}"#,
                                content
                            );

                            return vec![PromptMessage::new_text(
                                PromptMessageRole::User,
                                prompt_text,
                            )];
                        }
                    }
                }
            }
        }

        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!("Error: Document not found: {}", doc_id),
        )]
    }

    #[prompt(
        name = "find_decisions",
        description = "Extract key decisions made in one or more meetings"
    )]
    async fn find_decisions_prompt(
        &self,
        params: Parameters<CompareDocumentsRequest>,
    ) -> Vec<PromptMessage> {
        let doc_ids = &params.0.doc_ids;
        let mut transcripts = Vec::new();

        for doc_id in doc_ids {
            if let Ok(entries) = std::fs::read_dir(&self.paths.transcripts_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();

                    if path.extension().and_then(|s| s.to_str()) != Some("md") {
                        continue;
                    }

                    if let Ok(Some(fm)) = crate::storage::read_frontmatter(&path) {
                        if &fm.doc_id == doc_id {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                transcripts.push(format!(
                                    "## Meeting: {} ({})\n\n{}",
                                    fm.title.unwrap_or_else(|| "Untitled".to_string()),
                                    fm.created_at.format("%Y-%m-%d"),
                                    content
                                ));
                                break;
                            }
                        }
                    }
                }
            }
        }

        if transcripts.is_empty() {
            return vec![PromptMessage::new_text(
                PromptMessageRole::User,
                "Error: No matching documents found".to_string(),
            )];
        }

        let prompt_text = format!(
            r#"Please identify all key decisions made in these meeting transcripts.

For each decision:
1. **Decision**: What was decided?
2. **Rationale**: Why was this decision made?
3. **Alternatives Considered**: What other options were discussed?
4. **Impact**: Who/what does this affect?
5. **Date**: When was this decided?
6. **Follow-up Required**: Any actions needed?

Group decisions by theme or category if multiple meetings are provided.

# Transcripts

{}"#,
            transcripts.join("\n\n---\n\n")
        );

        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            prompt_text,
        )]
    }

    #[prompt(
        name = "follow_up_check",
        description = "Compare two meetings to check if action items were completed and decisions implemented"
    )]
    async fn follow_up_check_prompt(
        &self,
        params: Parameters<FollowUpCheckRequest>,
    ) -> Vec<PromptMessage> {
        let mut transcripts = Vec::new();

        // Load both meetings
        for doc_id in [&params.0.previous_doc_id, &params.0.current_doc_id] {
            if let Ok(entries) = std::fs::read_dir(&self.paths.transcripts_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();

                    if path.extension().and_then(|s| s.to_str()) != Some("md") {
                        continue;
                    }

                    if let Ok(Some(fm)) = crate::storage::read_frontmatter(&path) {
                        if &fm.doc_id == doc_id {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                let label = if doc_id == &params.0.previous_doc_id {
                                    "Previous"
                                } else {
                                    "Current"
                                };
                                transcripts.push(format!(
                                    "## {} Meeting: {} ({})\n\n{}",
                                    label,
                                    fm.title.unwrap_or_else(|| "Untitled".to_string()),
                                    fm.created_at.format("%Y-%m-%d"),
                                    content
                                ));
                                break;
                            }
                        }
                    }
                }
            }
        }

        if transcripts.len() < 2 {
            return vec![PromptMessage::new_text(
                PromptMessageRole::User,
                "Error: Could not find both meetings".to_string(),
            )];
        }

        let prompt_text = format!(
            r#"Please compare these two meetings to track progress and accountability.

Analyze:

1. **Action Items Follow-Through**:
   - Which action items from the previous meeting were completed?
   - Which are still pending or in progress?
   - Which weren't mentioned (possibly forgotten)?

2. **Decision Implementation**:
   - Were decisions from the previous meeting implemented?
   - Were any decisions reversed or modified?
   - What was the outcome?

3. **New vs. Recurring Issues**:
   - What new topics emerged?
   - What issues came up again (might indicate systemic problems)?

4. **Progress Assessment**:
   - Overall, is the team making progress on goals?
   - Are there blockers preventing forward movement?

5. **Accountability**:
   - Who followed through on commitments?
   - Where were there gaps in ownership?

# Meetings

{}"#,
            transcripts.join("\n\n---\n\n")
        );

        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            prompt_text,
        )]
    }
}

#[tool_handler(router = self.tool_router)]
#[prompt_handler(router = self.prompt_router)]
impl ServerHandler for MuesliMcpService {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        use rmcp::model::{Implementation, PromptsCapability, ServerCapabilities, ToolsCapability};

        rmcp::model::ServerInfo {
            protocol_version: rmcp::model::ProtocolVersion::default(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability::default()),
                prompts: Some(PromptsCapability::default()),
                ..Default::default()
            },
            server_info: Implementation {
                name: "muesli".to_string(),
                title: Some("Muesli Meeting Transcript Manager".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Muesli MCP server for managing meeting transcripts. \
                 Use tools to list, search, sync, and summarize transcripts. \
                 Use prompts for structured analysis of meetings."
                    .to_string(),
            ),
        }
    }
}

pub async fn serve_mcp(data_dir: Option<std::path::PathBuf>) -> crate::Result<()> {
    use rmcp::{transport::stdio, ServiceExt};

    let service = MuesliMcpService::new(data_dir)?;
    let server = service.serve(stdio()).await.map_err(|e| {
        crate::Error::Filesystem(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("MCP server failed: {}", e),
        ))
    })?;

    server.waiting().await.map_err(|e| {
        crate::Error::Filesystem(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("MCP server error: {}", e),
        ))
    })?;

    Ok(())
}
