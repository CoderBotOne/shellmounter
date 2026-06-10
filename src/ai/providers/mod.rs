pub mod openai;
pub mod anthropic;
pub mod ollama;

use crate::ai::chat::{Message, ChatState};
use anyhow::Result;
use async_trait::async_trait;
use std::pin::Pin;
use tokio::sync::mpsc;

/// Streamed chunk from a provider
#[derive(Debug, Clone)]
pub enum StreamChunk {
    TextDelta(String),
    ReasoningDelta(String),
    ToolCallStart {
        id: String,
        name: String,
    },
    ToolCallArgs(String),
    ToolCallEnd {
        id: String,
    },
    Error(String),
    Done(TokenUsage),
}

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
}

/// Trait for AI providers
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// Stream a chat completion
    async fn stream_chat(
        &self,
        model: &str,
        messages: &[ProviderMessage],
        system_prompt: Option<&str>,
        tools: &[ToolDef],
    ) -> Result<mpsc::Receiver<StreamChunk>>;

    /// List available models
    async fn list_models(&self) -> Result<Vec<String>>;
}

/// OpenAI-compatible message format
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProviderMessage {
    pub role: String,
    pub content: ProviderContent,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(untagged)]
pub enum ProviderContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_call_id: String,
        content: String,
    },
}

/// Tool definition for the provider API
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolDef {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDef,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}
