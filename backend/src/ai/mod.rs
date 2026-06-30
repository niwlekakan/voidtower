#![allow(dead_code)]

pub mod providers;
pub mod orchestrator;
pub mod router;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// ── Unified request/response types ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRequest {
    pub id: String,
    pub messages: Vec<AiMessage>,
    pub system_prompt: Option<String>,
    pub context: Option<serde_json::Value>,
    pub stream: bool,
}

impl AiRequest {
    pub fn new(messages: Vec<AiMessage>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            messages,
            system_prompt: None,
            context: None,
            stream: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCapabilities {
    pub reasoning: u8,  // 0-10
    pub coding: u8,
    pub tool_use: bool,
    pub vision: bool,
    pub local: bool,
    pub streaming: bool,
}

// ── Provider trait ────────────────────────────────────────────────────────────

#[async_trait]
pub trait AiProvider: Send + Sync {
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;
    fn capabilities(&self) -> AiCapabilities;

    /// Non-streaming completion — returns the full response text.
    async fn complete(&self, req: &AiRequest) -> std::result::Result<String, String>;

    /// Streaming completion — returns an SSE/NDJSON byte stream compatible with
    /// the OpenAI streaming format so the frontend can consume it unchanged.
    async fn stream(
        &self,
        req: &AiRequest,
    ) -> std::result::Result<reqwest::Response, String>;

    /// Quick connectivity check — returns Ok(()) if the provider is reachable.
    async fn health_check(&self) -> std::result::Result<(), String>;
}

// ── Provider config (persisted in DB) ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProviderConfig {
    pub id: String,
    pub kind: String,       // "odysseus" | "openai" | "anthropic" | "local"
    pub name: String,
    pub enabled: bool,
    pub base_url: Option<String>,
    pub api_key_ref: Option<String>,  // key name in secrets table
    pub model: Option<String>,
    pub priority: i64,      // lower = preferred
    pub created_at: i64,
    pub updated_at: i64,
}

// ── Re-exports ────────────────────────────────────────────────────────────────

pub use orchestrator::AiOrchestrator;
pub use providers::build_provider;
