use super::{AiProvider, ProviderMessage, StreamChunk, TokenUsage, ToolDef};
use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

/// Anthropic Claude provider — reuses the OpenAI-compatible wrapper.
/// Most Anthropic endpoints now support the OpenAI-compatible API via /v1.
pub struct AnthropicProvider {
    inner: super::openai::OpenAiProvider,
}

impl AnthropicProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            inner: super::openai::OpenAiProvider::new_compatible(
                api_key,
                "https://api.anthropic.com/v1".into(),
            ),
        }
    }
}

#[async_trait]
impl AiProvider for AnthropicProvider {
    async fn stream_chat(
        &self,
        model: &str,
        messages: &[ProviderMessage],
        system_prompt: Option<&str>,
        tools: &[ToolDef],
    ) -> Result<mpsc::Receiver<StreamChunk>> {
        self.inner
            .stream_chat(model, messages, system_prompt, tools)
            .await
    }

    async fn list_models(&self) -> Result<Vec<String>> {
        self.inner.list_models().await
    }
}
