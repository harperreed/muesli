// ABOUTME: Model Context Protocol server implementation
// ABOUTME: Exposes muesli functionality as MCP tools for AI assistants

use crate::storage::Paths;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters, ServerHandler},
    model::{CallToolResult, Content, ErrorData as McpError},
    schemars::JsonSchema,
    tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone)]
pub struct MuesliMcpService {
    paths: Arc<Paths>,
    tool_router: ToolRouter<Self>,
}

impl MuesliMcpService {
    pub fn new(data_dir: Option<std::path::PathBuf>) -> crate::Result<Self> {
        let paths = Paths::new(data_dir)?;
        Ok(Self {
            paths: Arc::new(paths),
            tool_router: Self::tool_router(),
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
}

#[tool_handler]
impl ServerHandler for MuesliMcpService {}

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
